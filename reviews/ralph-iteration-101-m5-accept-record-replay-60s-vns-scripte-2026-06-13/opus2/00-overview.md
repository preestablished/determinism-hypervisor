# M5 Record/Replay Acceptance (a5e) — Second Independent Review

- **Branch:** `ralph/iteration-101-m5-accept-record-replay-60s-vns-scripte` vs `main`
- **Date:** 2026-06-13
- **Reviewer:** Claude Opus (2nd reviewer)
- **Stats:** 1 file, +473, 1 commit (`c610074`)
- **File under review:** `crates/dh-worker/tests/m5_record_replay.rs` (entirely new)

## Summary

This change adds the M5 acceptance gate: it boots the `pad_echo` nanokernel guest from a real
snapstore snapshot, records a seeded SplitMix64-scripted PAD_SET sequence spanning N guest-seconds
under a deliberately non-1:1 (10_000:1) clock ratio, then replays that `(snapshot, DHILOG)` and
asserts the replay engine reproduces `end_state_hash`, every `EPOCH_HASH`, `end_vns`, and reseals
the log byte-identically. A `#[ignore]`d 60s × 100 acceptance follows the M4 perf-gate precedent;
an unignored 6s × 1 smoke keeps the non-1:1 clock path covered in every sweep. The test correctly
delegates the substantive verification to the production `replay_segment` contract rather than
re-implementing it, and adds independent host-side cross-checks (recorded pad sequence == script,
one EPOCH_HASH per second, guest-observed latch eras == script). The placement in `dh-worker/tests`
(not `tests/determinism`) is forced by the ARCH §1 dependency rule and is correctly justified.

The code is careful, well-documented, and the assertions are — with one real exception and a few
minor caveats — meaningful rather than tautological. I found no Critical defects. I raise one
Important issue (a genuine over-assertion that couples this acceptance gate to an incidental log
*byte layout*, so a legitimate reseal/header-layout refactor would break the acceptance spuriously),
and several Suggestions around the era-dedup false-pass window and a redundant assertion.

## Verdict

**APPROVE** (with one Important item recommended before merge; it is a maintainability/robustness
concern, not a correctness hole that lets a divergence pass).

- Critical: 0
- Important: 1
- Suggestions: 4
