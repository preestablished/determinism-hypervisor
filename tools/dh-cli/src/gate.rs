//! `dh-cli gate` (bead ksx; phase doc gate 1): the one-command Phase-1
//! determinism gate. Three sub-gates, every fingerprint compared:
//!
//!   plain   — boot the landing loop, run to N: hash twice, then the
//!             full zero-divergence sweep (default 100 runs);
//!   timer   — same, with a timer event injected at an exact icount
//!             (the §4 → agenda → §3.4 chain in the fingerprint).
//!
//! Emits the report artifact (run list, fingerprints, verdicts).

use std::sync::atomic::AtomicBool;

use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};
use dh_verify::gate::{zero_divergence, GateReport};
use dh_vmm::boundary::BoundaryError;
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::hash::StateHashChain;
use dh_vmm::kvm::KvmSystem;
use dh_vmm::runctl::{run_segment, Segment, TimerArm, Until};
use kvm_ioctls::VcpuExit;

const BUDGET: u64 = 2_000_000;
const TIMER_AT: u64 = 1_234_567;

fn cold_fingerprint(timer: Option<TimerArm>) -> Result<String, String> {
    dh_vmm::run::install_kick_handler().map_err(|e| format!("kick: {e}"))?;
    let sys = KvmSystem::open().map_err(|e| format!("{e:?}"))?;
    let mut slot = sys.create_slot_vm(16 << 20).map_err(|e| format!("{e:?}"))?;
    let elf = if timer.is_some() {
        nanokernel::timer_guest_elf()
    } else {
        nanokernel::landing_loop_elf()
    };
    dh_vmm::boot::load_and_enter(&slot, elf, b"1000000000").map_err(|e| format!("{e}"))?;
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
        [9; 32],
        BootSpec::Elf {
            kernel_hash: [9; 32],
            cmdline: b"1000000000".to_vec(),
        },
    );
    let mut chain = StateHashChain::new(&[9; 32], &[9; 32]);
    let pause = AtomicBool::new(false);
    let mut seg = Segment {
        slot: &mut slot,
        counter: &counter,
        chain: &mut chain,
        config: &config,
        start_icount: 0,
        injections: &[],
        timer,
        pause: &pause,
    };
    let out = run_segment(
        &mut seg,
        Until::IcountBudget(BUDGET),
        &mut || false,
        &mut |exit: VcpuExit| Err(BoundaryError::Exit(format!("unexpected exit: {exit:?}"))),
    )
    .map_err(|e| format!("{e}"))?;
    Ok(format!(
        "icount={} rip={:#x} vns={} hash={} timer={:?}",
        out.boundary.icount,
        out.boundary.rip,
        out.vns,
        out.state_hash
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
        out.timer_fired.map(|t| t.delivered_icount)
    ))
}

pub fn run_gate(runs: usize) -> Result<(GateReport, GateReport), String> {
    let plain = zero_divergence("plain-landing", runs, |_| cold_fingerprint(None))?;
    let timer = zero_divergence("timer-event", runs, |_| {
        cold_fingerprint(Some(TimerArm {
            deadline_vns: TIMER_AT,
            vector: 0x41,
        }))
    })?;
    Ok((plain, timer))
}
