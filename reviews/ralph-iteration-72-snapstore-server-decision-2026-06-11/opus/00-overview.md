# Review Overview — snapstore-server for tests (R12 / bead wbq)

- **Branch:** `ralph/iteration-72-snapstore-server-decision` vs `main`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Verdict:** **APPROVE**

## Summary

This iteration records the R12 decision that joint tests run against the
REAL snapshot-store — never a mock — and implements it by spawning the
sibling `snapstore-server` *in-process* via its `serve_for_tests(config) →
(ServerHandle, PathBuf)` seam (the same seam snapstore-client's own
integration tests use). The choice is sound: the decision doc correctly
collapses the "build-and-spawn is slower" framing (there is no second
binary and no rebuild — the store is a normal workspace dev-dependency
started on a `TempDir` in milliseconds), and its rejection of the
provisioned long-running service is well-argued on the two axes that
actually matter for a determinism suite — hermeticity (fresh `TempDir`
per test, no cross-run state) and drift (server and client come from one
sibling HEAD by construction). The `spawn_store` helper is a clean,
reusable seam returning `(ServerHandle, SnapstoreClient, TempDir)` with
explicit lifetime ownership, and the three ground-truth tests
(byte-identity roundtrip, dedup + ref stability, hermetic isolation) are
each load-bearing rather than ceremonial. The standout is the encoded
discovery that `put_pages`' `(pages_new, pages_deduped)` split is
*unreliable* because the client transparently retries content-idempotent
uploads — only the sum is invariant; I verified this against the sibling's
`with_retry`/`is_retryable` source and it is correct. Tests pass cleanly
3/3 across two multi-threaded runs and one single-threaded run. The only
gaps are documentation-level: the doc omits the `blocking::SnapstoreClient`
path that qmp's KVM engine will actually need (the sibling literally
documents that facade as "for KVM vCPU worker loops"), and it doesn't
address the tokio-runtime-vs-KVM-ioctl process-sharing question the bead
raised. Neither blocks the decision; both are noted for the qmp follow-on.

## Stats

| Metric | Value |
|---|---|
| Commits reviewed | 1 (`82c2c21`) |
| Non-lockfile files changed | 5 |
| New tests | 3 (`store_joint.rs`) |
| Test runs performed | 3 (2× default parallel, 1× `--test-threads=1`) |
| Test result | 3 passed / 0 failed (all runs) |
| Critical findings | 0 |
| Important findings | 0 |
| Suggestions | 5 |
| Cargo.lock churn | dev-dep closure for `snapstore-server` (expected, not reviewed line-by-line) |
