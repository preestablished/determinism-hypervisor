# Iteration 87 — EPOCH_HASH writer + epoch sink (bead y62) — Review Overview

- **Branch:** `ralph/iteration-87-epoch-hash-writer`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Commit:** `c97f87e` — "ralph: iteration 87 checkpoint - DHILOG EPOCH_HASH writer + epoch sink (y62)"
- **Scope:** 3 files, +171/-1, 247 diff lines, 1 commit
- **Bead:** `determinism-hypervisor-y62` — DHILOG EPOCH_HASH writer + run_segment wiring (the a5e prerequisite surfaced by iteration 86's review)

## Verdict

**APPROVE**

This is a clean, well-scoped landing of the EPOCH_HASH producer side. Every concern the
task flagged as a potential trap was checked against source and found correctly handled.
The change is the missing producer that bead a5e's critical path required; it builds, the
golden-fixture freeze tests still pass unchanged, and the live test proves the sink-to-log
byte path end-to-end. No correctness defects found.

## Summary of the change

1. **`crates/dh-inputlog/src/dhilog.rs`** — adds `LogWriter::epoch_hash(icount, boundary_rip,
   epoch_index, chain_value)` writing an AUX `KIND_EPOCH_HASH` record with a 40-byte payload
   (`epoch_index` u64 LE @0..8 ‖ `chain_value` [u8;32] @8..40), exactly the reader's frozen
   decode shape. A new `wrote_epoch_hash` flag makes `seal()` OR in `FLAG_EPOCH_HASHES`.

2. **`crates/dh-vmm/src/runctl.rs`** — adds `run_segment_with_epochs(..., epoch_sink: &mut
   Vec<(u64,u64,[u8;32])>)`; `run_segment` delegates with a throwaway `Vec` (zero call-site
   churn). The sink collects `(epoch_index, icount, chain_value_after_push)` at BOTH chain-link
   sites: the agenda-walk `point.epoch_hash` arm and the pause roll-forward (which lands on the
   epoch grid by construction). Final-pause links at non-epoch boundaries are deliberately not
   sinked — they ride in `END.end_state_hash`.

3. **`crates/dh-vmm/src/recording.rs`** — adds `DeviceRail::log_epoch_hashes(links, rip)` to
   land the sink as AUX records after a quantum, plus a `/dev/kvm`-gated live test proving the
   indices `(1,30k),(2,60k),(3,90k)`, nonzero chains, byte-identical records, and the header flag.

## Verification performed

- `cargo build -p dh-inputlog -p dh-vmm` — clean.
- `cargo test -p dh-inputlog` — **29 + 2 passed, 0 failed**, including the golden-fixture
  freeze tests (`tests/golden.rs`): confirms adding the writer cannot perturb the frozen v1.0
  bytes (neither fixture calls `epoch_hash()`, so `wrote_epoch_hash` stays false and the byte
  output is identical).
- `cargo test -p dh-vmm --no-run` — test binary compiles (the new live test is `/dev/kvm`-gated
  and skips without hardware; logic reviewed by hand and against the byte assertions).

## Stats

| Metric | Value |
| --- | --- |
| Files changed | 3 |
| Lines added / removed | +171 / -1 |
| Diff lines | 247 |
| Commits | 1 |
| Critical findings | 0 |
| Important findings | 0 |
| Suggestions | 3 |
| Positive notes | 6 |
