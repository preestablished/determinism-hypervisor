//! Shared rig for dh-worker's live joint tests (snapshot_engine,
//! restore_engine, m4_transparency): the KVM probe, the REAL in-process
//! snapshot-store (R12; docs/decisions/snapstore-server-for-tests.md),
//! and the canonical 4-device test bus. Every test target is x86_64-gated
//! at its crate root, so this module never compiles elsewhere.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use dh_devices::clock::PvClock;
use dh_devices::entropy::PvEntropy;
use dh_devices::pad::PvPad;
use dh_devices::serial::DebugSerial;
use dh_devices::MmioBus;
use snapstore_client::blocking::SnapstoreClient as BlockingClient;
use snapstore_client::Transport;
use snapstore_server::build_server::{serve_for_tests, ServerHandle};
use snapstore_server::config::{PageChannelConfig, ServerConfig};
use tempfile::TempDir;

// Not every test target uses every helper (replay_engine builds its own
// pad+entropy bus).
#[allow(dead_code)]
pub const CLOCK_BASE: u64 = 0xD000_2000;

#[allow(dead_code)]
pub const DH_M9_BZIMAGE: &str = "DH_M9_BZIMAGE";
#[allow(dead_code)]
pub const DH_M9_INITRAMFS: &str = "DH_M9_INITRAMFS";
#[allow(dead_code)]
pub const DH_M9_BASE_IMAGE: &str = "DH_M9_BASE_IMAGE";
#[allow(dead_code)]
pub const DH_M9_GAME_IMAGE: &str = "DH_M9_GAME_IMAGE";
#[allow(dead_code)]
pub const DH_M9_IMAGE_CACHE: &str = "DH_M9_IMAGE_CACHE";

#[allow(dead_code)]
pub const M9_LINUX_ARTIFACT_ENV_VARS: [&str; 5] = [
    DH_M9_BZIMAGE,
    DH_M9_INITRAMFS,
    DH_M9_BASE_IMAGE,
    DH_M9_GAME_IMAGE,
    DH_M9_IMAGE_CACHE,
];

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub struct M9LinuxArtifacts {
    pub bzimage: PathBuf,
    pub initramfs: PathBuf,
    pub base_image: PathBuf,
    pub game_image: PathBuf,
    pub image_cache: PathBuf,
}

#[allow(dead_code)]
impl M9LinuxArtifacts {
    pub fn from_env_required(test_name: &str) -> Result<Self, String> {
        let artifacts = Self::from_lookup(test_name, |name| std::env::var_os(name))?;
        artifacts.validate_paths()?;
        Ok(artifacts)
    }

    pub fn from_lookup<F>(test_name: &str, mut lookup: F) -> Result<Self, String>
    where
        F: FnMut(&str) -> Option<OsString>,
    {
        let mut missing = Vec::new();
        let mut required = |name: &'static str| -> Option<PathBuf> {
            match lookup(name) {
                Some(value) if !value.is_empty() => Some(PathBuf::from(value)),
                _ => {
                    missing.push(name);
                    None
                }
            }
        };

        let bzimage = required(DH_M9_BZIMAGE);
        let initramfs = required(DH_M9_INITRAMFS);
        let base_image = required(DH_M9_BASE_IMAGE);
        let game_image = required(DH_M9_GAME_IMAGE);
        let image_cache = required(DH_M9_IMAGE_CACHE);

        if !missing.is_empty() {
            return Err(format!(
                "M9 Linux acceptance test {test_name:?} requested, but missing required artifact env vars: {}. Set all of {}. *_ALLOW_SKIP=1 is not accepted for final M9 gates.",
                missing.join(", "),
                M9_LINUX_ARTIFACT_ENV_VARS.join(", ")
            ));
        }

