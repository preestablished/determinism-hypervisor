# Action Items

### Critical
_None._

### Important

- [ ] **Widen the run-twice comparison beyond the RAM hash (I-1).** Today the repeatability
  assertion compares `(serial, icount, state_hash, log_records)`, and `state_hash` covers
  vCPU + full RAM only — `device_sections` is passed as `&[]` in every `push_final_link`
  call, so pv-clock/pad/entropy/blk internal state and detchannel host state are NOT in the
  hash, and the drained `beacons` Vec contents are not compared at all. After `run_segment`
  returns (while `bus` and `channel` are still in scope), capture
  `dh_vmm::hash::device_sections(&bus)` and `channel.snapshot(&mut v)` into `RunOutcome`,
  add the drained `beacons` to the compared tuple (`GuestEvent: PartialEq`), and assert all
  three are byte/value-identical across the two runs. Converts the M1 milestone claim from
  "true for this RAM layout" to "true for the observable device surface."

- [ ] **Assert the IRQ queue is empty at end of run (I-2).** `irqs` is threaded into every
  `DevCtx` but never drained, applied, or inspected. For this guest it provably stays empty
  (no device queues an IRQ; the guest has no `sti`/IDT so injection would be wrong anyway),
  but a future device or guest that queued one would have it silently dropped with the test
  still green. Add `assert!(irqs.is_empty(), ...)` to lock the invariant.

### Suggestions

- [ ] **S-1**: Assert the drained Beacon's `vnanos` equals the stage-C clock read (1:1 clock
  ⇒ vns == icount at that exit); today it is unasserted.
- [ ] **S-2**: `boundary_rip: 0` flattens the DHILOG rip field for every record this run
  writes. File/track a bead to expose the boundary RIP to `on_exit`, or restate in the test
  header that record-level RIP fidelity is out of M1 scope.
- [ ] **S-3**: Wrap the temp base-image path in a scope guard so it is cleaned up even when
  an assertion panics (currently leaks on red).
- [ ] **S-4**: Add a one-line note (test or entropy.rs) that the entropy DMA fill has no
  host/guest memory race because emulation is synchronous and the vCPU is not running during
  the fill. Comment-only; no code change.
- [ ] **S-5**: Replace `record_count >= 4` with an exact pinned count (derive N — by my read
  it is 5: 2 PIO_ANSWERs + 1 CONS_BUMP + 1 SDK_EVENT + 1 ENTROPY), or raise the floor to the
  true value with a deriving comment.
- [ ] **S-6**: Optionally assert `PvBlk::dirty_clusters() == 1` to directly confirm the write
  hit the CoW overlay (needs a bus accessor/downcast).
