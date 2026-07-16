# Iteration 89 — VerifyReplay reporting model — Review (2nd reviewer)

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-11
- **Branch:** `ralph/iteration-89-verify-replay`
- **Bead:** determinism-hypervisor-1py (VerifyReplay execution path)
- **Scope:** 5 files, 347 diff lines

## Summary

Bead 1py adds the VerifyReplay reporting layer on top of the already-landed
replay executor (39w). `dh-verify/src/verify.rs` introduces the pure-types
reporting model (`VerifyProgress` enum + `VerifyReport` collector), and
`dh-worker/src/verify_replay.rs` is the thin executor wrapper that runs
`replay_segment` and translates its outcome into that model: one `EpochOk`
per recorded epoch on success plus a terminal `Done`, or a single
`Divergence` verdict otherwise. Infrastructure failures (store/log-parse/KVM)
remain `Err`. A live KVM test exercises both the good-recording (10 EpochOk +
Done) and RAM-poisoned (Divergence at epoch 1) paths.

The architecture is clean and the dependency direction is correct (dh-verify
owns the shapes, dh-worker imports them — nothing depends on dh-worker).
**dh-verify adds zero new dependencies** (still only `dh-snapshot`); verify.rs
is pure types and builds host-only/aarch64-clean. The honesty story for the
reconstructed EpochOk stream holds up under adversarial tracing — see I-1.

The findings below are about **API-contract precision** and **what-aware
Divergence mapping**, not correctness bugs in the happy/poison paths.

## Verdict

**APPROVE WITH NITS.** No Critical defects. Two Important items concern the
fidelity of the `Divergence` event for non-epoch divergence shapes (the
`first_bad_epoch`/`expected`/`got` fields carry nonsense or misleading values
for `resealed log bytes`, `end_vns`, and `end_state_hash` divergence kinds),
and one concerns the `verified()` last-event contract. None block the M5 demo
path; all should be resolved before cw2 (the 1000x harness) treats these
fields as load-bearing.

## Stats

| Severity   | Count |
|------------|-------|
| Critical   | 0     |
| Important  | 3     |
| Suggestions| 5     |
| Positive   | 5     |

## Files reviewed

- `crates/dh-verify/src/verify.rs` (new, 104 lines) — reporting model
- `crates/dh-worker/src/verify_replay.rs` (new, 87 lines) — executor wrapper
- `crates/dh-worker/src/replay_engine.rs` (read in full — divergence surface)
- `crates/dh-worker/tests/replay_engine.rs` (new test, +98 lines)
- `crates/dh-verify/src/lib.rs`, `crates/dh-worker/src/lib.rs` (module wiring)
