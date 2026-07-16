# Positive Notes

## Generalizing the HLT sentinel-unwind instead of inventing a new mechanism

`finish_halted` → `finish_at_counter(reason, …)` (`runctl.rs:621-661`) is exactly
the right move: the event stops have the same shape as a terminal HLT (the guest
stops the flight mid-air at a deterministic icount), so they reuse the same
counter-and-regs read for the boundary. One unwind path, one set of arms, no new
control-flow concept. This keeps the determinism argument identical to the
already-proven HLT path.

## "The flags, not the wrapper, are the truth"

The comment at `:429-431` and the consistent `Err(_) if halted` / `Err(_) if
event_stop` arms across all four flight sites capture the key insight cleanly: the
sentinel `BoundaryError` is just an unwind vehicle; the booleans set immediately
before each `return Err` carry the meaning. This makes it impossible for a
sentinel to be confused with a genuine error, and the `inject_at_boundary` path
correctly handles the case where the stop surfaces *inside* the injection
deferral (a subtlety easy to miss).

## `on_exit(exit)?` before counting — device state current at the stop

Servicing the exit FIRST (`:342`) means that when a frame mark or SDK event
flags the stop, the rail has already logged the FRAME_MARK / drained the
doorbell at that exit's icount. The stop boundary IS the exit boundary with
device state already applied — precisely what record/replay parity needs. The
docstring at `:324-329` explains this rather than leaving it implicit.

## `frames_seen` counted in every mode

Counting frame marks unconditionally (not only under `FrameBudget`) gives
`SegmentOutcome.frames_elapsed` a well-defined value for SDK-event, budget, goal,
and pause stops alike — matching the proto's "`FRAME_MARK count during this Run`"
semantics for `frames_elapsed = 5`, independent of the stop reason.

## Loud-failure design for the missing feed

Replacing `NotYetWired` with `MissingSdkEventFeed` (`:97-101`, `:277-283`) and
keeping the dedicated test (`next_sdk_event_without_a_feed_fails_loudly`) is the
right call: a caller-fed mode run without its feed would otherwise spin silently
to the hard cap and report `HARD_CAP` instead of the event — a wrong-reason bug
that is far worse than an early loud error. The error's `Display` even names the
exact field the caller forgot.

## Segment-start baseline for the SDK feed

Capturing the baseline at segment start (`:280-283`) and stopping only on a
*rise* is the correct semantics for a feed that may be cumulative across chained
segments — a stale carried count cannot false-trigger the next segment. The
inline comment states exactly why.

## Exhaustive, cross-pinned byte/proto mappings

Both `stop_reason_u8` and `stop_reason_to_proto` stay `_`-free, the
`recording_end_byte_agrees_with_the_proto_mapping` test couples the two crates
that cannot see each other's enums, and the wire-number pins lock `NextSdkEvent =
3` against `proto/hypervisor.proto`. A renumber on any side breaks a test at the
same commit. The proto-map docstring was also updated to record that
`NextSdkEvent` "landed exactly that way" via the exhaustiveness trap — good
provenance.

## Genuinely determinism-pinning live tests

The run-twice-identity assertions on `(icount, rip, state_hash)` for both stop
modes test the actual product property (replay identity), not just that the code
returns the right enum. The hard-cap-fallthrough tests pin the safety-net
behavior at the exact cap icount, and `frame_budget_zero` pins the no-entry
edge — the full edge surface is covered.
