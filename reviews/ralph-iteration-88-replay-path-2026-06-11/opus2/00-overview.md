# Iteration 88 — Replay Path — Second-Reviewer Review (Overview)

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-11
- **Branch:** `ralph/iteration-88-replay-path`
- **Diff:** `/tmp/iter88.diff` (~904 lines)
- **Scope:** `crates/dh-worker/src/replay_engine.rs` (new, 349 lines), the
  `run_segment_with_epochs` Vec-sink → callback conversion in
  `crates/dh-vmm/src/runctl.rs`, the matching `log_epoch_hashes` → `log_epoch_hash`
  change in `crates/dh-vmm/src/recording.rs`, the new live joint test
  `crates/dh-worker/tests/replay_engine.rs`, and `tests/common/mod.rs` dead-code gates.

## Summary

This iteration makes the product's core property executable: drive a fresh slot
from `(snapshot, DHILOG)`, land each canonical record at its recorded icount via
the boundary engine, apply through the same `DeviceRail` entry points recording
used, verify every `EPOCH_HASH` against the live chain at the link point, check
`end_state_hash` at END, and finally re-seal — with a byte-identical reseal hammer
as the strongest available equality.

The design is genuinely strong. The two adversarial bug candidates I was pointed
at — RefCell re-entrancy and the entropy v2 split — are both **clean** under
close tracing. The boundary-engine mechanics also save the GuestHalted tail from
being the bug it first appears to be. The one real finding is a **verification
gap, not a correctness bug**: `end_vns` is never compared against the replay's
computed tail vns, and the reseal hammer cannot catch it because `end_vns` lives
only in the header (outside `body_hash`) and is copied verbatim from the input.
In the 1:1-clock tests this is fully masked (vns == icount). It will stay masked
until a non-1:1 clock recording is replayed — exactly the kind of latent gap a
P0 replay path should not ship silently.

Verification performed in source (not the summary):
- `seal()` in `dhilog.rs` confirms `end_vns` → header bytes `[168..176]`, **not**
  in the END record payload and **not** under `body_hash` (which covers
  `[HEADER_LEN..]`).
- `land_at` returns at `c == target` at an *instruction start*, so a tail budget
  that coincides with a HLT boundary lands `BudgetReached` *before* executing the
  HLT — the GuestHalted tail does not error for the pad-echo/halt-boundary shape.
- `push_final_link` hashes **full memory** (every page ascending), so the
  RAM-poison divergence test is sound — the poison at `0x60_0000` perturbs the
  very first `EPOCH_HASH`.
- `LogReader::aux()` is `records().filter(is_aux)` over file order = normative
  `(icount, seq)` order, so `expected_epochs` is in emission order and the
  in-order `Cell` index match is correct.
- RefCell `borrow_mut()` in `on_exit` and in the sink are each scoped to one
  statement; nothing holds a borrow across a `run_segment_with_epochs` call;
  `irqs` is read between records with no other borrow live. No nesting possible.
- `cargo check -p dh-worker --lib`, `cargo test -p dh-worker --test replay_engine
  --no-run`, and `cargo clippy -p dh-worker --lib` all pass clean.

## Verdict

**APPROVE WITH NITS.** Ship-worthy core. The `end_vns` verification gap and the
GuestHalted-tail brittleness should be filed and addressed before a non-1:1-clock
recording or a halting guest enters the replay corpus, but neither blocks the
pad-echo M5 path this iteration targets. No Critical findings.

## Stats

| Class       | Count |
|-------------|-------|
| Critical    | 0     |
| Important   | 2     |
| Suggestions | 5     |
| Positive    | 6     |

The two Important items are a verification gap (`end_vns` unverified) and a
forward-looking robustness gap (GuestHalted tail relies on an implicit
boundary-timing coincidence with no test pinning it). Details in
`01-critical-and-important.md`; condensed, self-contained tasks in
`04-action-items.md`.
