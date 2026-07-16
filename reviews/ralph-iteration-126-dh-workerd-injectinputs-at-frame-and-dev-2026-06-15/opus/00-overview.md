# Review Overview

- Branch: `ralph/iteration-126-dh-workerd-injectinputs-at-frame-and-dev`
- Date: 2026-06-15
- Reviewer: Claude Opus

This branch extends input scheduling so worker `InjectInputs` requests can queue inputs by absolute pv-pad frame counter and can map generic `DeviceEvent` payloads into canonical DHILOG `DEV_EVENT` records. The VMM run loop now detects frame-counter MMIO writes, applies matching frame-scheduled inputs after the frame mark is serviced, and the worker tracks queued inputs by either icount or frame while refreshing the runtime frame counter from device state after each run.

Overall verdict: REQUEST_CHANGES

Stats:
- Files changed: 4
- Lines added/removed: 417 insertions, 35 deletions
- Commits: 1 (`987f1c0 ralph: iteration 126 checkpoint - inject input schema tails`)
