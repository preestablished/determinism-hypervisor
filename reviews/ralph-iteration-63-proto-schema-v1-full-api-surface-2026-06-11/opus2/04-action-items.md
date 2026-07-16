# Action Items

## Critical

- [ ] None.

## Important

- [ ] None.

## Suggestions

- [ ] **Add a package-level enum-naming convention comment** to
  `proto/hypervisor.proto` (near the top) stating that proto3 enum values are
  PACKAGE-scoped, must be unique across all enums, and should be enum-prefixed —
  so future authors don't reuse a bare name (`RUNNING`, `FROZEN`, `EMPTY`, `EQ`...)
  and trip a fresh `PAUSED_S`-style collision. Forward-guards the next enum author;
  no code/wire change. (See `02-suggestions.md` #1.)

- [ ] **Extend `full_surface_message_shapes`** (`crates/dh-proto/src/lib.rs`) with
  direct number pins for the enums API.md numbers but the test doesn't:
  `HashEpochs` (1,2), `PixelFormat` (1,2), `StopReason` (1,4,7),
  `QuiesceMode` (1,2), `mem_predicate::Op` (3,4). A silent renumber of any of these
  would currently compile and pass. Snippet in `02-suggestions.md` #2.

- [ ] **(Optional, with the above)** pin the two `*_UNSPECIFIED = 0` floors
  (`StopReason::StopUnspecified`, `SlotState::SlotUnspecified`) for completeness.
  (See `02-suggestions.md` #3.)

- [ ] **File a tracking bead for the DHILOG ↔ proto `StopReason` mirror coupling**
  (API.md §11 END record `stop_reason: u8` "mirrors proto StopReason"): a future
  StopReason renumber must stay in sync with the iteration-62 DHILOG golden-bytes
  fixtures. Not a defect here; just untracked tribal knowledge. (See
  `02-suggestions.md` #4.)
