//! Shared rig for the determinism gate tests: boot a guest, run one
//! segment at a time, read the timer-guest ISR table.

use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};
use dh_vmm::boundary::BoundaryError;
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::hash::StateHashChain;
use dh_vmm::kvm::{KvmSystem, SlotVm};
use dh_vmm::runctl::{run_segment, Segment, SegmentOutcome, TimerArm, Until};
use kvm_ioctls::VcpuExit;
use vm_memory::Bytes;

pub fn kvm_usable() -> bool {
    match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/kvm")
    {
        Ok(_) => true,
        Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::PermissionDenied) => false,
        Err(e) => panic!("unexpected /dev/kvm probe failure: {e}"),
    }
}

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

#[allow(dead_code)] // used by if0_deferral, not timer_determinism (per-test compilation)
pub fn hex(b: &[u8; 32]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub struct Rig {
    pub slot: SlotVm,
    pub counter: InstRetired,
    pub chain: StateHashChain,
    pub config: MachineConfig,
}

impl Rig {
    pub fn boot(elf: &[u8], cmdline: &[u8]) -> Result<Rig, String> {
        dh_vmm::run::install_kick_handler().map_err(|e| format!("kick: {e}"))?;
        let sys = KvmSystem::open().map_err(|e| format!("{e:?}"))?;
        let slot = sys.create_slot_vm(16 << 20).map_err(|e| format!("{e:?}"))?;
        dh_vmm::boot::load_and_enter(&slot, elf, cmdline).map_err(|e| format!("{e}"))?;
        let counter = InstRetired::open_for_current_thread().map_err(|e| format!("{e:?}"))?;
        counter
            .route_overflow_to_thread(dh_vmm::run::current_tid(), dh_vmm::run::kick_signal())
            .map_err(|e| format!("{e:?}"))?;
        counter
            .arm_period(NEVER_FIRES_PERIOD)
            .map_err(|e| format!("{e:?}"))?;
        counter.reset().map_err(|e| format!("{e:?}"))?;
        counter.enable().map_err(|e| format!("{e:?}"))?;
        let config = MachineConfig::new(
            16 << 20,
            [3; 32],
            BootSpec::Elf {
                kernel_hash: [3; 32],
                cmdline: cmdline.to_vec(),
            },
        );
        Ok(Rig {
            slot,
            counter,
            chain: StateHashChain::new(&[3; 32], &[3; 32]),
            config,
        })
    }

    /// One segment from the current position to `budget` (absolute).
    pub fn run_one(
        &mut self,
        timer: Option<TimerArm>,
        budget_abs: u64,
    ) -> Result<SegmentOutcome, String> {
        let start = self.counter.read().map_err(|e| format!("{e:?}"))?;
        let pause = AtomicBool::new(false);
        let mut seg = Segment {
            slot: &mut self.slot,
            counter: &self.counter,
            chain: &mut self.chain,
            config: &self.config,
            start_icount: start,
            injections: &[],
            timer,
            pause: &pause,
            sdk_events: None,
            hash_device_sections: None,
        };
        run_segment(
            &mut seg,
            Until::IcountBudget(budget_abs.saturating_sub(start)),
            &mut || false,
            &mut |exit: VcpuExit| Err(BoundaryError::Exit(format!("unexpected exit: {exit:?}"))),
        )
        .map_err(|e| format!("{e}"))
    }

    pub fn read_table(&self) -> (u64, Vec<u8>) {
        let mut head = [0u8; 8];
        self.slot
            .guest_mem
            .read_slice(
                &mut head,
                vm_memory::GuestAddress(nanokernel::TIMER_GUEST_TABLE_GPA),
            )
            .unwrap();
        let count = u64::from_le_bytes(head);
        let mut vecs = vec![0u8; count as usize];
        self.slot
            .guest_mem
            .read_slice(
                &mut vecs,
                vm_memory::GuestAddress(nanokernel::TIMER_GUEST_TABLE_GPA + 8),
            )
            .unwrap();
        (count, vecs)
    }
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
