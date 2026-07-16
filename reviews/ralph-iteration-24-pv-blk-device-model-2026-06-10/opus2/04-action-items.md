# Action items — pv-blk (second reviewer)

Self-contained checklist. File paths are repo-relative to `/home/infra-admin/git/preestablished/determinism-hypervisor`.

### Critical

None.

### Important

- [ ] **Decide and enforce the `host_io_errors` snapshot contract (I-2).** `crates/dh-devices/src/blk.rs` `snapshot` (~L249) omits `host_io_errors`, so `restore` of a snapshot taken at `status == STATUS_HOST_IO (0xFE)` yields the impossible live-pair `(status = 0xFE, host_io_errors = 0)`. The `STATUS_HOST_IO` doc (L55-61) promises run control can trust the counter. Pick one and document it:
  - **(preferred)** Serialize `host_io_errors` as a u64 in the section; bump `SECTION_VERSION` to 2 and restore it. Add a round-trip test. This makes the counter a true function of device state, matching the `DetDevice` trait's "Must be a pure function of device state."
  - or reject `status == STATUS_HOST_IO` in `restore` (return `RestoreError`) with a comment that a host-IO state is never snapshottable.
  Do this **before** any run-control code starts reading `PvBlk::host_io_errors` (none does today — confirmed only `blkfile.rs`/`blk.rs` reference it).

- [ ] **Document the partial-mutation guest ABI (I-1).** In `crates/dh-devices/src/blk.rs`, add to the §6.5 STATUS doc block (and a comment at the `do_read` `ctx.mem.write(...).is_err()` site ~L168) that a non-OK completion (`MEM_FAULT`/`HOST_IO`) may leave guest RAM and/or the overlay **partially** mutated, and that this partial state is deterministic device state — not a bug to "fix" by buffering/atomicizing the transfer (which could introduce a replay hazard). Mirror the wording of the existing `do_write` partial-fault comment (~L202-204). Update ARCHITECTURE.md §6.5 to match. No code change.

### Suggestions

- [ ] **S-1:** In `request_range` (`blk.rs` ~L135-150), make the multiply overflow-safety local: either a comment ("`sector*512 < len_bytes` by the range check; `count` u32 ⇒ `count*512` fits 64-bit usize") or switch to `checked_mul` returning `STATUS_BAD_REQUEST` on `None`. Removes reliance on a sane `BlockBase::len_bytes`.
- [ ] **S-2:** In `restore` (`blk.rs` ~L275), replace `SECTION_FIXED + n * SECTION_PER_CLUSTER` with `n.checked_mul(SECTION_PER_CLUSTER).and_then(|x| x.checked_add(SECTION_FIXED))` → `RestoreError` on `None`. 64-bit-safe today, but defends 32-bit/future-growth and documents intent. (Ordering already correct: length check gates `with_capacity` — keep it.)
- [ ] **S-3:** Document (ARCHITECTURE.md §6.5 + a `mmio_write` comment in `blk.rs` ~L235) that registers accept *only* their natural width at their natural offset; an 8-byte write spanning COUNT+CMD (0x18,len=8) and sub-register accesses (e.g. 4B read at 0x08/0x0C) are no-ops/zeros by design — so a future driver author does not chase a silently-dropped CMD.
- [ ] **S-4:** Add a comment at `FileBase::read_at`'s `map_err(|_| BaseIoError)` (`blkfile.rs` ~L703) noting it folds genuine read errors and base-truncated-after-open (`UnexpectedEof`) into one `STATUS_HOST_IO`, both legitimately host faults. (EINTR is already handled by `read_exact_at`'s internal retry — no change needed there.)
- [ ] **S-5:** Confirm `FileBase::len`/`is_empty` (`blkfile.rs` ~L678-685) have a production consumer; if test-only, gate or document them as observability accessors to keep the production backend's public surface minimal.
- [ ] **S-6:** Add three tests to `blk.rs`: (a) `restore(snapshot(d))` then `snapshot` is byte-identical to the original snapshot; (b) a write whose `end_sector == capacity` exactly (last valid sector) returns `STATUS_OK`; (c) a ≥2-cluster read where every cluster is fully overlaid (no base fallthrough).
