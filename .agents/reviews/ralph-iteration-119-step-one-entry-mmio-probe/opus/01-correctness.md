# Correctness

No blocking correctness findings.

The live test in `crates/dh-vmm/src/boundary.rs:550` exercises the important sequence directly:

- `discover_mmio_crossing_entry` walks the new guest with `step_one_entry` until a single entry observes emulated-MMIO exits, then returns the pre-entry `icount` and the no-interrupt post-entry `icount` (`crates/dh-vmm/src/boundary.rs:525`).
- The test lands back at that discovered pre-entry boundary, queues vector `0x40`, and asserts the injection queues at the requested boundary (`crates/dh-vmm/src/boundary.rs:558`, `crates/dh-vmm/src/boundary.rs:568`).
- The following `step_one_entry` call records MMIO exits from the entry that delivered vector `0x40`, requiring at least one MMIO write and one MMIO read (`crates/dh-vmm/src/boundary.rs:583`).
- It then queues vector `0x41` at the boundary returned by that first `step_one_entry`, matching the same-boundary chaining pattern used by `runctl` for later injections sharing an agenda point (`crates/dh-vmm/src/boundary.rs:607`, `crates/dh-vmm/src/runctl.rs:420`).
- The guest table check proves both vectors delivered, in order (`crates/dh-vmm/src/boundary.rs:621`).

The boundary exactness checks are strong enough for this bead. `after_first.icount > discovered_after` confirms the first stepped entry included ISR retirements in addition to the no-interrupt MMIO-crossing entry, so the queued interrupt was not skipped. The two fresh-boot runs compare `(target, after_first.icount, after_second.icount, first_exits, vecs)`, which is the right replay-stability property for a dynamically discovered MMIO-adjacent boundary.

The test does not call `runctl::run_segment` itself. That leaves a small integration gap around the `unwind_or!` wrapper and agenda plumbing, but the production code path at issue is the same primitive sequence: `inject_at_boundary`, then `step_one_entry` between vectors at the same point. I would not block the bead on a full `runctl` integration test unless this branch is meant to prove agenda behavior too.
