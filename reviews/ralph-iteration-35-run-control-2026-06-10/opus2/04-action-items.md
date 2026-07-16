# Action items

Self-contained; each item names the file, the defect, and the fix.

### Critical

- [ ] **Fix the multi-vector injection overwrite in `run_segment`'s chaining loop.**
  `crates/dh-vmm/src/runctl.rs:194-212`. When a `StopPoint` carries ≥2 injections at an already-injectable boundary, the loop queues every vector via `inject_at_boundary` with **no KVM_RUN between them**, so each `KVM_INTERRUPT` overwrites the previous (proven live: `delivered_icounts=[4,4]`, KVM queue holds only the last vector). This violates the CONTRACT at `inject.rs:96-99`. Fix: force a VM entry (`land_at(at.icount + 1)`) between consecutive queued vectors at one boundary, so each prior vector delivers and the next sees a fresh kvm_run — OR push the per-boundary multi-vector handling into `inject.rs` which owns the "one vector per entry" invariant. Also fix `injections_delivered` to count actual deliveries, not loop iterations (`runctl.rs:206`).
- [ ] **Add a live multi-vector regression test.** None exists today. Schedule two vectors at one boundary on `sti_window` and assert both deliver (e.g. observe `KVM_GET_VCPU_EVENTS.interrupt.nr` transition across entries, or two distinct guest-observable effects). This gap is why the overwrite shipped.

### Important

- [ ] **Decide HLT/Shutdown handling and stop dropping captured serial.**
  `tools/dh-cli/src/run.rs:661-667` and `crates/dh-vmm/src/runctl.rs:73-78`. `dh-cli run pipeline_smoke` past the HLT aborts with `unexpected exit: Hlt`, rc=1, no JSON, and discards the captured 'K' serial. The proto has `GUEST_HALTED = 6` for this. Either (A) document that Phase-1 treats Hlt/Shutdown as caller-fatal and the budget must land before a terminal HLT, OR (B, preferred for M6) add `StopReason::GuestHalted`, recognize `VcpuExit::Hlt`/`Shutdown` as a terminal stop in `run_segment`, finish the segment normally (hash boundary, return serial), and map to proto `GUEST_HALTED`. File a bead for (B). At minimum, never drop already-captured serial on the error path.

### Suggestions

- [ ] **Define agenda↔deferral interplay (deferral past the next agenda point).** `runctl.rs:181-264` + `inject.rs:147`. A deferral that steps past point *k+1* makes the next `land_at` overshoot (fatal). Deterministic-loud but wrong semantics. File an M6-scheduler bead to specify re-planning/skipping points the deferral stepped over (ARCH §3.3/§3.4 are silent on this).
- [ ] **Guard `start_icount` against caller lies.** `runctl.rs:143`. Assert `seg.counter.read() == seg.start_icount` at segment entry; agenda grids are `start_icount`-relative while `land_at` is absolute, so a mismatch silently mis-lands every point.
- [ ] **Document the goal-callback determinism requirement.** `runctl.rs:141-147`. The `goal` closure's return must be a pure function of guest state for replay identity; the doc doesn't say so.
- [ ] **Confirm/annotate the coincident epoch+final double-hash.** `runctl.rs:221` then `finish()` `runctl.rs:277`. When final lands on an epoch multiple, `push_final_link` runs twice at one boundary. Likely intended per ARCH §8.5 ("every epoch boundary AND every final pause") — add a one-line comment confirming, or guard if unintended.
- [ ] **(Optional) `gettid()` main-thread assumption.** `tools/dh-cli/src/run.rs:613-617`. `pid as tid` is correct only on the main thread; add a guard/comment if the CLI ever runs the segment off the main thread (PMI signal routing would break otherwise).

### Verified clean (no action)

- Pause `Ordering::Relaxed` racy observation point — acceptable by design; ARCH §3.3 + API.md §2.4 document it. (S3)
- `vns-budget`, `icount-budget`, run-twice determinism, unwired-mode loud failure — all confirmed live.
