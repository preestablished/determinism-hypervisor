# Suggestions (non-blocking)

## S1 — Pages are uploaded twice (redundant `put_pages`)

`crates/dh-worker/src/snapshot_engine.rs:223-225` calls
`store.put_pages(pages.clone())` (step 3), then
`crates/dh-worker/src/snapshot_engine.rs:232-244` calls
`store.put_snapshot_from_parts(...)` (step 5). But
`put_snapshot_from_parts` **internally calls `put_pages` again**
(`../snapshot-store/crates/snapstore-client/src/client.rs:757` →
`self.put_pages(pages.clone()).await?`). So every page is uploaded twice
per snapshot. For a FULL snapshot that is a full redundant upload of guest
RAM.

It is *correct* (the store dedups by content hash; the ref is byte-identical
either way — verified in experiment 1), so this is efficiency, not a bug.
But it doubles bandwidth and the client-side `batch_blake3` cross-check
cost on the hot path, and there is also a third `pages.clone()` between the
two calls.

**Options:**
- Drop the explicit step-3 `put_pages` entirely and rely on
  `put_snapshot_from_parts` to upload — simplest, removes one clone too.
  The step-3 comment ("server hashes + dedups; client cross-checks
  batch_blake3") is satisfied equally well by the internal call.
- If the explicit pre-upload is intentional (e.g. to surface a
  page-upload error *before* spending effort assembling the DHSNAP), keep
  it but say so in the comment, and consider asking the sibling for a
  `put_snapshot_from_parts` variant that assumes pages are already
  resident (skip its internal `put_pages`).

```rust
// Option A — let put_snapshot_from_parts own the upload:
// (delete the step-3 block at lines 221-225)
// ...
let snapshot_ref = store
    .put_snapshot_from_parts(parent.as_ref(), slot.mem_bytes, pages, DeviceBlob { .. })
    .map_err(|e| EngineError::Store(format!("put_snapshot: {e}")))?;
```

## S2 — Document the empty-delta contract (experiment 2)

An `Incremental` snapshot with a cleared/never-dirtied dirty set ships
**0 pages** and `Manifest::new_delta` accepts an empty entry list, yielding
a valid DELTA (`parent=Some`, `entries=0`). I confirmed this end-to-end.
This is a sensible "no-op delta / pure device-state checkpoint" and needs
no guard — but it is currently undocumented, so a caller can't tell whether
it's intended behaviour or an accident waiting to be tightened.

Add a line to the `PageSource::Incremental` doc (or `take_snapshot`'s) such
as: "An empty dirty set produces a valid zero-page DELTA (device state
only); the engine does not treat this as an error." If a zero-page delta is
ever *undesirable* (e.g. a caller bug indicator), guard it explicitly
instead — but document whichever way.

## S3 — `agenda_empty: bool` vs sourcing from `SegmentOutcome` (experiment 5)

`BoundaryState` is hand-built by the caller and carries `agenda_empty`,
`icount`, `vns`, `epoch_index`, `hash_chain`. After `run_segment` returns,
`crates/dh-vmm/src/runctl.rs:59` `SegmentOutcome` already carries
`boundary`, `vns`, and `state_hash` — i.e. three of the five fields the
engine needs — and the agenda is by-construction fully walked (it is a
local `Vec<StopPoint>` consumed inside the loop; there is no persistent
`Agenda` object to query). So:

- For `agenda_empty` *specifically*, the bool is the **honest seam** today —
  there is no harder handle to take until ol1 (the slot table) owns a
  durable run-control object. No change needed now.
- But `icount/vns/hash_chain` *could* be sourced from `SegmentOutcome`
  rather than re-supplied by the caller, removing a class of "caller passes
  a stale/mismatched boundary" mistakes. Consider, when ol1 lands, an
  `impl From<&SegmentOutcome> for BoundaryState`-style constructor (with
  `agenda_empty` still asserted by the caller until a richer handle exists).

Position, not a defect: flag the gap so it isn't forgotten at the ol1 seam.

## S4 — `mem_bytes` page-alignment is assumed, not checked (experiment 4 follow-on)

`take_snapshot` computes `total_pages = slot.mem_bytes / PAGE_SIZE`
(`snapshot_engine.rs:191`). `create_slot_vm`
(`crates/dh-vmm/src/kvm.rs:121`) does **not** validate that `mem_bytes` is
a multiple of 4096. If a non-aligned slot were ever created, the FULL walk
would silently drop the trailing partial page; and passing
`slot.mem_bytes` as `guest_ram_bytes` for the FULL case would then trip
`Manifest::new_full`'s `GuestRamNotAligned` / `FullCountMismatch` at the
store — so it fails *loudly at the store*, not silently. Confirmed: passing
`slot.mem_bytes` for the DELTA path is also correct (it's used only as the
`max_idx` index-range bound, not a count requirement).

Low likelihood (all current callers use page-multiple sizes), but a
one-line `debug_assert!(slot.mem_bytes.is_multiple_of(PAGE_SIZE))` at the
top of `take_snapshot`, or alignment validation in `create_slot_vm`, would
turn a confusing far-away store error into a local one. Optional.
