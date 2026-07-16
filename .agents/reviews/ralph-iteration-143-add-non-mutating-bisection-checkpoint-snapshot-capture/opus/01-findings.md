Findings:

1. P1: checkpoint capture incorrectly inherits TakeSnapshot's agenda-empty precondition.
   `capture_bisection_checkpoint_snapshot` rejected `BoundaryState { agenda_empty: false }`.
   This is too strict for non-mutating checkpoint evidence because recorder checkpoints can be
   needed while future inputs remain queued.

2. P2: capture-vs-no-capture execution equivalence coverage was missing.
   Existing tests proved immediate field stability and full/parentless storage, but did not run
   a paired control leg through the next normal run/snapshot/log seal boundary.
