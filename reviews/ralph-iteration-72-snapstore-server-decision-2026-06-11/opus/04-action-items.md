# Action Items

Verdict: **APPROVE**. No blocking items. Everything below is optional polish
or a hand-off note for the qmp engine follow-on (bead qmp). Nothing here
needs to land before merge.

## Checklist

- [ ] **(qmp hand-off, doc)** Add a Consequences note to
  `docs/decisions/snapstore-server-for-tests.md` that qmp's engine — a
  blocking/synchronous KVM vCPU loop — should consume the store via
  `snapstore_client::blocking::SnapstoreClient` (the sibling's
  `blocking.rs:1-8` documents it as "for KVM vCPU worker loops that are not
  tokio-native"), connecting a blocking client to the same UDS path
  `spawn_store` serves. `spawn_store` returns the *async* client for the
  pure-store joint tests. (S1)

- [ ] **(qmp hand-off, doc)** Address the tokio-vs-KVM process-sharing
  question the bead raised: qmp's engine test will run the in-process
  `serve_for_tests` tokio runtime *and* KVM ioctls in the same process. There
  is no inherent conflict — KVM ioctls run on a dedicated blocking thread and
  the blocking client owns a `current_thread` runtime separate from the
  server's `rt-multi-thread` — but the doc should state this explicitly so
  the qmp author doesn't worry about runtime contention. (S1, context)

- [ ] **(optional, ergonomics)** In `store_joint.rs`, either return
  `uds_path` as a 4th tuple element from `spawn_store`, or add a one-line
  comment noting the live socket is `dir.path().join("snapstore.sock")`, so a
  future caller (qmp) can attach a second/blocking client without re-deriving
  it. (S2)

- [ ] **(optional, clarity)** Add a one-line comment in `spawn_store`
  documenting that `ServerHandle` is intentionally held as `_handle` with no
  graceful per-test shutdown — server tasks are reaped at process exit — so a
  future reader doesn't "fix" it into a use-after-shutdown. Revisit
  `handle.shutdown()` only if a suite ever spawns hundreds of stores per
  process. (S3)

- [ ] **(optional, docs)** Clarify in
  `docs/decisions/snapstore-server-for-tests.md` (line ~52) that the deps are
  gated to x86_64 to match the determinism suite's lane, *not* because the
  store is arch-specific (it is arch-independent). The gate lives on the test
  file (`#![cfg(target_arch = "x86_64")]`), not the deps. (S5)

- [ ] **(watch-list, no action now)** If the joint suite ever flakes on a
  loaded kvm-intel box, replace the fixed 20ms settle sleep with a
  connect-retry loop rather than a larger sleep — but keep parity with the
  sibling's constant until then. (S4)

## Verification performed by this review

- [x] Ran `cargo test -p determinism-tests --test store_joint` twice
  (default parallel) — 3 passed / 0 failed each.
- [x] Ran once with `--test-threads=1` — 3 passed / 0 failed (rules out the
  parallel-interference alternative for the retry discovery).
- [x] Verified `put_pages` retries the whole idempotent upload
  (`client.rs:128`, `retry.rs:37-48`) — the retry-semantics discovery is
  correct.
- [x] Confirmed `new == 0` and `deduped == 2` re-put assertions are
  retry-proof (see `01-critical-and-important.md`).
- [x] Confirmed `serve_for_tests` signature and the 20ms-settle parity with
  the sibling's `page_channel_fallback.rs`.
- [x] Confirmed `blocking::SnapstoreClient` exists and is documented for KVM
  vCPU loops.
