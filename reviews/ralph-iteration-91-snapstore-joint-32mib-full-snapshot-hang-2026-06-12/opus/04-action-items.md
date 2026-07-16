# Action Items

## Action Items

### Critical
- [ ] None.

### Important
- [ ] None. The fix is correct, retry-safe, exercises the real gRPC path in
      both tests, and stays within tonic's message-size limits. No blocking
      changes required before merge.

### Suggestions (optional, non-blocking)
- [ ] [dh-worker/tests/snapstore_large_put.rs:35 and ../snapshot-store/crates/snapstore-client/src/tests/test_cases.rs:621] Add a one-line comment that the watchdog worker thread is *intentionally* not joined, so a future reader does not "fix" it by joining and reintroduce the unbounded hang. (S1)
- [ ] [dh-worker/tests/snapstore_large_put.rs:60 and ../snapshot-store/crates/snapstore-client/src/tests/test_cases.rs:642] Optionally note inline why the watchdog is 120 s rather than ~35 s: the retry budget caps at 30 s (`retry.rs:28`), so the long ceiling exists only to catch the *non-retryable* deadlock. (S2)
- [ ] [../snapshot-store/crates/snapstore-client/src/tests/test_cases.rs:618] Optionally factor the sibling test's `8192`/`4096` literals into named `PAGES`/`PAGE` consts to mirror the DH joint test and make the "32 chunks of 256" intent self-evident. (S4)
