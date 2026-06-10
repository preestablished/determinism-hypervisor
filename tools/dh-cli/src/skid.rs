//! `dh-cli skid` (bead 19l): measure the PMI skid distribution on THIS
//! box and gate it against skid_margin / 2 (risk R1 alert threshold).
//!
//! Method (mirrors the §3.2 far approach with a zero margin): boot the
//! landing-loop guest, and per sample arm the PMI exactly `period`
//! retirements ahead, KVM_RUN until the kick's EINTR, and record
//! `counter_after − armed_point`. Periods cycle through a spread (≥ 10k
//! to stay clear of perf_event_max_sample_rate throttling — the
//! iteration-16 hazard).

use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};
use dh_verify::skid::SkidHistogram;
use dh_vmm::kvm::KvmSystem;
use kvm_ioctls::VcpuExit;

const PERIODS: &[u64] = &[100_000, 50_000, 25_000, 10_000];

pub struct SkidReport {
    pub histogram: SkidHistogram,
    pub skid_margin: u64,
    pub gate: Result<(), String>,
}

pub fn measure(samples: u64) -> Result<SkidReport, String> {
    dh_vmm::run::install_kick_handler().map_err(|e| format!("kick handler: {e}"))?;
    let sys = KvmSystem::open().map_err(|e| format!("{e:?}"))?;
    let mut slot = sys.create_slot_vm(16 << 20).map_err(|e| format!("{e:?}"))?;
    // Effectively endless landing loop: far more iterations than any
    // sample plan consumes.
    dh_vmm::boot::load_and_enter(&slot, nanokernel::landing_loop_elf(), b"4000000000")
        .map_err(|e| format!("{e}"))?;

    let counter = InstRetired::open_for_current_thread().map_err(|e| format!("{e:?}"))?;
    counter
        .route_overflow_to_thread(dh_vmm::run::current_tid(), dh_vmm::run::kick_signal())
        .map_err(|e| format!("{e:?}"))?;
    counter
        .arm_period(NEVER_FIRES_PERIOD)
        .map_err(|e| format!("{e:?}"))?;
    counter.reset().map_err(|e| format!("{e:?}"))?;
    counter.enable().map_err(|e| format!("{e:?}"))?;

    let mut histogram = SkidHistogram::default();
    let mut guard = dh_vmm::run::KickGuard::register(&mut slot.vcpu);
    for i in 0..samples {
        let period = PERIODS[(i % PERIODS.len() as u64) as usize];
        let before = counter.read().map_err(|e| format!("{e:?}"))?;
        let armed_point = before + period;
        counter.arm_period(period).map_err(|e| format!("{e:?}"))?;
        loop {
            match guard.run() {
                Err(e) if e.errno() == libc::EINTR => {
                    dh_vmm::run::clear_immediate_exit(&mut guard);
                    break;
                }
                Ok(VcpuExit::IoOut(..)) => {} // loop-completion serial; ignore
                Ok(other) => return Err(format!("unexpected exit: {other:?}")),
                Err(e) => return Err(format!("KVM_RUN: {e}")),
            }
        }
        let after = counter.read().map_err(|e| format!("{e:?}"))?;
        if after < armed_point {
            return Err(format!(
                "kick before the armed point ({after} < {armed_point}) — stale signal?"
            ));
        }
        histogram.record(after - armed_point);
        // Park the period so no stray overflow fires between samples.
        counter
            .arm_period(NEVER_FIRES_PERIOD)
            .map_err(|e| format!("{e:?}"))?;
    }
    drop(guard);

    let skid_margin = u64::from(dh_vmm::config::DEFAULT_SKID_MARGIN);
    let gate = histogram
        .assert_margin(skid_margin)
        .map_err(|v| v.to_string());
    Ok(SkidReport {
        histogram,
        skid_margin,
        gate,
    })
}
