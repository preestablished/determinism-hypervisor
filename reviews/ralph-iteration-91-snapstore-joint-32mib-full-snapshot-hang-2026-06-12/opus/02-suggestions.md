# Suggestions (non-blocking)

### S1 — Detached watchdog threads leak on timeout (both tests)

- **Files:**
  `dh-worker/tests/snapstore_large_put.rs:35-62`,
  `../snapshot-store/crates/snapstore-client/src/tests/test_cases.rs:621-645`
- **What:** Both tests `std::thread::spawn` the worker and never join it; on
  timeout the test panics (via `.expect(...)` on `recv_timeout`) while the
  worker thread is still parked in the hung put. That detached thread keeps the
  store runtime / temp dir alive until the process exits. This is *intentional
  and correct* for a hang test — you cannot join a thread that is hung, so the
  watchdog-via-channel pattern is the right call (the research file
  `tokio-channel-streaming-deadlocks.md` explicitly endorses "worker thread +
  `recv_timeout`"). The only suggestion is a one-line comment noting the leak is
  deliberate, so a future reader does not "fix" it by joining and reintroducing
  the unbounded hang.
- **Snippet:**
  ```rust
  // NOTE: the worker thread is intentionally NOT joined. On a regression it
  // stays parked in the hung put; joining it here would re-hang the suite,
  // which is exactly what the recv_timeout watchdog exists to prevent.
  ```

### S2 — 120 s watchdog margin vs the 30 s retry budget (consider documenting)

- **Files:** `dh-worker/tests/snapstore_large_put.rs:60`,
  `../snapshot-store/crates/snapstore-client/src/tests/test_cases.rs:642`
- **What:** The happy path completes in well under a second; the genuine hang is
  unbounded (the channel send never wakes). 120 s is a generous margin that will
  not flake on slow CI — good. Note for context: `with_retry`'s own budget caps
  at `MAX_ELAPSED = 30 s` (`retry.rs:28`), so any *retryable* failure resolves
  or gives up inside 30 s regardless; the 120 s ceiling exists purely to catch
  the non-retryable deadlock. A short inline note to that effect would explain
  why 120 s and not, say, 35 s.

### S3 — Duplicated page-generation helper across the two new tests

- **Files:** `dh-worker/tests/snapstore_large_put.rs:39-46`,
  `../snapshot-store/crates/snapstore-client/src/tests/test_cases.rs:625-632`
- **What:** Both tests build `Vec<(u64, Vec<u8>)>` of distinct pages with the
  identical `data[..8].copy_from_slice(&i.to_le_bytes())` idiom. They live in
  different crates/repos so sharing is impractical and copy-paste is acceptable
  here (the research file `rust-integration-testing.md` flags dedup *within* a
  crate, not across repos). No action needed; noted only for completeness.

### S4 — `PAGES`/`PAGE` consts vs inline literals (minor consistency)

- **File:** `../snapshot-store/crates/snapstore-client/src/tests/test_cases.rs:618-637`
- **What:** The DH joint test factors out `const PAGES`/`const PAGE`
  (`snapstore_large_put.rs:27-28`), which reads cleanly. The sibling test uses
  an inline `n_pages = 8192u64` and bare `4096` literals. Aligning the sibling
  test to named constants would make the "32 chunks of 256" intent self-evident
  and the two tests visually parallel. Purely cosmetic.
