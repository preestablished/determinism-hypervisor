# Critical & Important findings

## Critical

None. The change has no correctness defects. Build, clippy, and all
`slot_state_tests` pass; the edge set is fail-closed and exhaustively tested.

## Important

### I1. No `StopReason::Faulted` *producer* — the new `*→Faulted` edges have no driver

The new doc comment grounds the fault transitions in
`StopReason::FAULTED` ("proto FAULTED_S / StopReason::FAULTED"). But the Rust
`StopReason` enum that run control actually emits
(`crates/dh-vmm/src/runctl.rs:46-55`) has **no `Faulted` variant**:

```rust
/// Why the segment stopped (mirrors proto StopReason).
pub enum StopReason {
    BudgetReached,
    GoalSatisfied,
    HardCap,
    Paused,
    GuestHalted,   // proto GUEST_HALTED = 6
}
```

That is 5 variants against the proto's 7 (`NEXT_SDK_EVENT` and `FAULTED` are
both absent). The comment claims the enum "mirrors proto StopReason", which is
already not literally true (NextSdkEvent is deferred with the device run loop),
but `FAULTED` is the more interesting gap *for this change*: there is currently
**no run-control code path that can return a fault outcome**, so nothing can
ever drive a `Running → Faulted` or `Paused → Faulted` transition. The
`runctl.rs:235` comment ("Terminal HLT … is a STOP, not a fault") shows the
author has thought about the stop/fault distinction, but the fault branch was
never built — `GuestHalted` maps to proto `GUEST_HALTED`, and a triple fault is
folded into `GuestHalted` per the proto comment, not into a fault.

This is not a defect in *this* diff — the state machine is a pure relation and
landing it before the producer is the correct sequencing (it must precede bead
ol1's slot table). But the change is, by design, currently unreachable. It
should be tracked so the producer side is not forgotten:

- When run control gains a fault outcome (divergence detected, log `DATA_LOSS`
  at a boundary, counter revocation), `runctl::StopReason` needs a `Faulted`
  variant and the boundary/verification path needs to emit it.
- The slot-table integration (bead ol1) must map that `StopReason::Faulted`
  onto `SlotState::transition(.., Faulted)`.

Without both, the `Faulted` state is decorative. Recommend an explicit bead
(see action items) so the edges added here acquire a caller. **Important, not
blocking** — the integration note at `lib.rs:137-140` already commits future
call sites to adopting the guard, and this enum-only change is the right unit.

### I2. No `dh-vmm SlotState ↔ proto SlotState` mirror pin (parallel to bead sr5)

Bead `sr5` exists precisely because API.md §3.3 says the DHILOG `stop_reason`
u8 "mirrors proto StopReason" yet nothing asserts the *mirror* — both sides are
individually frozen but no cross-crate test couples them. The `SlotState` side
has the **same shape of gap, and it is currently weaker than the StopReason
side**:

- **proto side is pinned:** `crates/dh-proto/src/lib.rs:163-168` asserts
  `SlotState::SlotUnspecified=0, Empty=1, PausedS=2, Running=3, Frozen=4,
  FaultedS=5`. Good.
- **dh-vmm side is now pinned by ordering:** the `transition_matrix` test pins
  the edge relation, and the `ALL` array fixes the variant set.
- **nothing couples the two.** There is no `From`/`TryFrom`, no conversion
  function, and no test asserting that dh-vmm's `SlotState` maps onto proto
  `SlotState`. The two enums even **disagree on shape**: dh-vmm `SlotState` has
  *no* `Unspecified`/zero variant and its declaration order is
  `Empty, Running, Paused, Frozen, Faulted` (so `Running as i32 == 1`), whereas
  proto is `…EMPTY=1, PAUSED_S=2, RUNNING=3…` (`Running == 3`). Their `as i32`
  values do **not** line up.

So unlike the StopReason case (where the two enums at least share an ordering),
here a naive `slot_state as i32` cast into a proto field would be **silently
wrong** (dh-vmm `Running`=1 → proto `EMPTY`=1). When bead ol1 wires
`SlotInfo.state` (API.md §2.8, `SlotState state = 2`) from the engine's
`SlotState`, it MUST go through an explicit match, not a cast, and a mirror test
must pin that match. This is exactly the sr5 hazard one type over, and it is
worth filing now while the divergence is fresh — before ol1 lands a cast.

Recommend a bead analogous to sr5 (see action items). **Important** because the
failure mode (reporting the wrong slot state over gRPC) is silent and would
surface only as a confusing operator-facing bug, not a test failure.
