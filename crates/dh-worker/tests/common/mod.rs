//! Shared rig for dh-worker's live joint tests (snapshot_engine,
//! restore_engine, m4_transparency): the KVM probe, the REAL in-process
//! snapshot-store (R12; docs/decisions/snapstore-server-for-tests.md),
//! and the canonical 4-device test bus. Every test target is x86_64-gated
//! at its crate root, so this module never compiles elsewhere.

use std::ffi::OsString;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

use dh_devices::clock::PvClock;
use dh_devices::entropy::PvEntropy;
use dh_devices::pad::PvPad;
use dh_devices::serial::DebugSerial;
use dh_devices::MmioBus;
use snapstore_client::blocking::SnapstoreClient as BlockingClient;
use snapstore_client::Transport;
use snapstore_manifest::input_log::InputLogContainer;
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
pub const DH_M9_GUEST: &str = "DH_M9_GUEST";

#[allow(dead_code)]
pub const M9_LINUX_ARTIFACT_ENV_VARS: [&str; 5] = [
    DH_M9_BZIMAGE,
    DH_M9_INITRAMFS,
    DH_M9_BASE_IMAGE,
    DH_M9_GAME_IMAGE,
    DH_M9_IMAGE_CACHE,
];

#[allow(dead_code)]
pub const DH_M9_ALLOW_SKIP: &str = "DH_M9_ALLOW_SKIP";
#[allow(dead_code)]
pub const M9_LINUX_MEM_BYTES: u64 = 128 * 1024 * 1024;
#[allow(dead_code)]
pub const M9_READY_HARD_CAP: u64 = 10_000_000_000;

#[allow(dead_code)]
pub type TestResult<T> = Result<T, String>;

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

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct M9CachedHashes {
    pub bzimage: [u8; 32],
    pub initramfs: [u8; 32],
    pub base_image: [u8; 32],
    pub game_image: [u8; 32],
}

#[allow(dead_code)]
pub struct M9LinuxReady {
    pub _store_rt: tokio::runtime::Runtime,
    pub _store_handle: ServerHandle,
    pub _store_dir: TempDir,
    pub store: BlockingClient,
    pub svc: dh_worker::service::WorkerService,
    pub config_hash: [u8; 32],
    pub initial_snapshot: dh_proto::v1::SnapshotRef,
    pub ready_snapshot: dh_proto::v1::TakeSnapshotResponse,
    pub ready_snapshot_ref: dh_proto::v1::SnapshotRef,
    pub ready_state_hash: Vec<u8>,
    pub lease: dh_proto::v1::Lease,
}

#[allow(dead_code)]
pub fn m9_allow_skip() -> bool {
    std::env::var(DH_M9_ALLOW_SKIP).as_deref() == Ok("1")
}

