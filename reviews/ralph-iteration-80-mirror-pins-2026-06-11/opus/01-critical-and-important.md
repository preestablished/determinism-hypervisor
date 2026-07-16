# Critical & Important Findings

## Critical

None.

I checked the obvious correctness hazards and found none:

- **Wire-number correctness.** Every arm in `slot_state_to_proto` and
  `stop_reason_to_proto` maps to the proto variant whose number matches API.md /
  `proto/hypervisor.proto`. Verified by running the in-crate pins
  (`slot_state_wire_numbers_are_pinned`, `stop_reason_wire_numbers_are_pinned`) —
  both pass on x86_64.
- **`u8`-fit invariant.** `every_proto_stop_reason_fits_the_u8_slot` confirms all 8
  proto `StopReason` values fit the END record's single `u8` slot (max value today
  is `FAULTED=7`). Passes.
- **Fixture coupling.** `golden_fixture_stop_reasons_decode_to_the_intended_proto_variants`
  decodes the *actual frozen bytes* in `v1_kitchen_sink.dhilog` and `v1_minimal.dhilog`
  through `LogReader::end()` and asserts they resolve to `GoalSatisfied` and
  `StopUnspecified`. Passes — the cross-crate coupling is real, not asserted against
  a hand-typed constant.

## Important

### I1 — The iteration-79 "grep-forbid `as i32` casts on SlotState" ask is discharged by convention, not by a CI gate (follow-up, non-blocking)

The iteration-79 extension to sr5 (recorded in the bead's NOTES) asked for two
things on the `SlotState` hazard:

1. *"Pin a hand-written match conversion … with a test"* — **done**, and done well
   (`slot_state_to_proto` + `slot_state_wire_numbers_are_pinned`).
2. *"grep-forbid `as i32` casts on SlotState"* — **not done as a gate.** Instead the
   change relies on (a) the module doc comment declaring "an `as i32` cast on a
   domain enum is ALWAYS a bug", and (b) the `lying_casts == 4` pin that *demonstrates*
   the bug class but does not *forbid* it elsewhere.

The lying-casts assertion is excellent pedagogy and a good regression tripwire for
the trap itself, but it is local to `proto_map.rs`. It does nothing to stop a future
ol1 commit from writing `slot.state as i32` directly into `SlotInfo.state` in some
other module — which is precisely the silent-mislabel failure the extension was
guarding against. The doc comment is advisory; nothing mechanically enforces it.

This repo already has the right enforcement primitive: `dh-devices` ships a
`no_host_ambient_authority` source-grep test (`crates/dh-devices/src/lib.rs:86`)
that walks `src/*.rs` and fails on forbidden token spellings, paired with a
`clippy.toml`. An analogous `no_slotstate_as_i32_cast` grep over `dh-worker/src`
(and eventually wherever ol1's slot table lives) would convert the convention into
a checked fact — the same upgrade this very change made for the `stop_reason`
mirror prose.

**Why Important, not Critical:** I grepped the entire tree. There are currently
**zero** `as i32` casts on `SlotState` anywhere (the only `as i32` occurrences are
in `proto_map.rs` itself — inside the pins, deliberately, on the proto result — and
unrelated `libc::syscall(...) as i32` / `KVM_API_VERSION as i32` uses). The first
real consumer of `slot_state_to_proto`, ol1's `SlotInfo.state` serialization, does
not yet exist (sr5 *blocks* ol1). So today there is no live bug; the risk is purely
forward-looking. That makes it a hardening follow-up rather than a blocker.

**Recommendation:** File a small follow-up (blocking ol1, or as an ol1 acceptance
criterion) to add a `dh-devices`-style source-grep test forbidding `<SlotState>
as i32` / `SlotState as i32` outside `proto_map.rs`, so ol1 physically cannot
reintroduce the cast it was warned about. Approve the current change as-is.
