Actions taken:

- Split snapshot precondition validation so public `take_snapshot` still requires
  `agenda_empty`, while `capture_bisection_checkpoint_snapshot` requires only `Paused`.
- Updated snapshot-engine coverage to prove checkpoint capture succeeds with
  `agenda_empty = false`.
- Added a paired service-level equivalence test with a future queued input: one leg captures a
  bisection checkpoint at the pause boundary and the other does not, then both continue and seal.
- The new test compares final state hash, final snapshot ref, input log id, sealed DHILOG bytes,
  slot icount, and slot base snapshot lineage.
