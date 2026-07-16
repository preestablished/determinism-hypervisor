# Positive Notes

## P1 — Loss-free invariant is the right one, and the code enforces it correctly

`harvest_into` (`dirty.rs:84-121`) marks each entry `RESET` *only after* recording its GFN
into the set, and the free-running `next_harvest` cursor never rewinds. Combined with the
kernel's ring-full exit (which I confirmed fires only at a soft-full watermark, with
headroom), KVM can never overwrite an un-RESET entry. This is exactly the invariant the
snapshot engine needs, and the implementation matches the doc claim. Live-verified: the
guest-write test harvests exactly the written pages across two run/harvest cycles.

## P2 — The `slot != 0` guard is a hard error, not a silent skip

`dirty.rs:110-114` turns an unexpected memslot id into a loud `KvmError` rather than
skipping the entry. With a single registered memslot in v1, any other id is a kernel/ABI
contract violation, and failing loud is the correct determinism posture (a skipped dirty page
would silently corrupt a snapshot). Same philosophy on `DirtyPageSet::insert`
(`dirty.rs:140-148`): a GFN past RAM is an error, not a clamp.

## P3 — `harvest_at_boundary` is genuinely a single drain+reset entry point

One function (`dirty.rs:184-199`) is both the pause-boundary drain *and* the
`KVM_EXIT_DIRTY_RING_FULL` service path, so the two call sites can never diverge in behavior.
The `harvested > 0` guard correctly skips the reset ioctl when there is nothing to reap — and
I confirmed by experiment that skipping it is safe (an empty reset returns rc=0 and is a
no-op; a DIRTY-but-unharvested entry is never reaped by reset regardless, so nothing can be
stranded by the skip).

## P4 — `AtomicU32` access is minimal and correctly scoped

Only the `flags` word is touched atomically (acquire-load of DIRTY, release-store of RESET);
`slot`/`offset` are read with plain `ptr::read` *after* the acquire load establishes
happens-before, exactly matching the KVM publication order (KVM writes slot/offset, then
release-stores DIRTY). The comment at `dirty.rs:95-96` documents this ordering dependency
precisely. The cast is alignment-sound and `x86_64`-gated.

## P5 — `DirtyPageSet` iteration is deterministic by construction

`iter()` (`dirty.rs:154-161`) walks words then bits in ascending order, giving a
deterministic ascending page-index sequence — which the manifest entry order depends on.
The unit test `page_set_inserts_iterates_ascending_and_clears` pins this, including dedup
(`set_count` counts only newly-set bits) and the out-of-range loud-error case. The
`div_ceil` math is correct for non-page-multiple RAM (separate unit test covers it).

## P6 — Honest, accurate comments throughout

The module header documents the divergence from the §8.2 RoaringBitmap sketch and *why*
(dense bitmap is smaller for ≤3 GiB v1 guests). The `enable_dirty_logging` comment
(`dirty.rs:151-154`) states plainly "Without the flag the ring stays empty" — which is the
exact behavior my experiment confirmed and which contradicts (correctly) the stale ARCH
wording. The raw-ioctl comment derives `0xAEC7` from the `_IO` encoding rather than just
asserting a magic number. Comments describe what the code *does*, not aspirations.

## P7 — Clean integration into the existing exit-classification surface

`kvm.rs` adds `ExitEvent::DirtyRingFull` and classifies
`VcpuExit::Unsupported(KVM_EXIT_DIRTY_RING_FULL)` into it (`kvm.rs:427-431`), leaving the
existing `Hlt`/`Shutdown`/`Other` arms untouched. The new variant is documented as
host-visible-only (never perturbs guest state), which is the correct determinism framing.
The whole change is additive (+423, 0 deletions) and the full 84-test lib suite stays green.
