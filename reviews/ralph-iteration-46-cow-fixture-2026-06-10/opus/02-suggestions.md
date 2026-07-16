# Suggestions (optional)

## S1 — Add one read request that mixes a CLEAN and a DIRTY cluster within a single request

**File:** `crates/dh-vmm/tests/blk_fixture.rs` (phase 3)

The phase-3 sweep uses `BATCH=64` and reads are 64-aligned; since
`SECTORS_PER_CLUSTER=128`, **no single request ever spans more than one
cluster**. So the device's per-cluster chunk loop (`do_read` lines 160-176)
is never exercised across a clean→dirty boundary *by this test*. The
device unit test `cross_cluster_requests_split_correctly`
(`crates/dh-devices/src/blk.rs:441`) does cross a boundary, but there BOTH
clusters are dirty — the clean+dirty mix in one request is not asserted
anywhere in the suite.

I verified the device handles it correctly (temporary in-tree test: a single
read spanning clean cluster 0 tail → dirty cluster 1 sector → dirty-cluster
RMW-fill sector, all served correctly; reverted). So this is a *coverage* gap,
not a bug. Since the fixture's whole point is end-to-end CoW through the real
file backend, one request that straddles a dirty/clean cluster boundary would
make the test self-contained for that case. Cheap to add, e.g. after phase 2:

```rust
// One request that straddles a clean→dirty cluster boundary (the chunk
// loop must pick base then overlay within ONE request). Sectors
// 96..160 span cluster 0 (clean here) and cluster 1 (dirty at 128,130).
let st = request(&mut dev, &mut mem, CMD_READ, 96, 0, 64);
assert_eq!(st, STATUS_OK, "boundary-crossing read");
for sec in 96..160 {
    let off = ((sec - 96) as usize) * SECTOR_SIZE;
    assert_eq!(mem.0[off..off + SECTOR_SIZE], image::expected_sector_after_writes(sec));
}
```

(96..160: 96..128 in clean cluster 0, 128..160 in dirty cluster 1 — sectors
128 and 130 are overlay, the rest RMW-fill base. A 64-sector read that starts
at 96 crosses the 128 boundary.) Optional; the device path is already proven by
the unit test plus my throwaway check.

## S2 — Note the temp-file naming relies on per-binary process isolation, not on a nonce

**File:** `crates/dh-vmm/tests/blk_fixture.rs:32`,
`tests/nanokernel/src/image.rs:170`

`temp_path` uses only `process::id()` + a tag (no random nonce). This is
collision-free in practice: the three file-creating tests live in three
separate test binaries (separate processes → distinct pids), and within
`blk_fixture` the two file tests use distinct tags ("cow"/"serve") so threads of
one process don't collide. `File::create`/`fs::write` truncate, so even a stale
file from a recycled pid is overwritten correctly. Leak-on-assert-failure (the
`remove_file` runs only after the asserts) is acceptable for tests.

No change required — just calling out that the safety here is "different test
binaries get different pids," which is true but implicit. A one-line comment, or
appending the tag to the nanokernel test path too, would make the invariant
self-evident. Lowest priority.
