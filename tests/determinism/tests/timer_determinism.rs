//! M3 accept (bead 0zh): repeated timer fires produce IDENTICAL
//! delivered-icount lists across 100 cold-boot runs (exact list compare).
//!
//! Phase-1 scoping: the guest's own 1ms-x-10s MMIO arming loop awaits the
//! device-bus run loop (bead 40q); here run control host-arms the same
//! cadence — one TimerArm per 1M-icount segment, 10 fires per run — which
//! exercises the identical §4-convert → agenda → §3.4-deliver chain.

mod common;

use common::*;

const FIRES: u64 = 10;
const PERIOD_ICOUNT: u64 = 1_000_000; // 1 ms-vns at the 1:1 clock

#[test]
fn delivered_icount_lists_identical_across_100_runs() {
    if !kvm_usable() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let report = dh_verify::gate::zero_divergence("timer-determinism", 100, |_| {
        let mut rig = Rig::boot(nanokernel::timer_guest_elf(), b"")?;
        let mut delivered = Vec::new();
        for k in 0..FIRES {
            let deadline = (k + 1) * PERIOD_ICOUNT;
            let out = rig.run_one(
                Some(dh_vmm::runctl::TimerArm {
                    deadline_vns: deadline,
                    vector: 0x41,
                }),
                deadline,
            )?;
            let fired = out
                .timer_fired
                .ok_or_else(|| format!("fire {k}: timer did not fire"))?;
            delivered.push(fired.delivered_icount);
        }
        // The guest ISR observed every delivery except the final segment's
        // still-queued vector (budget == deadline merges the points).
        let (count, vecs) = rig.read_table();
        if count != FIRES - 1 || !vecs.iter().all(|&v| v == 0x41) {
            return Err(format!("ISR table wrong: count={count} vecs={vecs:?}"));
        }
        Ok(format!("{delivered:?}"))
    })
    .unwrap();
    assert!(report.passed(), "{}", report.artifact());
}
