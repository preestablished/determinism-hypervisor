//! Shared rig for the determinism gate tests: boot a guest, run one
//! segment at a time, read the timer-guest ISR table.

use std::io::ErrorKind;
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
