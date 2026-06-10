//! M3 accept (bead 3t9): a timer deadline landing inside a masked (IF=0)
//! window defers to the FIRST injectable boundary >= B — identically
//! across 100 cold-boot runs, with delivered > requested and the guest
//! ISR observably running post-STI.

mod common;

use common::*;

#[test]
fn masked_window_deferral_identical_across_100_runs() {
    if !kvm_usable() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let report = dh_verify::gate::zero_divergence("if0-deferral", 100, |_| {
        // 'defer' mode: ~12k masked instructions, then STI + spin. The
        // deadline lands well inside the masked window.
        let mut rig = Rig::boot(nanokernel::timer_guest_elf(), b"defer")?;
        let requested = 5_000u64;
        let out = rig.run_one(
            Some(dh_vmm::runctl::TimerArm {
                deadline_vns: requested,
                vector: 0x40,
            }),
            60_000, // budget past the STI so the deferred delivery lands
        )?;
        let fired = out
            .timer_fired
            .ok_or_else(|| "timer never fired".to_string())?;
        if fired.delivered_icount <= requested {
            return Err(format!(
                "no deferral: delivered {} <= requested {requested}",
                fired.delivered_icount
            ));
        }
        let (count, vecs) = rig.read_table();
        if (count, vecs.as_slice()) != (1, &[0x40][..]) {
            return Err(format!("ISR table wrong: count={count} vecs={vecs:?}"));
        }
        Ok(format!(
            "requested={requested} delivered={} hash={}",
            fired.delivered_icount,
            hex(&out.state_hash)
        ))
    })
    .unwrap();
    assert!(report.passed(), "{}", report.artifact());
}
