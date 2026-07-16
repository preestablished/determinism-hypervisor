# Critical and Important Findings

## Critical

None. The two pre-flagged bug candidates (RefCell re-entrancy, ENTR v2 split)
are clean (see `03-positive-notes.md`), and the GuestHalted-tail concern is
downgraded to Important after tracing `land_at` (below).

---

## Important

### I1 — `end_vns` is structurally unverified by replay (masked by 1:1 clock)

**Files:** `crates/dh-worker/src/replay_engine.rs` (tail run-out + reseal block,
lines ~281–327); `crates/dh-inputlog/src/dhilog.rs::seal` (lines 325–364).

**The gap.** Replay verifies `end_state_hash` (live `chain.value()` vs
`header.end_state_hash`) and every `EPOCH_HASH`, but it **never verifies
`end_vns`**. The tail quantum's actual end-of-run vns *is* computed —
`run_segment_with_epochs` returns it as `out.vns` — but `run_to` discards `out`
entirely except for the `out.reason == BudgetReached` check (line ~231). The
reseal then constructs `outcome_like.vns = header.end_vns` (line ~311), copying
the input header's value verbatim.

**Why the reseal hammer does not cover it.** I traced `seal()` in `dhilog.rs`:
`end_vns` is written to header bytes `[168..176]` (line 358) and is **not** part
of the END record payload (payload at lines 326–328 carries only `stop_reason`
and `end_state_hash`). `body_hash = blake3(buf[HEADER_LEN..])` (line 361) covers
only the *records*, not the header. So the `resealed != log_bytes` byte-compare
(replay_engine line ~320) *does* include the header `end_vns` bytes — but because
replay copied `header.end_vns` into the reseal, both sides carry the identical
value **by construction**. The comparison is tautological for `end_vns`; a
recording with a wrong/corrupt `end_vns` (or a genuine vns divergence under a
non-1:1 clock) replays "successfully".

**Why it is masked today.** Every test uses `clock_num = clock_den = 1`, so
`vns == icount` and `end_vns == end_icount`, and `end_icount` *is* checked (the
tail `run_to(header.end_icount)` lands there or errors). The gap only bites a
non-1:1 clock recording — which is precisely where the conversion math could
diverge and where you most want the check.

**Severity rationale.** Not Critical: it cannot cause replay to *accept* a run
whose **state** diverged (the chain check still holds, and `vns` feeds the chain
links via `push_final_link(.., vns)`, so a wrong tail vns that mattered to state
would surface in `end_state_hash`). It *can* let a wrong `end_vns` header field —
or a clock-conversion bug whose effect is confined to the reported vns and not
the hashed state — pass undetected. For a P0 replay path the right posture is to
verify it, not rely on the chain to catch a subset of it.

**Fix.** Capture the tail outcome and compare. In `run_to`, return `out.vns` for
the final quantum (or have the tail run-out use a dedicated variant that yields
the `SegmentOutcome`), then before the reseal:

```rust
if tail_out.vns != header.end_vns {
    return Err(ReplayError::Divergence {
        what: "end_vns",
        at_icount: header.end_icount,
        expected: [0; 32], // numeric — see message / dedicated fields
        got: [0; 32],
    });
}
```

Consider giving `Divergence` an optional numeric pair, or a separate
`ReplayError::VnsMismatch { expected, got }`, so the vns values are reportable
(the current `[u8; 32]` expected/got fields cannot carry a `u64` cleanly). Add a
non-1:1-clock replay test (e.g. `ClockRatio::new(2, 1)`) so the check has teeth —
note this also exercises the only path where `out.vns != out.boundary.icount`.

---

### I2 — GuestHalted tail survives only by an unpinned boundary-timing coincidence

**Files:** `crates/dh-worker/src/replay_engine.rs` (`run_to` reason check,
line ~231; tail run-out, line ~282); `crates/dh-vmm/src/runctl.rs`
(`run_segment_with_epochs` HLT handling, lines 263–292, 458–491);
`crates/dh-vmm/src/boundary.rs::land_at` (lines 109–135).

**The concern, restated.** `run_to` insists `out.reason == StopReason::Budget
Reached` and errors otherwise. `stop_reason_from_u8` accepts byte 6
(`GuestHalted`) and the reseal happily synthesizes a `GuestHalted` outcome — so a
GuestHalted recording is *in scope* for the reseal but the tail `run_to` only
tolerates `BudgetReached`. On its face: replaying an honestly-halted segment errs
in `run_to`.

**Why it does not fire for the halt-boundary shape.** I traced `land_at`: it
returns at `c == target` at an **instruction start** (lines 123–135), and HLT
only produces a `VcpuExit::Hlt` when KVM tries to *execute* it. A recording sealed
`GuestHalted` via `finish_halted` stamps `end_icount = counter.read()` taken
*after* the HLT exit; since HLT does not retire, that icount equals the HLT
instruction's start boundary. The replay tail's `IcountBudget(end_icount - start)`
therefore lands at the HLT boundary **before** executing it → `BudgetReached`, not
`GuestHalted`. So for a guest that halts exactly at the sealed `end_icount`, the
tail does *not* error. The reseal then stamps `stop_reason = 6` from the header
(synthesized, fine), and byte-identity holds.

**Why it is still Important.** This correctness rests on a subtle, unpinned
coincidence — "the budget boundary always sits one instruction before the HLT
exit". Two failure modes are unguarded:

1. **Intermediate quantum hits a HLT before its target.** If a future recording
   carries a canonical record *after* a point where the guest could halt (batched
   guests — `entropy_draw` HLTs every batch per the runctl doc comment and the
   `segments_resume_past_a_halt_live` test), an intermediate `run_to(record_icount)`
   could stop `GuestHalted` short of its target and error as a misleading
   `ReplayError::Run("expected to land at … stopped GuestHalted …")` — which reads
   like a replay fault, not the "this segment legitimately halted here" case.
2. **No test pins the halt-boundary behavior.** pad-echo never halts, so nothing
   in the suite exercises a `GuestHalted` recording through replay. The
   coincidence above is asserted nowhere; a boundary-engine change that made
   `land_at` run one instruction further (or a counting change around HLT) would
   silently turn the tail into an error path with no regression catching it.

**Severity rationale.** Not Critical because the only in-scope guest (pad-echo,
a5e) never halts and the reseal path is otherwise correct. But the replay
executor *advertises* `GuestHalted` support (it decodes byte 6 and reseals it),
and that support is unverified and brittle. Either constrain the claim or test it.

**Fix (pick one):**
- **Minimal / honest-scope:** in the tail run-out, when the header's
  `stop_reason` is `GuestHalted`, accept `out.reason == GuestHalted` at
  `out.boundary.icount == header.end_icount` as success (in addition to
  `BudgetReached`). Same for an intermediate quantum that legitimately halts at a
  recorded boundary — though for v1 you may simply reject a record *after* a halt
  as a malformed log.
- **Or scope-cut loudly:** if GuestHalted replay is genuinely out of v1 scope,
  make `stop_reason_from_u8(6)` (or the tail) return `ReplayError::NotYetWired
  ("GuestHalted replay")` rather than silently decoding it, so the reseal cannot
  claim a halt it never verified.
- **Either way:** add a live test that records a halting guest
  (`pipeline_smoke`/`entropy_draw`) and replays it, pinning the chosen behavior.
