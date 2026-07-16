# Suggestions (non-blocking)

## 1. Add a standing naming convention for future enum-value authors

`proto/hypervisor.proto:441-445` and API.md §2.8 now both explain *why* `PAUSED_S`
exists for `SlotState`. But the explanation is anchored to one enum. The real hazard
is the **next** enum author who reuses a generic value name that happens to be unique
today: `RUNNING`, `FROZEN`, `EMPTY` (all in `SlotState`), `COOP`/`FORCED`
(`QuiesceMode`), `EQ`/`NE`/`GE`/`LE` (`Op`) are all unprefixed and would silently
collide if a future enum reused them. There is no single place that states the rule as
forward guidance.

Suggested: a one-line standing convention near the top of the proto, e.g.

```proto
// CONVENTION: proto3 enum values are PACKAGE-scoped (C++ rules), not enum-scoped.
// Every value name must be unique across ALL enums in this package. Prefix new
// values with the enum name (SLOT_*, HASH_EPOCHS_*, ...) — bare names like PAUSED
// forced the SlotState.PAUSED_S workaround. protoc rejects collisions at build time.
```

This is cheap insurance: the cost of getting it wrong is a confusing protoc error in
a future PR, and the convention is the only thing that scales past the two values
(`PAUSED_S`, `FAULTED_S`) currently carrying the suffix.

## 2. Test pins cover only 2 of the 6 enums; the unpinned numbers are equally normative

`full_surface_message_shapes` pins `StopReason::Paused==5`, `SlotState::PausedS==2`,
`SlotState::FaultedS==5`. These are the highest-risk values (the collision-disambiguated
ones), so the choice is reasonable. But API.md pins concrete numbers for *every* enum,
and a future careless reorder of, say, `PixelFormat` or `HashEpochs` would compile and
pass all tests while silently breaking the wire contract with already-snapshotted
`MachineConfig`/`FbInfo` bytes.

Consider adding a compact block:

```rust
// Enum number pins — every value is wire-normative (API.md §2).
assert_eq!(v1::HashEpochs::EpochsOn as i32, 1);
assert_eq!(v1::HashEpochs::FinalOnly as i32, 2);
assert_eq!(v1::PixelFormat::Xrgb8888 as i32, 1);
assert_eq!(v1::PixelFormat::Rgb565 as i32, 2);
assert_eq!(v1::StopReason::BudgetReached as i32, 1);
assert_eq!(v1::StopReason::HardCap as i32, 4);
assert_eq!(v1::StopReason::Faulted as i32, 7);
assert_eq!(v1::QuiesceMode::Coop as i32, 1);
assert_eq!(v1::QuiesceMode::Forced as i32, 2);
assert_eq!(v1::mem_predicate::Op::Ge as i32, 3);
assert_eq!(v1::mem_predicate::Op::Le as i32, 4);
```

`Op::Eq` is already indirectly pinned (used as `Op::Eq as i32` in the GoalCondition
round-trip), but an explicit number assert is clearer intent.

## 3. `StopReason`/`SlotState` zero-values are pinned only by round-trips, not directly

`STOP_UNSPECIFIED = 0` and `SLOT_UNSPECIFIED = 0` are the proto3-required zero
defaults; they are exercised by `..default()` round-trips but never asserted. Low
priority — proto3 hard-requires a zero value so a reorder to non-zero is impossible —
but if #2 is taken, adding `assert_eq!(v1::StopReason::StopUnspecified as i32, 0)`
costs nothing and documents the contract floor.

## 4. The `stop_reason: u8 (mirrors proto StopReason)` coupling in API.md §11 (DHILOG END record)

API.md line ~575 says the DHILOG `END` record's `stop_reason: u8` "mirrors proto
StopReason". That mirror is now a cross-format invariant: any future StopReason
renumber must stay ≤ 255 and stay in sync with the DHILOG golden-bytes fixtures
(iteration 62). Not actionable in this diff, but worth a bead so the coupling is
tracked rather than tribal knowledge — a StopReason reorder would need to touch both
the proto pins (#2) and the DHILOG reader/fixtures.
