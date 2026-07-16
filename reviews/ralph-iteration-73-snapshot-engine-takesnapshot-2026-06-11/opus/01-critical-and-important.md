# Critical & Important Findings

## Critical

None. The engine is correct on every fidelity axis I checked:

- **Drain at pause:** `harvest_at_boundary(ring, &slot.vm, dirty)` runs before reading the
  page set (incremental path). ✓
- **Hashing:** §8.2's "hash every dirty page (blake3, rayon-parallel)" is **delegated**, and
  the delegation is correct. The engine ships bare bytes; `put_snapshot_from_parts` →
  `build_snapshot_container` (`snapstore-client/src/helpers.rs:34-40`) computes
  `blake3::hash(page)` for each `ManifestEntry`, and `put_pages` cross-checks `batch_blake3`
  server-side. For v1 this is the right boundary — the hashes that "go into the manifest
  entry table" are produced by exactly the code §8.2 describes, just in the client crate.
  The "rayon-parallel" detail is a store-side optimization concern, not an engine
  obligation. ✓
- **Ship bare bytes:** `store.put_pages(pages)` sends `(idx, Vec<u8>)`; no indices baked into
  page payloads. ✓
- **Order: PutSnapshot → ref → clear:** dirty clear (`snapshot_engine.rs:247-249`) is strictly
  after the `put_snapshot_from_parts` `?` (line 232-244). ✓ No ordering violation.
- **DELTA `guest_ram_bytes`:** passing `slot.mem_bytes` for both FULL and DELTA is correct.
  `Manifest::new_full` validates `entry_count == guest_ram_bytes/4096`; `new_delta`
  (`snapstore-manifest/src/lib.rs:178-199`) validates only `entry_index < guest_ram_bytes/4096`
  AND `resolve_chain` requires all manifests in a chain share the same `guest_ram_bytes` — so
  the engine MUST pass the full RAM size (not the delta size) on the DELTA path, which it does.
  ✓ A subtle correctness point that the code gets right.

## Important

### I1 — Every page is uploaded to the store twice (redundant `put_pages`)

**Severity:** Important (efficiency / resource, not correctness)
**File:** `crates/dh-worker/src/snapshot_engine.rs:223-244`

The engine explicitly calls `put_pages` in step 3:

```rust
store
    .put_pages(pages.clone())
    .map_err(|e| EngineError::Store(format!("put_pages: {e}")))?;
```

then in step 5 calls `put_snapshot_from_parts(parent, slot.mem_bytes, pages, ...)`. But
`put_snapshot_from_parts` (`snapstore-client/src/client.rs:756-757`) **already uploads the
pages itself** as its first action:

```rust
// Upload pages.
self.put_pages(pages.clone()).await?;
```

So the full page set is shipped over the page-channel/gRPC **twice**, plus an extra full
`pages.clone()` allocation of the whole RAM image on the FULL path (512 pages here, but the
production target is the entire guest RAM — megabytes to gigabytes). The second upload is a
no-op on the wire only insofar as the server dedups, but the bytes still travel and the
batch_blake3 is recomputed. This is pure waste in the platform's central operation.

The separate `put_pages` call buys nothing the engine needs: `pages_shipped` is
`pages.len()` (line 219), computable without the upload. The whole of step 3 should be
deleted and the comment about the cross-check moved to the `put_snapshot_from_parts` call,
since that path does the cross-check too.

**Fix:**

```rust
    let pages_shipped = pages.len() as u64;

    // ── 3. Assemble DHSNAP ────────────────────────────────────────────────
    let dhsnap = build_dhsnap(slot, bus, entropy, machine_config, &boundary)?;

    // ── 4. put_snapshot_from_parts ships the pages (server hashes + dedups,
    //       client cross-checks batch_blake3) AND builds the manifest; the
    //       returned ref is the durability receipt (R12). ────────────────────
    let snapshot_ref = store
        .put_snapshot_from_parts(
            parent.as_ref(),
            slot.mem_bytes,
            pages,            // moved, no clone
            DeviceBlob { /* ... */ },
        )
        .map_err(|e| EngineError::Store(format!("put_snapshot: {e}")))?;
```

This also drops the `pages.clone()` (step 3) and the `pages.clone()` inside
`put_snapshot_from_parts`'s own `put_pages` is the only remaining clone — itself a candidate
for a future `&[(u64, Vec<u8>)]` API, but out of scope here.

**Caveat worth a sentence in the fix commit:** removing the standalone `put_pages` means the
engine no longer pre-verifies that pages land before it spends time building the DHSNAP. That
ordering (pages-then-build) is harmless for retry-safety (build failures still leave the
dirty set intact), so collapsing the two uploads loses nothing — the orphaned-pages-on-build-
failure case is unchanged because `put_snapshot_from_parts` uploads pages *before* it builds
and puts the container anyway.
