# Suggestions (non-blocking)

### S-1. Assert the drained Beacon's `vnanos` equals the stage-C clock read
The asm samples `vns_sample` at the `'C'` stage (the pv-clock VNS read) and stamps it into
the Beacon's `vnanos` field (ringW+8). With `PvClock::new(1, 1)`, vns == icount at that
exit. The drained `GuestEvent.vnanos` is therefore a *known* deterministic value, but the
test asserts nothing about it. A one-liner — `assert_eq!(out.beacons[0].vnanos, <stage-C
icount>)` or at least `assert_ne!(out.beacons[0].vnanos, 0)` — would turn the clock→ring
data-flow into an end-to-end checked path. (If you take I-1 and add `beacons` to the
run-twice tuple, run-to-run identity is covered; this S-1 additionally pins the *value*.)

### S-2. The `boundary_rip: 0` convention silently flattens a real DHILOG field
Every `DevCtx` is built with `boundary_rip = 0`, so every canonical record this run writes
carries rip 0. The test comment explains why (the vCPU is mutably borrowed by the segment
inside `on_exit`, so RIP is not retrievable there). That is a defensible Phase-1 shortcut,
but it means the acceptance test exercises the DHILOG *with the rip field permanently
zero* — a future replayer that keys on `boundary_rip` would get no signal here. Worth a
bead (if not already tracked) to expose the boundary RIP to `on_exit` so M1 can actually
record it; at minimum, restate in the test header that record-level RIP fidelity is NOT
covered by this acceptance run.

### S-3. Temp file leaks on assertion failure
`dh-m1-base-{pid}.img` is removed only at the very end via `remove_file(...).ok()`. Any
panic between creation and that line (a failed assert, or `run_m1` erroring) leaks the
1 MiB file in `temp_dir()`. The blk_fixture sibling test has the same shape, so this is a
repo convention, not new — but a scope guard / `Drop` wrapper around the path would make
the acceptance test self-cleaning even on red. Low priority.

### S-4. Document the cache/ordering reasoning for the entropy DMA (comment-level only)
pv-entropy's `doorbell()` writes guest RAM (`ctx.mem.write`) then the device sets
`STATUS = OK`; the guest's *next* MMIO read of STATUS is a separate VM exit. Because the
guest is NOT executing during `on_exit` (synchronous emulation — KVM has exited to
userspace and the vCPU thread is the one running the handler), there is no torn read and
no host-write/guest-read race on x86: the guest cannot observe the buffer until it
re-enters and re-exits, by which point the host write has long completed in program order
on the same thread. This is correct as-is. A one-line note in the test (or in entropy.rs)
stating "single-threaded synchronous emulation; no host/guest memory race because the vCPU
is not running during the fill" would forestall a future reviewer re-deriving it. No code
change warranted.

### S-5. `record_count >= 4` is a floor, not an exact count — consider pinning it
The asm's detcall sequence produces a *determinable* number of records: INIT_GO IN answer
(1 PIO_ANSWER), the doorbell drain (1 CONS_BUMP + 1 SDK_EVENT for the single Beacon), the
doorbell IN answer (1 PIO_ANSWER), and 1 ENTROPY AUX from the 32-byte fill — i.e. exactly
5 by my reading (the three INIT OUTs log nothing; CHANNEL_INIT itself logs nothing). A
`>= 4` floor would not notice if a record went missing above the floor or if a spurious
extra record appeared. Since the count is deterministic, `assert_eq!(out.log_records, N)`
(derive N and pin it) is strictly stronger and would catch DHILOG-emission regressions.
If the exact count is fragile across detguest-wire encoder changes, keep the floor but
raise it to the true value and add a comment deriving it.

### S-6. Consider asserting pv-blk overlay state and base-vs-overlay divergence
The base-immutability check (blake3 + mtime) is excellent. But the *positive* side — that
the write actually went to the overlay and the read-back differs from the base at sector 0
— is only implied by the `'B'` serial byte (the guest's own wbuf==rbuf compare). If you
have access to the `PvBlk` after the run (it is boxed into the bus, so this needs a
downcast or a bus accessor), asserting `dirty_clusters() == 1` would directly confirm CoW
happened. Optional; the serial `'B'` already proves the guest's read-back matched its
write.
