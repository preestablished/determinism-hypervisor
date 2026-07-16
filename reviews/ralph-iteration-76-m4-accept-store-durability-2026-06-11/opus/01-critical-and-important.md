# Critical and Important Findings

**None.**

No correctness, durability-honesty, or test-mechanics defects rise to Critical or Important severity. The durability assertion is backed by a real `fdatasync`-before-ack path in the store (verified — see below), the restart genuinely discards all in-process state of instance 1, and the byte-identity / ref-identity assertions are the right ground truth for R12.

## Why "durability" here is real, not page-cache theatre (the question this review had to answer)

A restart-in-the-same-process test *would* be satisfiable by mere OS page cache if the server never fsynced — the new instance would `read()` bytes the old instance only `write()`-buffered. I checked the store to rule that out:

- `SnapshotStore::put_snapshot` Step 4 runs `gc.barrier(|| self.pages.sync())` **before** writing/returning the ref — `crates/snapstore-store/src/lib.rs:380-382`.
- `PageStore::sync()` flushes the write buffer, then `file.sync_data()` (fdatasync) on **every dirty pack including the active one**, then `dir_file.sync_all()` if new pack files were created — `crates/snapstore-pagestore/src/ingest.rs:652-680`.
- The manifest itself is written to a staging file, `tmp.sync_all()`, renamed, then the shard directory is `sync_all()`'d — `crates/snapstore-store/src/lib.rs:412-437`.

So by the time `take_snapshot` returns a ref, the referenced pages *and* the manifest are fdatasync'd to disk. `put_pages`/`ingest` alone does **not** fsync (it uses `seal_no_sync`, `pack.rs:248-260`), but the engine always finishes with a `put_snapshot`, whose group-commit barrier covers those pages. The test therefore proves the production durability contract, not an accident of caching.

This is the correct scope for an in-process test. True power-loss / `kill -9`-mid-write semantics (torn writes, fsync-lying filesystems, crash-consistency of the rename) are out of reach in-process and belong to the chaos/fault-injection bead (v1n: store latency/fault injection — the store already has `fail_point!` hooks at `manifest-fsync`, `manifest-rename`, `manifest-dirsync`, `pack-fdatasync`, `sidecar-fsync` ready for exactly that). See suggestion 02-#1 for documenting that boundary.