#[allow(dead_code)]
pub fn m9_artifacts(test_name: &str) -> TestResult<Option<M9LinuxArtifacts>> {
    match M9LinuxArtifacts::from_env_required(test_name) {
        Ok(artifacts) => Ok(Some(artifacts)),
        Err(e) if m9_allow_skip() => {
            eprintln!("skipping M9 Linux acceptance {test_name}: {e}");
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

#[allow(dead_code)]
pub fn reject_unimplemented_m9_linux_gate(test_name: &str, required_evidence: &str) {
    let guest = std::env::var(DH_M9_GUEST);
    let linux_env = guest.as_deref() == Ok("linux");
    let linux_filter = std::env::args().any(|arg| arg == "linux");
    if linux_env || linux_filter {
        panic!(
            "{test_name} selected Linux evidence, but no real Linux implementation exists yet. Required evidence: {required_evidence}. Do not count a zero-test or guard-only run as M9 Linux acceptance."
        );
    }
    match guest {
        Ok(value) => {
            eprintln!(
                "skipping {test_name} Linux guard because {DH_M9_GUEST}={value:?}, not \"linux\""
            );
        }
        Err(_) => {
            eprintln!("skipping {test_name} Linux guard because {DH_M9_GUEST} is not set");
        }
    }
}

#[allow(dead_code)]
pub fn m9_masked_cpuid_table(
    test_name: &str,
) -> TestResult<Option<Vec<dh_vmm::config::CpuidLeaf>>> {
    match dh_vmm::kvm::KvmSystem::open() {
        Ok(sys) if sys.dirty_ring => sys
            .masked_cpuid_table()
            .map(Some)
            .map_err(|e| format!("{test_name}: masked CPUID table: {e:?}")),
        Ok(_) if m9_allow_skip() => {
            eprintln!("skipping M9 Linux acceptance {test_name}: KVM dirty ring unavailable");
            Ok(None)
        }
        Ok(_) => Err(format!("{test_name}: KVM dirty ring unavailable")),
        Err(e) if m9_allow_skip() => {
            eprintln!("skipping M9 Linux acceptance {test_name}: KVM unavailable: {e:?}");
            Ok(None)
        }
        Err(e) => Err(format!("{test_name}: KVM unavailable: {e:?}")),
    }
}

#[allow(dead_code)]
pub fn hash_file(path: &Path) -> TestResult<[u8; 32]> {
    let mut file =
        std::fs::File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("read {}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(*hasher.finalize().as_bytes())
}

#[allow(dead_code)]
pub fn ensure_cache_entry(source: &Path, cache_root: &Path) -> TestResult<[u8; 32]> {
    let hash = hash_file(source)?;
    let key = dh_worker::image_resolver::cache_key(&hash);
    let dest = cache_root.join(&key);
    if dest.exists() {
        if hash_file(&dest)? == hash {
            return Ok(hash);
        }
        return Err(format!(
            "existing image cache entry {} does not match key {}",
            dest.display(),
            key
        ));
    }

    match std::fs::hard_link(source, &dest) {
        Ok(()) => Ok(hash),
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {
            if hash_file(&dest)? == hash {
                Ok(hash)
            } else {
                Err(format!(
                    "concurrent image cache entry {} does not match key {}",
                    dest.display(),
                    key
                ))
            }
        }
        Err(_) => {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default();
            let tmp = cache_root.join(format!("{}.{}.{}.tmp", key, std::process::id(), nonce));
            let _ = std::fs::remove_file(&tmp);
            std::fs::copy(source, &tmp).map_err(|e| {
                format!(
                    "copy {} to temporary image cache entry {}: {e}",
                    source.display(),
                    tmp.display()
                )
            })?;
            if hash_file(&tmp)? != hash {
                let _ = std::fs::remove_file(&tmp);
                return Err(format!(
                    "temporary image cache entry {} hash mismatch",
                    tmp.display()
                ));
            }
            let publish = match std::fs::hard_link(&tmp, &dest) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                    if hash_file(&dest)? == hash {
                        Ok(())
                    } else {
                        Err(format!(
                            "concurrent image cache entry {} does not match key {}",
                            dest.display(),
                            key
                        ))
                    }
                }
                Err(e) => Err(format!(
                    "publish temporary image cache entry {} to {}: {e}",
                    tmp.display(),
                    dest.display()
                )),
            };
            let cleanup = std::fs::remove_file(&tmp)
                .map_err(|e| format!("remove temporary image cache entry {}: {e}", tmp.display()));
            publish?;
            cleanup?;
            Ok(hash)
        }
    }
}

#[allow(dead_code)]
pub fn populate_m9_image_cache(artifacts: &M9LinuxArtifacts) -> TestResult<M9CachedHashes> {
    Ok(M9CachedHashes {
        bzimage: ensure_cache_entry(&artifacts.bzimage, &artifacts.image_cache)?,
        initramfs: ensure_cache_entry(&artifacts.initramfs, &artifacts.image_cache)?,
        base_image: ensure_cache_entry(&artifacts.base_image, &artifacts.image_cache)?,
        game_image: ensure_cache_entry(&artifacts.game_image, &artifacts.image_cache)?,
    })
}

#[allow(dead_code)]
pub fn m9_linux_machine_config(
    hashes: &M9CachedHashes,
    cpuid_table: Vec<dh_vmm::config::CpuidLeaf>,
) -> dh_vmm::config::MachineConfig {
    let mut config = dh_vmm::config::MachineConfig::new(
        M9_LINUX_MEM_BYTES,
        hashes.game_image,
        dh_vmm::config::BootSpec::BzImage {
            kernel_hash: hashes.bzimage,
            initramfs_hash: hashes.initramfs,
            cmdline: dh_vmm::config::canonicalize_bzimage_cmdline_extras(b"quiet")
                .expect("M9 allows quiet as an append-only cmdline extra"),
        },
    );
    config.cpuid_table = cpuid_table;
    config.device_set = vec![
        dh_devices::detchannel::DEVICE_ID_DETCHANNEL,
        dh_devices::clock::DEVICE_ID_PV_CLOCK,
        dh_devices::pad::DEVICE_ID_PV_PAD,
        dh_devices::entropy::DEVICE_ID_PV_ENTROPY,
        dh_devices::blk::DEVICE_ID_PV_BLK,
        dh_devices::serial::DEVICE_ID_DEBUG_SERIAL,
    ];
    config
}

#[allow(dead_code)]
pub fn m9_worker_config(
    test_name: &str,
    slots: usize,
    image_cache_dir: PathBuf,
    snapstore: snapstore_client::Transport,
) -> dh_worker::service::WorkerConfig {
    let slot_cores = (0..slots)
        .map(|slot| u32::try_from(slot).expect("slot core id fits u32"))
        .collect();
    m9_worker_config_with_slot_cores(test_name, slot_cores, image_cache_dir, snapstore)
}

#[allow(dead_code)]
pub fn m9_worker_config_with_slot_cores(
    test_name: &str,
    slot_cores: Vec<u32>,
    image_cache_dir: PathBuf,
    snapstore: snapstore_client::Transport,
) -> dh_worker::service::WorkerConfig {
    dh_worker::service::WorkerConfig {
        worker_id: test_name.into(),
        slot_cores,
        lease_policy: dh_worker::slot_manager::LeasePolicy::default(),
        class: dh_proto::v1::DeterminismClass {
            cpu_model: "m9-test-cpu".into(),
            microcode: "m9-test-ucode".into(),
            host_kernel: "m9-test-kernel".into(),
            vmm_version: "m9-test-vmm".into(),
        },
        preflight: dh_worker::service::PreflightHealth::skipped(format!(
            "{test_name} acceptance harness"
        )),
        image_cache_dir,
        snapstore: Some(snapstore),
        bisection_checkpoints: dh_worker::service::BisectionCheckpointConfig::default(),
    }
}

#[allow(dead_code)]
pub fn m9_linux_ready_snapshot(test_name: &str, slots: usize) -> TestResult<Option<M9LinuxReady>> {
    m9_linux_ready_snapshot_with_config(test_name, slots, |_| {})
}

#[allow(dead_code)]
pub fn m9_linux_ready_snapshot_with_config<F>(
    test_name: &str,
    slots: usize,
    configure: F,
) -> TestResult<Option<M9LinuxReady>>
where
    F: FnOnce(&mut dh_vmm::config::MachineConfig),
{
    let slot_cores = (0..slots)
        .map(|slot| u32::try_from(slot).expect("slot core id fits u32"))
        .collect();
    m9_linux_ready_snapshot_with_slot_cores_and_config(test_name, slot_cores, configure)
}

#[allow(dead_code)]
pub fn m9_linux_ready_snapshot_with_slot_cores_and_config<F>(
    test_name: &str,
    slot_cores: Vec<u32>,
    configure: F,
) -> TestResult<Option<M9LinuxReady>>
where
    F: FnOnce(&mut dh_vmm::config::MachineConfig),
{
    use dh_proto::v1 as proto;
    use dh_proto::v1::hypervisor_worker_server::HypervisorWorker;
    use tonic::Request;

    if slot_cores.is_empty() {
        return Err(format!(
            "{test_name}: m9 Linux READY setup requires at least one slot core"
        ));
    }
    let Some(artifacts) = m9_artifacts(test_name)? else {
        return Ok(None);
    };
    let Some(cpuid_table) = m9_masked_cpuid_table(test_name)? else {
        return Ok(None);
    };
    let hashes = populate_m9_image_cache(&artifacts)?;
    let mut config = m9_linux_machine_config(&hashes, cpuid_table);
    configure(&mut config);
    let config_hash = config
        .config_hash()
        .map_err(|e| format!("MachineConfig hash: {e:?}"))?;

    let store_dir = TempDir::new().map_err(|e| format!("snapstore tempdir: {e}"))?;
    let store_sock = "snapstore.sock";
    let (_store_rt, _store_handle, store) =
        spawn_store_at(store_dir.path().to_path_buf(), store_sock);
    let snapstore = snapstore_client::Transport::Uds(store_dir.path().join(store_sock));
    let svc = dh_worker::service::WorkerService::new(m9_worker_config_with_slot_cores(
        test_name,
        slot_cores,
        artifacts.image_cache,
        snapstore,
    ))
    .map_err(|e| format!("WorkerService::new: {e:?}"))?;
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| format!("test runtime: {e}"))?;

    let (lease, initial_snapshot, ready_snapshot) = rt.block_on(async {
        let created = svc
            .create_vm(Request::new(proto::CreateVmRequest {
                config: Some(dh_worker::proto_map::machine_config_to_proto(&config)),
                entropy_seed: vec![0x9A; 32],
            }))
            .await
            .map_err(|e| format!("CreateVm BzImage: {e}"))?
            .into_inner();
        let lease = created
            .lease
            .ok_or_else(|| "CreateVm returned no lease".to_string())?;
        if created.icount != 0 {
            return Err(format!("CreateVm icount {}, expected 0", created.icount));
        }

        let initial = svc
            .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                lease: Some(lease.clone()),
                seal_input_log: Some(true),
                capture: None,
            }))
            .await
            .map_err(|e| format!("initial TakeSnapshot: {e}"))?
            .into_inner();
        let initial_snapshot = initial
            .snapshot
            .ok_or_else(|| "initial TakeSnapshot returned no snapshot".to_string())?;

        let run = svc
            .run(Request::new(proto::RunRequest {
                lease: Some(lease.clone()),
                until: Some(proto::run_request::Until::NextSdkEvent(
                    proto::NextSdkEvent {
                        stream: Some(detguest_wire::record::EventKind::Ready as u32),
                    },
                )),
                hard_icount_cap: M9_READY_HARD_CAP,
                capture: None,
            }))
            .await
            .map_err(|e| format!("Run until Ready: {e}"))?
            .into_inner();
        if run.reason != i32::from(proto::StopReason::NextSdkEvent) {
            return Err(format!(
                "Run stopped with reason {}, expected NextSdkEvent",
                run.reason
            ));
        }
        if run.sdk_event.as_ref().map(|event| event.stream)
            != Some(detguest_wire::record::EventKind::Ready as u32)
        {
            return Err("RunResponse.sdk_event was not Ready".into());
        }

        let ready_snapshot = svc
            .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                lease: Some(lease.clone()),
                seal_input_log: Some(true),
                capture: None,
            }))
            .await
            .map_err(|e| format!("Ready TakeSnapshot: {e}"))?
            .into_inner();
        Ok::<_, String>((lease, initial_snapshot, ready_snapshot))
    })?;

    let ready_snapshot_ref = ready_snapshot
        .snapshot
        .clone()
        .ok_or_else(|| "Ready TakeSnapshot returned no snapshot".to_string())?;
    let ready_state_hash = ready_snapshot
        .state_hash
        .as_ref()
        .ok_or_else(|| "Ready TakeSnapshot returned no state_hash".to_string())?
        .hash
        .clone();
    if ready_snapshot.machine_config_hash != config_hash.to_vec() {
        return Err("Ready TakeSnapshot machine_config_hash mismatch".into());
    }

    Ok(Some(M9LinuxReady {
        _store_rt,
        _store_handle,
        _store_dir: store_dir,
        store,
        svc,
        config_hash,
        initial_snapshot,
        ready_snapshot,
        ready_snapshot_ref,
        ready_state_hash,
        lease,
    }))
}

