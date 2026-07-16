# Action Items

Self-contained checklist derived from the findings. Files are relative to the
repo root `/home/infra-admin/git/preestablished/determinism-hypervisor`.

## Must fix before this is treated as ground truth (Important)

- [ ] **Correct the `put_pages` retry-flip comment** in
      `tests/determinism/tests/store_joint.rs:80-85`. The claim that the
      `(pages_new, pages_deduped)` split "flips to all-deduped, observed as (0,3)
      on a FRESH store under parallel test load" did **not** reproduce in
      experiment (246/246 fresh first-puts returned `(3, 0)`), and the client's
      retry policy only fires on `Unavailable | DeadlineExceeded | Transport(_)`
      — none of which occur against a healthy in-process UDS server. The sibling
      test `put_pages_retries_on_unavailable` (snapshot-store
      `crates/snapstore-client/src/tests/test_cases.rs:558`) even asserts the
      opposite shape `(new=1, deduped=0)` on retry. Do ONE of:
  - [ ] Replace the comment with the accurate version (drop the unverified
        mechanism; state that the sum is the conservative invariant and that
        `(3,0)` is in fact what a healthy fresh store returns). A drop-in
        replacement is in `01-critical-and-important.md` finding I-1.
  - [ ] OR re-capture the original `(0,3)` with a deterministic, committed repro
        and cite the exact failure mode in the comment. If you cannot reproduce
        it, prefer the first option.
  - [ ] OPTIONAL: tighten the assertion to `assert_eq!((new, deduped), (3, 0))`
        — verified stable — so a future fresh-store-dedups regression is caught.

## Should do (Suggestions — non-blocking)

- [ ] **S-1:** Add a note to `docs/decisions/snapstore-server-for-tests.md` that
      `serve_for_tests` is async and requires a tokio runtime, so the synchronous
      KVM/qmp callers must host the server on a dedicated runtime/thread and reach
      it via `blocking::SnapstoreClient::connect(Transport::Uds(path))`. Reference
      the sibling pattern in `blocking_facade_smoke` (test_cases.rs:579).
- [ ] **S-2:** Add a hazard line (decision doc + `store_joint.rs` helper doc
      comment): the returned `ServerHandle` and `TempDir` must outlive every
      client call; dropping the handle initiates server shutdown and will break
      an in-flight/subsequent (especially blocking) call.
- [ ] **S-3:** File a follow-up bead under qmp for a `spawn_store_blocking`
      helper variant (server on a held multi-thread runtime + `blocking`
      facade client) so the engine tests exercise the production transport facade
      rather than the async client.
- [ ] **S-4:** (folds into the must-fix above) prefer the strict
      `(3, 0)` assertion if the team is comfortable, matching the strict `(0, 2)`
      assertion already in `re_put_is_deduped_and_ref_stable`.
- [ ] **S-5:** Consider replacing the fixed 20 ms settle sleep in
      `store_joint.rs:32-33` with a readiness probe (health-reporter poll or a
      bounded connect-retry) to cut wall-clock and harden against future
      heavy-CI flakes. Low priority — 0 flakes observed today.

## Verified — no action needed

- [x] Decision is correct; build-and-spawn-in-process over provisioned service.
- [x] Cargo target-gating effective: aarch64 graph excludes snapstore-server
      (cargo metadata --filter-platform), aarch64 cross-clippy clean in 2.82s,
      `determinism-tests` is the sole x86-only consumer.
- [x] `cargo test -p determinism-tests --test store_joint` passes (run 2x).
- [x] Hermeticity holds: 5x `--test-threads=8` stress, 0 flakes.
- [x] x86_64 and aarch64 clippy clean; tree clean after experiments.
- [x] All doc citations (`serve_for_tests`, `serve_for_tests_with_metrics`,
      `page_channel_fallback.rs`) exist and match.
