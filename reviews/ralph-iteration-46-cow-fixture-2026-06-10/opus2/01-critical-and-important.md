# Critical & Important Findings

## Critical

**None.**

## Important

**None.**

I specifically tried to break the consumption test's assumptions and the PvBlk
boundary semantics it leans on, and found no correctness defect:

- The `(2040, 8)` tail write ends at `end_sector = 2048`, exactly equal to
  `capacity_sectors() = 2048`. The validation at `crates/dh-devices/src/blk.rs:145`
  rejects only `end_sector > capacity`, so this is STATUS_OK — verified intentional
  by reading the code, not just inferred from the green test. A write one sector
  longer (`2041, 8`) would correctly be STATUS_BAD_REQUEST.

- Cluster 15 (bytes 983040..1048576) is fully within the 1 MiB image, so the RMW
  `read_at(15 * 65536, 65536)` in `do_write` (`blk.rs:191-200`) does NOT exercise
  the zero-fill-past-EOF path. The image is cluster-aligned, so no tail-fill ever
  happens with this fixture. (The zero-fill path is separately covered by
  `blkfile.rs::reads_serve_file_content_and_zero_fill_past_eof` over a 2.5-sector
  image — good.)

- A full-cluster-covering write still RMWs the whole cluster from base first, then
  overwrites — wasteful but correct, and irrelevant for this fixture since no single
  write covers a full 128-sector cluster.

- `PvBlk` raises no IRQs on completion (synchronous completion inside the CMD-write
  emulation; confirmed by `grep` finding zero IRQ references in `blk.rs`). The
  fixture's per-request throwaway `irqs` Vec therefore discards nothing — no silent
  loss of completion signals.

- Buffer margins: 64-sector batch = 32 KiB into the 64 KiB `VecGuestMem`; max write
  4 KiB. `BASE_IMAGE_SECTORS (2048) % BATCH (64) == 0`, so the final batch is full
  and the loop never reads past capacity. (This last fact is load-bearing and
  implicit — see suggestion 02.S2.)
