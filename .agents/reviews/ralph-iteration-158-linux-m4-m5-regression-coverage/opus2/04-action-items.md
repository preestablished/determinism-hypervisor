# 04-action-items.md

1. Fix the Linux frame scheduling gate so success requires an actual frame-budget stop and evidence that the scheduled frame was reached.

2. Rework the pv-blk fallback so the overlay IO is part of the worker segment that is sealed, replayed, and final-hash checked.

3. Strengthen `verify_replay_done()` to report/assert `EpochOk` progress against parsed `EPOCH_HASH` records.

4. Decide whether `DH_M9_GUEST=linux` should be enforced by the tests or remain command-line documentation.
