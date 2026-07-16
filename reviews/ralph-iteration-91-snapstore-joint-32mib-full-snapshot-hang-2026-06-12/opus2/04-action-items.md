## Action Items

### Critical
- [ ] None.

### Important
- [ ] [crates/dh-worker/tests/snapstore_large_put.rs:63] Add
  `assert_eq!(manifest.guest_ram_bytes, PAGES * PAGE as u64);` after the existing
  entry-count assert, for parity with the client-side sibling test and to pin the
  FULL-manifest contract on the test designated to guard the 128 MiB (9sb) path.
  (Defensible to downgrade to a Suggestion — no correctness bug today.)

### Suggestions
- [ ] [../snapshot-store/crates/snapstore-client/src/client.rs:136-151] Drop the
  per-page `Bytes` round-trip (`Bytes::from(data.clone())` then `.to_vec()`) —
  push `data.clone()` straight into the `PutPagesRequest` and `mem::take` the
  chunk. Halves transient allocation on the 128 MiB path; remove the now-unused
  `bytes::Bytes` use in that function. (S1 — perf, defer to 9sb pass.)
- [ ] [../snapshot-store/crates/snapstore-client/src/client.rs:155-161] Extend the
  comment to note the whole `messages` Vec (~guest_ram_bytes) is resident for the
  RPC's duration, so a future reader doesn't reintroduce a channel. (S2)
- [ ] [../snapshot-store/crates/snapstore-client/src/client.rs:127-163] Add one
  sentence to the `put_pages` doc noting message construction is intentionally
  inside the retry closure (per-attempt rebuild is required because a
  `tokio_stream::iter` is single-use). (S3)
- [ ] [crates/dh-worker/tests/ring_chaos.rs:11-13] Reflow the "Ring-full exits are
  host-visible only and / harvest-on-full is / loss-free" mid-sentence wrap into
  normal prose while the adjacent lines are already being touched. Cosmetic. (S4)
- [ ] [crates/dh-worker/tests/snapstore_large_put.rs:51-65] (and client test)
  Optionally add a one-line comment that detaching the put thread on watchdog
  timeout is deliberate, so it isn't "fixed" into a join that re-hangs. No code
  change. (S5)
