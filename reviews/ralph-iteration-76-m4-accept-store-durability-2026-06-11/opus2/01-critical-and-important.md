# Critical & Important

## Critical

None.

## Important

### I1. Same-process restart proves "re-open over the same bytes," not fsync-durability — the module doc over-claims

**File:** `crates/dh-worker/tests/store_durability.rs:8-15`

The module doc frames the failure mode as: *"the store acked before persisting
(or persisted somewhere the next instance cannot see) and every ref the engine
ever returned was a lie — exactly the R12 failure mode."* The "acked before
persisting" half is the strong claim, and this test cannot actually catch it.

Walk the persistence model (verified against the real store):

- `put_pages` ingests into an in-memory write buffer / OS page cache; it does
  **not** fsync. Durability for pages comes later, at `put_snapshot` time, via
  the group-commit barrier `gc.barrier(|| self.pages.sync())`
  (`snapstore-store/src/lib.rs:380-382`), which flushes the buffer and
  `fdatasync`s dirty packs before the manifest is fsynced + renamed.
- The pagestore index is **in-memory only**, rebuilt at `PageStore::open` by
  scanning on-disk pack records (`ingest.rs:120-237`, `reopen_pack_for_append`
  → `scan()`).

Now the catch: **instance 2 runs in the same OS process as instance 1** — it is
just a new tokio runtime + a fresh `SnapshotStore` opened over the same
`data_root`. The recovery scan re-reads the pack files, but those reads go
through the **same kernel page cache** that instance 1's `write()` calls
populated. So even if `pages.sync()` and every `sync_all()`/`sync_data()` in the
write path were replaced with a no-op, instance 2 would still see every byte:
the data never had to reach durable storage to be visible across this restart.

Concretely, this test would still pass under a store that acked `put_snapshot`
*before* fsync — exactly the R12 failure mode the doc says it catches. What the
test genuinely proves is: (a) the in-memory index is correctly rebuilt from the
on-disk pack/manifest layout by a fresh `open`, (b) manifest resolution +
parent-chain flattening survive a new `SnapshotStore` instance, and (c) the
content-addressed bytes are byte-identical across instances. That is real,
non-trivial signal — but it is *re-open fidelity*, not *crash durability*.

The genuine fsync-vs-recovery proofs already exist as `failpoints`-gated unit
tests in the pagestore/store crates (`manifest-fsync`, `pack-fdatasync`,
`sidecar-fsync`, `crash_during_rotation_*`, `sync_spans_rotation`,
`put_snapshot_durable_after_reopen`). The acceptance bead's job is the
engine→store contract, and "re-open over the same data_root resolves the chain"
is the honest in-process maximum — `store_joint`'s wire pins plus this re-open
test are the right joint coverage.

**Recommended fix (doc only, no behavior change):** soften the module-doc claim
to match what the rig can prove, and add one line naming where true
fsync-durability is proven. For example, replace the "acked before persisting"
sentence with something like: *"This is an in-process restart, so it proves the
fresh instance reconstructs its index and resolves the full chain from the
on-disk layout — not power-loss fsync durability, which the store's own
`failpoints` unit tests pin. If this fails, a fresh `open` cannot see a ref a
prior instance issued: the layout or recovery is broken."* This keeps the test's
value honest and prevents a future reader from citing it as fsync proof.

This is "Important" rather than "Critical" because the test is *correct* and adds
real signal; only its self-description overstates the guarantee.
