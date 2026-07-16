# Suggestions

These are optional polish items. None block merge.

## S1 — `variant-count pin at 8` is the right call, but pin it against a constant the proto owns, not a literal

`every_proto_stop_reason_fits_the_u8_slot` ends with `assert_eq!(seen, 8, …)`. This
is good and *should* exist — it is the tripwire that fires when someone adds a 9th
proto `StopReason`, forcing them to look at the END `u8` carrier. It is not brittle
in the bad sense: a deliberate guard that fails loudly on intended-but-unreviewed
change is exactly what a mirror pin is for, and the failure message
("proto StopReason variant count moved") tells the next person what to do.

The only mild fragility is that `8` is a bare literal duplicated in spirit with
dh-proto's per-variant pins (`Faulted as i32 == 7` at `dh-proto/src/lib.rs:162`). If
proto ever gains a `StopReason` whose number is `> 255` (it won't for an enum, but
the test is written defensively over `0..=255`), the count and the fit assertions
both move and someone has to reconcile two crates. That is acceptable. If you want to
reduce the literal, a brief inline comment cross-referencing
`dh-proto`'s `Faulted as i32 == 7` pin (so a reader knows the two are intentionally
coupled) would help the next maintainer who trips it. Low value; take it or leave it.

## S2 — Consider a short doc note that the proto-number redundancy with dh-proto is intentional

`stop_reason_wire_numbers_are_pinned` re-asserts proto wire numbers that
`dh-proto/src/lib.rs:155-162` already pins. This is **complementary, not redundant**:
dh-proto pins the *number of the proto variant*, whereas proto_map pins *the mapping*
(domain `R::GoalSatisfied` → the proto variant whose number is 2). Both must hold; one
does not subsume the other. A one-line comment in the proto_map test saying so
("dh-proto pins the numbers; this pins the domain→proto routing to those numbers")
would preempt a future reviewer "deduplicating" the two and losing the routing pin.

## S3 — Reverse direction (proto → domain) is correctly out of scope here; note it for ol1/client work

There is no `proto_slot_state_to_domain` / `proto_stop_reason_to_domain`. For sr5's
and ol1's *serving* needs this is correct — the worker only ever emits domain→proto
when filling `SlotInfo.state` / `RunResponse.reason`; the worker never needs to parse
a proto enum *back* into a domain enum. The reverse direction is a *client*-side
concern (a control-plane consumer decoding `ListSlots`/`WatchSlots`), and that client
does not live in this repo's serving path. So one-way is sufficient for ol1.

Worth a one-line follow-up bead noting that if/when an in-repo client or a verify-path
round-trip needs proto→domain, it must be a hand-written match too (the same offset/
order trap applies in reverse, and the proto `*_UNSPECIFIED`/`*_S` variants have no
domain home, so the reverse fn must return a `Result`/error on those). Not needed now.

## S4 — `stop_reason_to_proto` has no fixture-coupling test analogous to the inputlog one

The `stop_reason_mirror.rs` test couples the *inputlog* byte to the proto variant via
golden fixtures, which is the load-bearing API.md §3.3 claim. The proto_map
`stop_reason_to_proto` is pinned only at the wire-number level, not against any
runctl→END round-trip. That is fine because runctl's `SegmentOutcome.reason` does not
yet flow into an END record in this change (that wiring is downstream). When the run
loop does seal an END record from a `SegmentOutcome`, a single round-trip test
(`run_segment` outcome → END byte → `StopReason::try_from` == proto-mirror-of-the-
domain-reason) would close the last seam. Out of scope here; note for the run-loop bead.
