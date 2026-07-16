# Action Items

Self-contained tasks distilled from the findings. Nothing here blocks the
pad-echo M5 path this iteration ships; the two Important items should be filed and
closed before a non-1:1-clock recording or a halting guest enters the replay
corpus (a5e/mub).

### Critical

None.

### Important

- [ ] **Verify `end_vns` against the replay's computed tail vns.**
  `crates/dh-worker/src/replay_engine.rs`. The tail `run_to(header.end_icount)`
  discards the `SegmentOutcome` it computes; capture its `out.vns` (return it from
  `run_to` for the final quantum, or add a tail-specific run-out that yields the
  outcome) and compare `tail_out.vns != header.end_vns` before the reseal,
  returning a divergence on mismatch. This is currently unverifiable by the reseal
  hammer: `dhilog::seal` writes `end_vns` to header bytes `[168..176]`, *outside*
  the END record payload and *outside* `body_hash` (which covers `[HEADER_LEN..]`),
  and the reseal copies `header.end_vns` verbatim — so the byte-compare is
  tautological for `end_vns`. Masked today only because every test uses a 1:1
  clock (vns == icount). Add a `ClockRatio::new(2, 1)` replay test so the check has
  teeth (it is also the only path where `out.vns != out.boundary.icount`).
  Consider a `ReplayError::VnsMismatch { expected: u64, got: u64 }` variant — the
  existing `Divergence { expected: [u8;32], got: [u8;32] }` cannot carry a u64.

- [ ] **Pin or scope-cut the GuestHalted tail.**
  `crates/dh-worker/src/replay_engine.rs`. `run_to` requires `out.reason ==
  BudgetReached`, but `stop_reason_from_u8` decodes byte 6 (`GuestHalted`) and the
  reseal synthesizes a halted outcome — so the executor *claims* GuestHalted
  support that no test exercises and that survives only because `land_at` returns
  at `c == target` at an instruction start (landing one instruction *before* the
  HLT exit, hence `BudgetReached`). This breaks for an intermediate quantum that
  legitimately halts short of its target (batched guests: `entropy_draw`/`pipeline
  _smoke` HLT per batch). Either (a) accept `out.reason == GuestHalted` at
  `out.boundary.icount == header.end_icount` in the tail when the header's
  `stop_reason` is `GuestHalted`, or (b) make `stop_reason_from_u8(6)` (or the
  tail) return `ReplayError::NotYetWired("GuestHalted replay")` so the reseal
  cannot claim an unverified halt. Add a live test recording a halting guest and
  replaying it to pin whichever behavior is chosen.

### Suggestions

- [ ] **Stop dropping divergence detail.** `replay_engine.rs`. The EPOCH_HASH
  mismatch message built in the sink (the detailed `expected/got` first-4-bytes
  string) is discarded by the outer `map_err`, which substitutes `"EPOCH_HASH (see
  message)"` with zeroed `expected/got` — pointing "see message" at a message no
  longer reachable. Give `Divergence` an owned `String` detail or per-kind
  variants so real corpus divergences report the actual mismatching bytes.

- [ ] **Fix the misleading `expected: header.body_hash` on the byte-compare
  branch.** `replay_engine.rs` (resealed-bytes divergence). `body_hash` is the
  input's body hash, not "what was expected of the reseal"; a reader misreads it
  as a hash comparison. Either compute+report both body hashes, or zero the field
  with a clearer `what`. This is the one place a real reseal divergence lands.

- [ ] **Comment the deliberate redundant `counter.read()` in `run_to`.**
  `replay_engine.rs`. The per-call `start` re-read self-syncs to the real counter
  and is intentional; a one-line note prevents a "dead read" cleanup.

- [ ] **Tighten the `epoch_hashes_verified == 10` comment.**
  `crates/dh-worker/tests/replay_engine.rs` (~line 810). Derive it from the
  *absolute* grid (`300_000 / 30_000 = 10`, with the 300k epoch coinciding with
  the final budget stop), not the muddled per-quantum `100k/30k` framing.

- [ ] **Note the reseal hammer's full-log re-serialization cost** in the module
  doc as a known trade and a candidate verify-mode gate for very large logs.

---

## a5e readiness (per `bd show determinism-hypervisor-a5e`)

`determinism-hypervisor-a5e` (M5 ACCEPT: record/replay 60s-vns scripted pad
sequence, x100, every EPOCH_HASH equal — **P0**, gates all M6 hw-acceptance) lists
39w (this iteration) as its last open dependency `◐`; all others are `✓`. With 39w
landed, **a5e can start**, but two pieces it requires are not in this iteration and
must be built for a5e itself:

1. **60-guest-second vns scale.** a5e specifies *60 guest-seconds of vns*; the
   joint test here runs `3 × 100k = 300k` icounts on a 1:1 clock. a5e needs the
   real vns scale, which means a non-1:1 clock or a long run — and that is exactly
   the regime where **Important item I1 (`end_vns` unverified)** stops being
   masked. Close I1 before a5e, or a5e's "every EPOCH_HASH equal" passes while
   `end_vns` rides unchecked.

2. **The x100 zero-divergence harness.** a5e demands a *seeded scripted pad
   sequence repeated 100x with zero divergence* in `tests/determinism`. The
   current test runs each leg once. a5e needs the loop harness plus a
   deterministic scripted-input generator (the pad sequence) — neither exists yet.

Neither gap is a defect in 39w; both are a5e's own remaining build. 39w gives a5e
a correct, byte-identical single-pass replay primitive to wrap.
