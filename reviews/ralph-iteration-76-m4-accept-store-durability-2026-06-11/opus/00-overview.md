# Review Overview — iteration 76: M4 ACCEPT store durability

- **Branch:** `ralph/iteration-76-m4-accept-store-durability`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Scope:** 2 files, +265/-4, 1 commit. Bead 6hg — "joint integration vs the REAL snapshot-store (R12) — refs only after durability." Adds the missing *durability* leg of M4 ACCEPT.

## What changed

- **`crates/dh-worker/tests/store_durability.rs`** (new, 246 lines): one hardware-gated test, `refs_survive_a_server_restart_and_restore_byte_identically`. Against server **instance 1** over a caller-owned `data_root`, it takes a FULL root + a guest-dirtied incremental DELTA, then a reference restore + re-snapshot (`ref_b1`). It then drops the client, calls `handle1.shutdown()`, drops the tokio runtime, and starts **instance 2** over the SAME `data_root` (different UDS name). It asserts the delta ref still restores; the three delta pages, the root-era page, and the full vCPU state byte-match the live source `slot_a`; the chain value round-trips; and a re-snapshot through instance 2 yields the IDENTICAL 32-byte ref instance 1 issued (`ref_b2 == ref_b1`).
- **`crates/dh-worker/tests/common/mod.rs`** (+24/-4): new `spawn_store_at(data_root, sock_name)` exposing a caller-owned-data-root seam; `spawn_store_blocking` now wraps it and gains `#[allow(dead_code)]` (not every test target uses it).

## Verdict

**APPROVE**

The change is correct, honest, and meaningfully strengthens the acceptance surface. I verified against the snapshot-store source that the durability claim is *real* — `put_snapshot` runs a group-commit `fdatasync` of every dirty pack and an `fsync` of the manifest + shard directory **before** returning the ref (`snapstore-store/src/lib.rs:380-437`, `snapstore-pagestore/src/ingest.rs:652-680`). So the test exercises genuine on-disk durability, not merely OS page-cache survival across an in-process restart. Crucially, because durability is established at *ack* time (not at shutdown), the test does not lean on `shutdown()` being graceful — `handle1.shutdown()` only sends a oneshot signal and does not wait or flush (`build_server.rs:56-61`), and the data would survive even a hard kill. That is exactly the property bead 6hg wants.

The union (this file + `snapshot_engine.rs` + `restore_engine.rs` + `m4_transparency.rs` + `tests/determinism/store_joint.rs`) honestly covers 6hg's stated criteria: page round-trip fidelity, parent-relative deltas, ref-after-ack, never-a-mock, and now durability-of-receipt.

No Critical or Important findings. A few low-severity suggestions and documentation nits below, none blocking.

## Stats

| Item | Count |
|------|-------|
| Files changed | 2 |
| Lines added / removed | +265 / -4 |
| Critical findings | 0 |
| Important findings | 0 |
| Suggestions | 4 |
| Positive notes | 6 |
