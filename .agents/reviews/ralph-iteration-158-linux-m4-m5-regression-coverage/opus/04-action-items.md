# 04-action-items.md

1. Rework `linux_pvblk_io_loopback_records_and_replays` so the pv-blk overlay write/read is the same operation whose DHILOG replay records and final state hash are verified.

2. Add a no-early-consumption assertion for the Linux frame test's current `GuestHalted`/`HardCap` zero-frame path.

3. Replace the Linux frame test's `saturating_sub` delta with a checked cumulative-icount assertion.
