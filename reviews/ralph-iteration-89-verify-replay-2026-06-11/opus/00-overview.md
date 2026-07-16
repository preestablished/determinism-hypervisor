# Iteration 89 — VerifyReplay execution path — Review Overview

- **Branch:** `ralph/iteration-89-verify-replay`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Bead:** determinism-hypervisor-1py (P0) — VerifyReplay execution path
- **Verdict:** **REQUEST_CHANGES**

## Summary

This change lands the VerifyReplay reporting model and its executor, split across
the ARCH §1 dependency seam (nothing depends on `dh-worker`, so the model lives in
`dh-verify` and the executor imports it). The split is correct and the seam is
respected. The executor is a thin, honest wrapper over the already-strong
`replay_segment` engine: it does not re-verify anything, it translates the engine's
outcome into the reporting model. The Ok-verdict / Err-infrastructure boundary is
the right design and is mostly classified correctly.

Two design choices undermine the model's two headline claims, and both are
load-bearing for the downstream consumers (cw2's 1000x harness and rfv's RPC):

1. **The model's `Divergence` does not mirror the proto it claims to mirror.**
   The doc comments assert fidelity to proto §2.7 `Divergence` and say only the
   M8 bisection fields are deferred. In fact the model invents `at_icount`, `what`,
   `expected`, `got` (none of which exist in the proto) and omits *every* proto
   `Divergence` field except `first_bad_epoch`. The proto carries no hash pair at
   all. This is not a blocker for the library harness, but the fidelity claim is
   false and rfv will need a non-trivial translation layer that the comments imply
   does not exist.

2. **`first_bad_epoch = at_icount / epoch_len` is only meaningful for the one
   `EPOCH_HASH chain value` divergence.** The engine produces five other divergence
   `what`s, and for them `at_icount` is either `end_icount` (yielding a
   `first_bad_epoch` that names an epoch that *matched*) or — for the resealed-log
   case — a raw **byte offset**, making the division nonsense. The wrapper applies
   one formula to all of them.

Plus one robustness issue: the EpochOk-count invariant is pinned only by
`debug_assert_eq!`, so a release build can silently misreport the epoch count.

The live test is good (real KVM, good + poisoned recordings, asserts the Ok-report /
Err split explicitly) but it only exercises the single divergence path where the
arithmetic happens to be correct, so it gives false confidence about the other five.

## Stats

| Metric | Value |
|---|---|
| Files changed | 5 (2 new src, 1 test, 2 mod-registration edits) |
| Diff lines | ~347 |
| Commits | 1 |
| New public API | `VerifyProgress` enum, `VerifyReport` struct, `verify_replay()` fn |
| Critical findings | 0 |
| Important findings | 3 |
| Suggestions | 5 |

## Findings by severity

- **Critical:** none.
- **Important:** (1) proto `Divergence` fidelity claim is false; (2) `first_bad_epoch`
  arithmetic is nonsense for the 5 non-`EPOCH_HASH-chain` divergence kinds; (3)
  `debug_assert_eq!` on the EpochOk count should be a hard check.
- **Suggestions:** model/proto naming drift on the variant names, `VerifyReport`
  ergonomics for cw2's 1000x loop, dead `.max(1)`, test coverage of the other
  divergence kinds, doc-comment overstatement.

## Verdict rationale

REQUEST_CHANGES rather than NEEDS_DISCUSSION: the `first_bad_epoch` arithmetic
(Important #2) is a correctness defect that will mislabel real divergences with a
confidently-wrong epoch number, and the fix is local and clear (a `what`-aware
mapping, or `Option<u64>`/sentinel for non-epoch divergences). The proto-fidelity
gap (#1) is at minimum a documentation correction and at most a model change that
rfv depends on; it should be resolved before cw2/rfv build on this surface. None of
the three is large; this is a "fix and re-review" not a redesign.