#[allow(dead_code)]
pub fn snapshot_section(
    store: &BlockingClient,
    snapshot: &dh_proto::v1::SnapshotRef,
    tag: [u8; 4],
) -> TestResult<Vec<u8>> {
    let hash: [u8; 32] = snapshot
        .hash
        .as_slice()
        .try_into()
        .map_err(|_| "snapshot ref must be 32 bytes".to_string())?;
    let container = store
        .get_snapshot(snapstore_types::SnapshotRef::from_bytes(hash))
        .map_err(|e| format!("get_snapshot: {e}"))?;
    let manifest =
        snapstore_manifest::Manifest::decode(&container).map_err(|e| format!("manifest: {e}"))?;
    let dhsnap = dh_snapshot::dhsnap::Container::parse(&manifest.device_blob.bytes)
        .map_err(|e| format!("DHSNAP parse: {e:?}"))?;
    dhsnap
        .get(tag)
        .map(|section| section.contents.to_vec())
        .ok_or_else(|| {
            format!(
                "snapshot missing DHSNAP section {}",
                std::str::from_utf8(&tag).unwrap_or("????")
            )
        })
}

#[allow(dead_code)]
pub fn input_log_payload(store: &BlockingClient, input_log_id: &[u8]) -> TestResult<Vec<u8>> {
    let id: [u8; 32] = input_log_id
        .try_into()
        .map_err(|_| format!("input log id must be 32 bytes, got {}", input_log_id.len()))?;
    let container = store
        .get_input_log(snapstore_types::LogId::from_bytes(id))
        .map_err(|e| format!("get_input_log: {e}"))?;
    let decoded = InputLogContainer::decode(&container)
        .map_err(|e| format!("input log container decode: {e}"))?;
    Ok(decoded.payload().to_vec())
}

