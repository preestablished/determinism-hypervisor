Actions taken:

- Added `bisection_checkpoint_capture_is_execution_equivalent_to_no_capture`.
- The test creates one root snapshot, runs control and checkpoint legs from it, queues a future
  input past the checkpoint, captures only in one leg, continues both legs, then compares final
  state hash, final snapshot ref, input log id, sealed DHILOG bytes, dirty-page count, slot icount,
  and slot base snapshot id.
