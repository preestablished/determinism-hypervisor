# Review: iteration 76 — M4 ACCEPT store durability (R12 receipt)

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-11
- **Branch:** `ralph/iteration-76-m4-accept-store-durability`
- **Scope:** 2 files, +265/-4
  - `crates/dh-worker/tests/store_durability.rs` (new, 246 lines)
  - `crates/dh-worker/tests/common/mod.rs` (`spawn_store_at` seam)
- **Bead:** determinism-hypervisor-6hg (R12 — "the ref IS the durability receipt")

## Summary

The change adds the one durability leg the existing live-server tests
(`snapshot_engine`, `restore_engine`, `m4_transparency`, `store_joint`) never
covered: a ref handed out by `take_snapshot` must survive the server instance
dying and be served back, byte-identically, by a fresh instance over the same
`data_root`. The test builds a FULL root + a guest-dirtied incremental DELTA
against in-process instance 1, takes a same-instance restore+re-snapshot as a
reference point, tears down instance 1 (client, then server signal, then
runtime), spins instance 2 over the same bytes on a new UDS, and asserts the
delta ref restores to a machine byte-identical to the still-live source slot,
the chain round-trips, and the re-snapshot ref is the same 32 bytes.

I verified the durability assumptions against the snapshot-store source. The
mechanism the test claims to exercise is real: the in-memory pagestore index is
rebuilt from on-disk pack records at `PageStore::open`
(`reopen_pack_for_append` → `scan()`; sealed packs from sidecars or rebuild),
and `put_snapshot` runs a group-commit `pages.sync()` durability barrier
(flush write buffer + `fdatasync` dirty packs) *before* the manifest is
fsynced + renamed (`snapstore-store/src/lib.rs:380-437`). The blocking client
owns its own runtime and `block_on`s each RPC, so by the time `take_snapshot`
returns the delta ref, the server's `put_snapshot` fsync has already completed
— the teardown that follows cannot race it. The engine restore path uses only
`get_snapshot` + `resolve_pages` (manifests + pages), never the meta DB, so the
startup reconcile/PRUNE machinery is out of scope and cannot corrupt the result.

The test is correct, well-targeted, and the seam (`spawn_store_at` with a
per-instance `sock_name`) is a clean, minimal addition. My one substantive
finding is a **scope-honesty gap**: because both instances share the same OS
process and therefore the same kernel page cache, the test cannot distinguish
"fsynced to durable storage" from "written to page cache but never fsynced." It
proves *re-open over the same bytes* (index rebuild, manifest resolution, chain
flattening), which is real and valuable — but it does not prove crash-durability
in the fsync sense the module doc's "acked before persisting" language implies.
That gap is inherent to a same-process restart and is arguably the honest max
for an in-process fixture; it just deserves a one-line comment so a future
reader does not over-claim. The pagestore's own fsync-vs-recovery proofs live in
the `failpoints`-gated unit tests.

## Verdict

**APPROVE.** The test is sound, the assumptions hold against the real store, and
it adds genuine signal no sibling test had. The findings are a documentation
nuance (page-cache vs fsync honesty), one near-tautological assertion worth a
comment, and minor robustness/style notes — none blocking.

## Stats

| Severity   | Count |
|------------|-------|
| Critical   | 0     |
| Important  | 1     |
| Suggestions| 5     |
| Positive   | 6     |