        Ok(Self {
            bzimage: bzimage.expect("missing handled above"),
            initramfs: initramfs.expect("missing handled above"),
            base_image: base_image.expect("missing handled above"),
            game_image: game_image.expect("missing handled above"),
            image_cache: image_cache.expect("missing handled above"),
        })
    }

    fn validate_paths(&self) -> Result<(), String> {
        require_regular_file(DH_M9_BZIMAGE, &self.bzimage)?;
        require_regular_file(DH_M9_INITRAMFS, &self.initramfs)?;
        require_regular_file(DH_M9_BASE_IMAGE, &self.base_image)?;
        require_regular_file(DH_M9_GAME_IMAGE, &self.game_image)?;
        require_directory(DH_M9_IMAGE_CACHE, &self.image_cache)?;
        Ok(())
    }
}

#[allow(dead_code)]
fn require_regular_file(env_name: &str, path: &Path) -> Result<(), String> {
    let meta = std::fs::metadata(path)
        .map_err(|e| format!("{env_name}={} is not readable: {e}", path.display()))?;
    if !meta.is_file() {
        return Err(format!(
            "{env_name}={} must name a regular file",
            path.display()
        ));
    }
    Ok(())
}

#[allow(dead_code)]
fn require_directory(env_name: &str, path: &Path) -> Result<(), String> {
    let meta = std::fs::metadata(path)
        .map_err(|e| format!("{env_name}={} is not readable: {e}", path.display()))?;
    if !meta.is_dir() {
        return Err(format!(
            "{env_name}={} must name an existing directory",
            path.display()
        ));
    }
    Ok(())
}

#[allow(dead_code)]
pub fn kvm_available() -> bool {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/kvm")
        .is_ok()
}

/// The current thread id — counter overflow routing needs the real tid.
/// (Hoisted from four per-target copies, iteration-101 review.)
#[allow(dead_code)]
pub fn gettid() -> i32 {
    // SAFETY: argless syscall.
    #[allow(unsafe_code)]
    unsafe {
        libc::syscall(libc::SYS_gettid) as i32
    }
}

/// GuestMem adapter over a slot's memory map — the DeviceRail seam the
/// live joint tests share. (Hoisted from three per-target copies,
/// iteration-101 review.)
#[derive(Clone)]
#[allow(dead_code)]
pub struct VmMem(pub vm_memory::GuestMemoryMmap<()>);

impl dh_devices::ctx::GuestMem for VmMem {
    fn read(&self, gpa: u64, out: &mut [u8]) -> Result<(), dh_devices::ctx::MemError> {
        use vm_memory::Bytes;
        self.0
            .read_slice(out, vm_memory::GuestAddress(gpa))
            .map_err(|_| dh_devices::ctx::MemError)
    }
    fn write(&mut self, gpa: u64, data: &[u8]) -> Result<(), dh_devices::ctx::MemError> {
        use vm_memory::Bytes;
        self.0
            .write_slice(data, vm_memory::GuestAddress(gpa))
            .map_err(|_| dh_devices::ctx::MemError)
    }
}

impl detguest_host::GuestMem for VmMem {
    fn read(&self, gpa: u64, out: &mut [u8]) -> Result<(), detguest_host::MemError> {
        use vm_memory::Bytes;
        self.0
            .read_slice(out, vm_memory::GuestAddress(gpa))
            .map_err(|_| detguest_host::MemError::Unmapped {
                gpa,
                len: out.len(),
            })
    }

    fn write(&mut self, gpa: u64, data: &[u8]) -> Result<(), detguest_host::MemError> {
        use vm_memory::Bytes;
        self.0
            .write_slice(data, vm_memory::GuestAddress(gpa))
            .map_err(|_| detguest_host::MemError::Unmapped {
                gpa,
                len: data.len(),
            })
    }
}

/// Real store on a side runtime; the engines stay synchronous and reach
/// it via the blocking facade (the production shape).
// Each test target compiles this module independently; not every target
// uses every helper (store_durability owns its data root via
// `spawn_store_at` directly).
#[allow(dead_code)]
pub fn spawn_store_blocking() -> (
    tokio::runtime::Runtime,
    ServerHandle,
    BlockingClient,
    TempDir,
) {
    let dir = TempDir::new().expect("tempdir");
    let (rt, handle, client) = spawn_store_at(dir.path().to_path_buf(), "snapstore.sock");
    (rt, handle, client, dir)
}