#[allow(dead_code)]
pub async fn verify_replay_done(
    svc: &dh_worker::service::WorkerService,
    base: dh_proto::v1::SnapshotRef,
    input_log_id: Vec<u8>,
) -> TestResult<dh_proto::v1::VerifyDone> {
    use dh_proto::v1 as proto;
    use dh_proto::v1::hypervisor_worker_server::HypervisorWorker;
    use tokio_stream::StreamExt;
    use tonic::Request;

    let mut stream = svc
        .verify_replay(Request::new(proto::VerifyReplayRequest {
            base: Some(base),
            log: Some(proto::verify_replay_request::Log::InputLogId(input_log_id)),
            bisect_on_divergence: Some(false),
        }))
        .await
        .map_err(|e| format!("VerifyReplay: {e}"))?
        .into_inner();
    while let Some(progress) = stream.as_mut().next().await {
        let progress = progress.map_err(|e| format!("VerifyReplay progress: {e}"))?;
        match progress.msg {
            Some(proto::verify_replay_progress::Msg::Done(done)) => return Ok(done),
            Some(proto::verify_replay_progress::Msg::Divergence(divergence)) => {
                return Err(format!("VerifyReplay divergence: {divergence:?}"));
            }
            Some(proto::verify_replay_progress::Msg::EpochOk(_)) => {}
            None => return Err("VerifyReplay emitted empty progress message".into()),
        }
    }
    Err("VerifyReplay ended without Done".into())
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
#[allow(unused_imports)]
mod tests {
    use super::{M9LinuxArtifacts, M9_LINUX_ARTIFACT_ENV_VARS};
    use std::ffi::OsString;
    use std::path::PathBuf;

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
