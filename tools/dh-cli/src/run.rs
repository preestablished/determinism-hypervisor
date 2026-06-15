//! `dh-cli run` (bead qs4): boot a guest and run one Phase-1 segment via
//! dh-vmm's run control, printing the outcome — the M3 run-twice-compare
//! driver (run it twice, diff the JSON).

use std::sync::atomic::AtomicBool;

use dh_devices::serial::{SERIAL_PIO_BASE, SERIAL_PIO_LEN};
use dh_vmm::boundary::BoundaryError;
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::hash::StateHashChain;
use dh_vmm::kvm::KvmSystem;
use dh_vmm::runctl::{run_segment_with_options, RunOptions, Segment, StopReason, Until};
use kvm_ioctls::VcpuExit;

pub struct RunReport {
    pub reason: &'static str,
    pub icount: u64,
    pub rip: u64,
    pub vns: u64,
    pub state_hash: String,
    pub serial: Vec<u8>,
}

pub fn run(
    elf: &[u8],
    mem_bytes: u64,
    cmdline: &[u8],
    until: Until,
    paranoid_hash: bool,
) -> Result<RunReport, String> {
    use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};

    dh_vmm::run::install_kick_handler().map_err(|e| format!("kick handler: {e}"))?;
    let sys = KvmSystem::open().map_err(|e| format!("{e:?}"))?;
    let mut slot = sys
        .create_slot_vm(mem_bytes)
        .map_err(|e| format!("{e:?}"))?;
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
        mem_bytes,
        [0; 32],
        BootSpec::Elf {
            kernel_hash: [0; 32],
            cmdline: cmdline.to_vec(),
        },
    );
    let mut chain = StateHashChain::new(&[0; 32], &[0; 32]);
    let pause = AtomicBool::new(false);
    let mut serial = dh_devices::DebugSerial::new();

    let outcome = {
        let mut seg = Segment {
            slot: &mut slot,
            counter: &counter,
            chain: &mut chain,
            config: &config,
            start_icount: 0,
            injections: &[],
            timer: None,
            pause: &pause,
            sdk_events: None,
        };
        const SERIAL_END: u16 = SERIAL_PIO_BASE + SERIAL_PIO_LEN;
        let mut on_exit = |exit: VcpuExit| match exit {
            VcpuExit::IoOut(port, data) if (SERIAL_PIO_BASE..SERIAL_END).contains(&port) => {
                serial.pio_write(port, data);
                Ok(())
            }
            VcpuExit::IoIn(port, data) if (SERIAL_PIO_BASE..SERIAL_END).contains(&port) => {
                serial.pio_read(port, data);
                Ok(())
            }
            other => Err(BoundaryError::Exit(format!("unexpected exit: {other:?}"))),
        };
        run_segment_with_options(
            &mut seg,
            until,
            RunOptions { paranoid_hash },
            &mut || false,
            &mut on_exit,
        )
        .map_err(|e| format!("{e}"))?
    };

    Ok(RunReport {
        reason: match outcome.reason {
            StopReason::BudgetReached => "budget_reached",
            StopReason::GoalSatisfied => "goal_satisfied",
            StopReason::NextSdkEvent => "next_sdk_event",
            StopReason::HardCap => "hard_cap",
            StopReason::Paused => "paused",
            StopReason::GuestHalted => "guest_halted",
        },
        icount: outcome.boundary.icount,
        rip: outcome.boundary.rip,
        vns: outcome.vns,
        state_hash: outcome
            .state_hash
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        serial: serial.take_output(),
    })
}
