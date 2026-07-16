# Critical & Important Findings

**None.**

I worked through every review priority and found no blocking issue. The
reasoning that cleared each one is recorded below so a future reader does not
have to re-derive it.

## Priority 1 — Determinism of the stop boundary (CLEARED)

Both stops are a deterministic function of guest execution:

- **FrameBudget:** the frame mark is the guest's own `MmioWrite` to
  `PV_PAD_BASE + REG_FRAME_COUNTER` (`runctl.rs:272`, `:340-341`). It is decoded
  by a pure `matches!` against device-side constants — no host/wall-clock input —
  and counted once per exit. Record and replay see the identical sequence of
  MMIO writes at identical icounts, so the Nth mark lands at the same boundary.
- **NextSdkEvent:** the stop is keyed off a *rise* in `Segment::sdk_events`
  during the segment (`:350-354`). The cell is bumped by the caller's `on_exit`
  for a matching drained event, i.e. a guest-initiated doorbell exit — itself at a
  fixed icount. The baseline is captured at segment start (`:280-283`) so only a
  rise within THIS segment stops it; a cumulative feed carried across chained
  segments will not false-trigger. Determinism here is contractual: it depends on
  the caller's matching being a pure function of guest state (the same burden
  already documented for `goal()` at `:492-494`). That contract is the rail's, not
  runctl's, and is correctly out of scope for this change.

`frames_seen` is counted in EVERY mode (not just FrameBudget), so the
`frames_elapsed` field is well-defined for an SDK-event or budget stop too.

## Priority 2 — Unwind coverage & priority order (CLEARED)

All four flight sites that run the guest under `exits!()` have BOTH a `halted`
and an `event_stop` arm, in that order, ahead of the real-error arm:

- `land_at` in the agenda walk — `:373-386`
- `step_one_entry` between chained injections — `:406-426`
- `inject_at_boundary` mid-deferral — `:442-462`
- the pause roll-forward `land_at` — `:539-559`

Priority order is correct: `halted` is checked before `event_stop`. The two can
never be true together — each is set immediately before its own sentinel
`return Err(...)`, so the wrapper unwinds the instant either fires; no second
flight runs. A real `BoundaryError` cannot be masked by `event_stop`, because the
flag is only ever set on the same exit-handler invocation that returns the
sentinel — there is no path where a flag is left set while the flight keeps going
and later hits a genuine error. (`finish_at_counter` re-reads the counter and
regs directly, so it does not depend on the wrapped error carrying a boundary.)

## Priority 3 — `exits!` ordering & borrow soundness (CLEARED)

`on_exit(exit)?` runs BEFORE frame counting (`:342`). This is the intended
order: a genuine device error from the rail surfaces as the real error, and the
device state (FRAME_MARK logged / doorbell drained) is current at the moment the
stop is flagged. The `frame_mark` bool is computed by `matches!` on a borrow of
`exit` (`:340-341`) *before* `exit` is moved into `on_exit(exit)` — sound; the
borrow ends before the move. The macro is instantiated at four sites but each
expansion closes over the same `halted` / `event_stop` / `frames_seen` /
`frame_target` / `sdk_feed` bindings, so counting is consistent across sites
(Rust's borrow checker also guarantees only one `&mut` closure is live at a time).

## Priority 4 — Hash-chain integrity (CLEARED)

`finish_at_counter` always passes `already_hashed = false` (`:651-660`), so
`finish` pushes one final link at the event boundary. Could that double-link a
point already hashed this walk? No: the event stop unwinds from INSIDE a flight
(`land_at` / `step_one_entry` / `inject_at_boundary`), i.e. strictly BETWEEN
agenda points. The epoch-grid `push_final_link` only happens AT an agenda point
with `point.epoch_hash` (`:483-490`), which is reached only after the flight to
that point returns `Ok`. An event stop returns `Err` from the flight before any
such point is processed, so the boundary it lands on was never linked. This is
exactly how the pre-existing HLT path behaved; the generalization preserves it.
The `GoalSatisfied` / `final_stop` paths differ only because they fire AT an
agenda point and therefore correctly forward `point.epoch_hash` — not applicable
to a mid-flight unwind.

## Priority 5 — API conformance (CLEARED)

Matches `proto/hypervisor.proto` (source of truth) and the API.md §2.4 mirror:
`frame_budget` → `BUDGET_REACHED`; `next_sdk_event` → `NEXT_SDK_EVENT = 3`;
`frames_elapsed` == `frames` on a BudgetReached frame run (asserted by
`frame_budget_stops_on_the_nth_frame_mark_live`). The `hard_cap 0 ⇒ worker
default (10e9)` mapping is correctly the WORKER's job, not runctl's: runctl takes
`hard_cap` as a literal `FinalStop::HardCap` and the `RunRequest → Until`
translation (where the 0-default lives) is a separate, not-yet-present layer.
This is mentioned in the proto comment (`hard_icount_cap … 0 ⇒ worker default`);
see suggestion S3 about making that boundary explicit in the runctl docstring.

## Priority 6 — proto/byte mappings (CLEARED)

`recording.rs:46` `stop_reason_u8(NextSdkEvent) = 3` matches
`proto_map.rs:41` `NextSdkEvent → proto::StopReason::NextSdkEvent` and the
`stop_reason_wire_numbers_are_pinned` pin `(NextSdkEvent, 3)`, all cross-checked
against `proto/hypervisor.proto:231` `NEXT_SDK_EVENT = 3`. The
`recording_end_byte_agrees_with_the_proto_mapping` test enforces END-byte ==
proto for every variant, and both `match`es are exhaustive (no `_` arm), so a
future renumber breaks loudly. Verified passing.

## Priority 7 — Test quality (CLEARED)

The live tests genuinely pin the semantics: run-twice identity on
`(icount, rip, state_hash)` for both frame and SDK stops; `frames==0` stops at
icount 0 with no entry; both modes fall through to `HARD_CAP` at the exact cap
when no event fires; the missing-feed case errs with `MissingSdkEventFeed`. The
stop-on-event-not-cap invariant is asserted (`out.boundary.icount < cap`). See
suggestion S1 for one assertion that would harden the SDK test further.
