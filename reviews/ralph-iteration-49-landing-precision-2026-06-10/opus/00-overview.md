# Iteration 49 — Landing Precision (M2 acceptance) — Review Overview

- **Reviewer:** Claude Opus
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-49-landing-precision`
- **Bead:** 8g1 (P0, M2 acceptance: landing precision)
- **Verdict:** **APPROVE**

## Scope

Diff `main...HEAD` (247 insertions, 4 files, zero deletions):

- `tests/nanokernel/asm/rep_loop.asm` (NEW) — endless 6-instruction REP-MOVSB loop; RCX is a built-in mid-REP detector (64 at REP start, 0 elsewhere).
- `tests/nanokernel/build.rs` — `rep_loop` added to the build list.
- `tests/nanokernel/src/lib.rs` — `rep_loop_elf()` accessor + `REP_LOOP_INSTRS_PER_ITER=6`, `REP_LOOP_RCX_AT_REP_START=64`.
- `tests/determinism/tests/landing_precision.rs` (NEW) — two hardware-gated tests: 10,000 random targets in the 100M landing loop (zero-overshoot + cross-boot tuple identity), and 1,000 random targets in `rep_loop` (no mid-REP + cross-boot tuple identity).

The code *under acceptance* (`crates/dh-vmm/src/boundary.rs::land_at`) is unchanged this iteration; this is a pure-test acceptance landing on the existing boundary engine.

## What I ran (this is an executing review)

- **Both new tests, full, serial:** PASS. 2 passed; 0 failed. Wall 93.1s on this box (Intel i5-8400 @ 2.80 GHz, 2 cores — slower than the lab's "~71s observed").
- **Skid measurement, 50,000 samples** (`dh-cli skid`): min 27, **max 39** (single outlier; 99.998% ≤ 31). Directly bears on boot B's margin 128.
- **Sharper margin-independence spot-check** (scratch, deleted): SAME 20 targets — including adjacency 1000/1001/1002 and targets up to 98,999,999 — landed at three margins {8192/1024, 4096/512, 64/64}. **All three boots produced bit-identical (icount, rip, rcx) tuples.** This is a wider spread than the production test's own 8192-vs-128 prefix evidence.
- **RCX-at-guest-entry probe** (scratch, deleted): landed at icount 1..20 of `rep_loop`. RCX is **0** (not garbage) for the first 6 retired instructions, becomes 64 exactly at icount=7 (right after `mov rcx,64`). The garbage-RCX false-failure hazard does not exist on this box.
- **`cargo clippy --workspace --all-targets`** — x86_64: clean. aarch64 (with the provided clang/llvm-ar env): clean.
- **`cargo test --workspace --lib`** — all unit tests pass (dh-detclock, dh-vmm, dh-worker, nanokernel image gen, etc.).
- **Working tree clean** after scratch deletion (verified `git status` empty).

## Bottom line

The acceptance is sound and the evidence is real. Margin-independence (§3.2) holds under a stricter test than the suite itself applies; the REP no-mid-boundary invariant holds and the RCX detector is well-constructed; zero overshoots across 10,000 targets at margin 256 *and* a re-land at 128; skid headroom at margin 128 is 3.3x the worst observed skid. No Critical or Important findings. A few Suggestions (CI lane scheduling, a doc-comment nit on `Boundary.rcx`, and the observation that the *wide* margin spread is confined to the prefix in the shipped test) are catalogued but none block merge.

See `01-critical-and-important.md`, `02-suggestions.md`, `03-positive-notes.md`, `04-action-items.md`.
