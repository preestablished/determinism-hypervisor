# Review Overview

Branch: `ralph/iteration-126-dh-workerd-injectinputs-at-frame-and-dev`
Date: 2026-06-15
Reviewer: Claude Opus (2nd reviewer)

This change extends worker input scheduling so `InjectInputs` can carry either icount/vns-scheduled inputs or absolute `FRAME_COUNTER`-scheduled inputs, adds a `QueuedInputAt` representation for that split, records generic `DEV_EVENT` payloads through the device rail, and updates run control to fire frame-scheduled inputs when pv-pad `FRAME_COUNTER` MMIO exits are observed. The overall direction is sound, but the new public `at_frame`/`dev_event` surfaces currently rely on assumptions that are not enforced: frame-scheduled vectored inputs fault after mutation, `at_frame` is accepted on machines that cannot ever produce frame marks, frame counter monotonicity is still trusted rather than checked, and `dev_event` can now produce logs that replay still rejects.

Overall verdict: REQUEST_CHANGES

Stats:
- Files changed: 4
- Lines added/removed: 417 insertions, 35 deletions
- Commits: 1 (`987f1c0 ralph: iteration 126 checkpoint - inject input schema tails`)
