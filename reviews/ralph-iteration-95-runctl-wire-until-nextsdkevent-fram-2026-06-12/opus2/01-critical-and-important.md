# Critical and Important Findings

**None.**

I probed every implicit assumption called out in the review brief and several of my own,
and found no Critical or Important defects. The notes below record the verification so a
future reader does not have to re-derive it.

## Verified — not defects

### V1. Event-stop boundary is replay-deterministic (the load-bearing claim)
`finish_at_counter` reads `seg.counter.read()` AFTER the triggering exit is serviced but
BEFORE re-entry. Per `crates/dh-vmm/src/boundary.rs` ("an instruction that exited
mid-emulation has not retired; the exit is serviced and the count is unchanged"), the
FRAME_COUNTER MMIO write / doorbell exit has not retired at that point, so the boundary
icount is the count of instructions strictly before the writing instruction. This is the
SAME invariant the pre-existing HLT stop (`finish_halted`) relied on and that m1
acceptance (`m1_acceptance.rs:256`, `StopReason::GuestHalted`) depends on. KVM completes
the un-retired write on the next entry — which is the next segment's first entry — so the
write is not lost. Determinism holds on both legs. **Not a bug.**

### V2. No frame double-count on retry after a non-sentinel BoundaryError
`land_at` and `step_one_entry` propagate an `on_exit` `Err` immediately (`on_exit(exit)?`
/ `break Err(e)`) — there is no loop path that re-services the same exit. ARCH §6.6
(normative): "one exit per frame". pad.rs accepts only a single 4-byte FRAME_COUNTER
write per `mmio_write`. So `frames_seen += 1` (runctl.rs:344) fires exactly once per
frame. **Not a bug.**

### V3. `halted` and `event_stop` cannot both be true
In the `exits!` macro (runctl.rs:335–356), `VcpuExit::Hlt` sets `halted` and returns
before the frame/sdk checks; those checks only run on non-HLT exits. The first flag set
returns `Err`, unwinding the flight. The unwind sites check `halted` first, but since the
two are mutually exclusive within a flight, ordering is immaterial. **Not a bug.**

### V4. `FrameBudget(0)` does not double-link vs `IcountBudget(0)`
`agenda.rs` `compile` for a zero budget emits a single final-stop point at `start_icount`
with `epoch_hash = false` even when `start_icount` is on the epoch grid (the grid loop's
first candidate is `(start/len + 1)*len > start`, immediately past `final_icount = start`).
`finish()` therefore pushes exactly one link there (`already_hashed = false`).
`FrameBudget(0)` (runctl.rs:293) routes through `finish_at_counter(…, already_hashed =
false)`, also pushing exactly one link. Both paths: one link. **Not a bug.**

### V5. `frames_elapsed` in derived `PartialEq`/`Eq` does not break outcome comparisons
- `m4_transparency.rs:194` (`r1==c1`), `:262` (`r2==c2`), `:421` (`out_a==out_b`) compare
  two runs that both use `Until::IcountBudget` with `on_exit = |exit| Err(...)` and the
  `landing_loop` guest (never writes FRAME_COUNTER) ⇒ `frames_elapsed == 0` on both sides.
- `replay_engine.rs` never compares a full `SegmentOutcome`; it checks `reason`,
  `boundary.icount`, and `vns` individually (`require_landed`, `reason_ok`). Its
  `outcome_like` with hardcoded `frames_elapsed: 0` (replay_engine.rs:367) is only fed to
  `seal()`, which does not read the field. **Not a bug.**

### V6. dh-cli cannot trip `MissingSdkEventFeed`
dh-cli constructs only `Until::IcountBudget` / `Until::VnsBudget` (cli.rs, gate.rs); it
never builds `Until::NextSdkEvent`, so `sdk_events: None` is unreachable as an error path
from the CLI. The new `run.rs` match arm only maps the `StopReason::NextSdkEvent` output
label. **Not a bug.**

### V7. SDK feed: multiple matching events in one drain, and pre-segment bumps
The baseline is read ONCE at segment start (runctl.rs:282). The stop condition is
`cell.get() > baseline` — a strict rise — so several matching events bumped inside a
single doorbell drain still produce exactly one stop (the count beyond 1 is irrelevant;
the caller returns the matching event). A bump that happened BEFORE the segment is folded
into the baseline, so it does not spuriously stop the new segment. In `FrameBudget` mode
`sdk_feed` is `None`, so cell activity is ignored entirely (the modes are mutually
exclusive `Until` variants). **Not a bug.**
