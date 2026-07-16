# Critical & Important Findings

## Critical

**None.** The timed windows are correctly scoped, the thresholds match the plan, and
there are no correctness bugs in either the test or the bench.

---

## Important

### I1 — The 8k harvest cost is in the gate's definition but not in the measured window

**Files:** `crates/dh-worker/tests/perf_gates.rs:131-167`,
`crates/dh-worker/benches/perf_gates.rs:103-130`

**What the engine does.** `take_snapshot`'s `Incremental` arm *always* runs
`harvest_at_boundary(ring, &slot.vm, dirty)` as step 1, inside the function and therefore
inside the test's `Instant::now()..elapsed()` window (`crates/dh-worker/src/snapshot_engine.rs:133-134`).
`harvest_at_boundary` drains the ring into the set and, **only if it harvested > 0 entries**,
issues `reset_dirty_rings(vm)` (`crates/dh-vmm/src/dirty.rs:213-218`).

**What the instrument feeds it.** Both the test and the bench build the 8k load by writing
guest RAM from the **host** and inserting page indices directly into the set:

```rust
for page in 0..DIRTY_PAGES {
    dirty.insert(page).unwrap();   // host-side bitset insert
}
// ring is mapped + logging enabled, but no GUEST writes occurred,
// so the ring is EMPTY at harvest time
```

Host `write_slice` does not populate the KVM dirty ring (only guest execution does — see
`snapshot_engine.rs` integration test `incremental_snapshot_ships_exactly_the_dirty_pages_and_clears`,
which deliberately runs a guest program to dirty pages). So in this instrument:

- `ring.harvest_into(set)` returns `0`,
- the `if harvested > 0` guard skips the `reset_dirty_rings` ioctl,
- the timed window measures **read 8k pages + assemble DHSNAP + ship 32 MiB**, but **not**
  the per-entry ring drain of 8192 GFNs nor the reset ioctl.

A real guest-dirtied 8k-page boundary harvests 8192 ring entries (the v1 ring is small, so
this is several `DirtyRingFull` exits, each followed by `harvest_into` + `reset_dirty_rings`),
and that work *is* part of "incremental snapshot ≤ 8k dirty pages" as the IMPLEMENTATION-PLAN
(§M4, line 84) words the gate.

**Why this is Important, not Critical.** The measured failure is storage-bound: 111.6 ms
against a 15 ms gate, dominated by the store's `put` durability receipt (~32 MiB fsync).
Harvesting 8192 ring entries is a memcpy of ~8k u64 GFNs plus a handful of ioctls — sub-millisecond,
swamped by I/O. So the instrument's under-measurement does **not** flip the verdict and does
not affect the 8ot escalation. But once the storage path is fixed and the gate approaches
15 ms, the harvest cost stops being noise, and an instrument that skips it would let a
regression hide. The gap should be recorded now, while the methodology is fresh.

**Fix — pick one, both acceptable:**

1. **Document the scope explicitly** (lowest cost). The existing comment says the set "is
   exactly the engine path the gate times" — that overclaims. Amend it to state that the
   ring is empty by construction, so harvest/reset cost is excluded, and that this is
   acceptable while the gate is storage-bound but must be revisited if the snapshot path
   ever becomes harvest-bound. Cross-reference 8ot.

2. **Dirty via the guest** (higher fidelity, matches the existing integration test). Run a
   guest loop that writes 8192 pages and harvest at `DirtyRingFull` boundaries before the
   timed `take_snapshot`, the way `incremental_snapshot_ships_exactly_the_dirty_pages_and_clears`
   already does at 3 pages. This makes the ring non-empty at the timed harvest and exercises
   the reset ioctl. Trade-off: it complicates the per-sample setup and the guest-write path
   itself adds cost to the *setup* (outside the timer), so option 1 is sufficient for now.

Given the storage-bound reality and the open 8ot decision, **option 1 (a one-comment scope
fix) is enough to ship**; option 2 is the follow-up when 8ot resolves the storage path.
