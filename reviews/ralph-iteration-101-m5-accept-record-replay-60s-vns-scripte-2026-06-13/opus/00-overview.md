# M5 Record/Replay Acceptance (a5e) — Review Overview

- **Branch:** `ralph/iteration-101-m5-accept-record-replay-60s-vns-scripte` vs `main`
- **Date:** 2026-06-13
- **Reviewer:** Claude Opus
- **Stats:** 1 file, +473, −0, 1 commit (`c610074`)
- **Files:** `crates/dh-worker/tests/m5_record_replay.rs` (new)

## Summary

This change adds the M5 acceptance gate (bead a5e): it records a seeded, scripted
pad sequence spanning 60 guest-seconds of vns on the `pad_echo` nanokernel guest
from a real snapstore snapshot, then replays it from `(snapshot, DHILOG)` 100 times,
asserting every replay reproduces `end_state_hash`, every `EPOCH_HASH`, `end_vns`,
and reseals the log byte-identically. The acceptance (`x100`, ~11 min release) is
`#[ignore]`d on the M4 perf-gate precedent; an unignored 6-guest-second `x1` smoke
keeps the same rig — same scripted source, same non-1:1 clock — in every sweep. The
file extends the bead-39w `replay_engine.rs` template and follows its proven
per-thread-counter / per-slot live-KVM pattern, so its concurrency behavior under
`cargo test --workspace` is inherited, not new. The standout property: the 10_000:1
clock ratio makes this the first gate to drive the replay engine's `end_vns` check
with a value the icount axis does not mask (the path `replay_engine.rs` flagged as
"masked by 1:1 clocks until a5e"), and the change verifies it lands at exactly
`60e9` vns. The host-side reseal hammer and the guest-RAM era check
(`assert_table_eras`) are genuinely independent verifications, not a tautology.

## Verdict

**APPROVE**

No Critical issues. The assertions exercise the replay contract correctly without
re-implementing production logic, the off-by-one boundaries (`seconds-1` pads,
`records_applied`, epoch count) are all consistent, and the non-1:1 clock genuinely
unmasks the `end_vns` path. One Important-but-borderline finding (the `expected.dedup()`
asymmetry in `assert_table_eras`, harmless under the current fixed seed but a latent
assertion-weakener) and a handful of non-blocking suggestions are recorded. None block
merge; the dedup note is worth a one-line fix or comment before the script seed is ever
changed.