/// Spawn a server instance over a CALLER-OWNED data root — the seam the
/// durability acceptance uses to restart the store over the same bytes.
/// `sock_name` keeps each instance's UDS distinct (no reliance on the
/// previous instance's socket file being unlinked).
pub fn spawn_store_at(
    data_root: std::path::PathBuf,
    sock_name: &str,
) -> (tokio::runtime::Runtime, ServerHandle, BlockingClient) {
    spawn_store_at_inner(data_root, sock_name, false)
}

#[allow(dead_code)]
pub fn spawn_store_at_with_corrupt_page_channel(
    data_root: std::path::PathBuf,
    sock_name: &str,
) -> (tokio::runtime::Runtime, ServerHandle, BlockingClient) {
    spawn_store_at_inner(data_root, sock_name, true)
}

fn spawn_store_at_inner(
    data_root: std::path::PathBuf,
    sock_name: &str,
    corrupt_page_channel: bool,
) -> (tokio::runtime::Runtime, ServerHandle, BlockingClient) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    let uds_path = data_root.join(sock_name);
    let page_channel_path = data_root.join(format!("{sock_name}.pages"));
    let config = ServerConfig {
        data_root: data_root.clone(),
        grpc_tcp_addr: "127.0.0.1:0".parse().expect("addr"),
        grpc_uds_path: Some(uds_path.clone()),
        page_channel_path: Some(page_channel_path.clone()),
        http_addr: "127.0.0.1:0".parse().expect("addr"),
        pagestore: Default::default(),
        meta: Default::default(),
        page_channel: PageChannelConfig {
            ingest_queue_pages: None,
            corrupt_cross_check_for_test: corrupt_page_channel.then_some(true),
        },
    };
    let (handle, uds) = rt
        .block_on(serve_for_tests(config))
        .expect("serve_for_tests");
    // Readiness probe (same shape as tests/determinism/store_joint.rs).
    let mut client = None;
    for _ in 0..50 {
        #[cfg(target_os = "linux")]
        if !page_channel_path.exists() {
            std::thread::sleep(std::time::Duration::from_millis(10));
            continue;
        }
        match BlockingClient::connect(Transport::Auto {
            uds_path: uds.clone(),
            tcp_addr: "http://127.0.0.1:1".into(),
            page_channel_path: Some(page_channel_path.clone()),
        }) {
            Ok(c) => {
                client = Some(c);
                break;
            }
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }
    (rt, handle, client.expect("store ready"))
}

/// The canonical joint-test bus: pad, clock, entropy, serial — one device
/// per DHSNAP tag the engines exercise, deterministic base order.
#[allow(dead_code)]
pub fn test_bus() -> MmioBus {
    let mut bus = MmioBus::new();
    bus.register(0xD000_1000, Box::new(PvPad::new())).unwrap();
    bus.register(CLOCK_BASE, Box::new(PvClock::new(1, 1)))
        .unwrap();
    bus.register(0xD000_3000, Box::new(PvEntropy::new()))
        .unwrap();
    bus.register(0xD000_6000, Box::new(DebugSerial::new()))
        .unwrap();
    bus
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m9_artifact_lookup_reports_all_missing_vars() {
        let err = M9LinuxArtifacts::from_lookup("linux-ready", |_| None).unwrap_err();
        for name in M9_LINUX_ARTIFACT_ENV_VARS {
            assert!(err.contains(name), "error did not mention {name}: {err}");
        }
        assert!(err.contains("*_ALLOW_SKIP=1"));
    }

    #[test]
    fn m9_artifact_lookup_accepts_all_required_vars() {
        let artifacts = M9LinuxArtifacts::from_lookup("linux-ready", |name| {
            Some(OsString::from(format!("/tmp/{name}")))
        })
        .unwrap();
        assert_eq!(artifacts.bzimage, PathBuf::from("/tmp/DH_M9_BZIMAGE"));
        assert_eq!(
            artifacts.image_cache,
            PathBuf::from("/tmp/DH_M9_IMAGE_CACHE")
        );
    }
}
