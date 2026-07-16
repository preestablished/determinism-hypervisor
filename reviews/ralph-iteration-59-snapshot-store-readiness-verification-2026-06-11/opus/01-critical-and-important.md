# Critical and Important Issues

## Critical

**None.** No security, data-loss, crash, or broken-functionality issues. The self-hosted
runner exposure is already correctly gated (see below), the dep is test-only, and the gate
compiles and passes.

## Important

**None blocking.** The one item below is a borderline Important/Suggestion correctness nit
in a comment — it cannot break the build and does not affect runtime, so it is recorded
here for visibility but does not gate merge.

### IMP-1 (borderline) — Test doc-comment misdescribes how the page channel is selected

- **Severity:** Important (documentation correctness; non-blocking)
- **File:** `crates/dh-snapshot/tests/snapstore_readiness.rs:9-12`
- **Description:** The module doc says:

  > The page channel (API.md §1.2 `hashes_only` / localpath fast path) is internal to the
  > client — it is selected via `Transport::Auto`'s `page_channel_path` field, which the
  > Transport pin below covers.

  This claims `page_channel_path` *selects* the page channel. The actual sibling source
  (`../snapshot-store/crates/snapstore-client/src/transport.rs`) documents that field as
  **reserved for the M5 page-channel arm (WI3) and currently unused** — `Transport::connect`
  destructures it as `page_channel_path: _`. So today the field selects nothing; the page
  channel is not yet wired. The pin itself is fine and valuable (it locks the field's
  existence and type so WI3 has a stable seam), but the prose overstates current behavior
  and will read as wrong to anyone who opens the sibling crate. Research note (Rust
  integration testing): pins should track the documented *contract*; here the contract is
  "field reserved," not "field selects the channel."
- **Suggested fix:** Soften the comment to match the sibling's own wording, e.g.:

  ```rust
  //! The page channel (API.md §1.2 fast path) is internal to the client and not yet
  //! wired: `Transport::Auto`'s `page_channel_path` field is reserved for the M5/WI3
  //! page-channel arm and currently unused. The Transport pin below locks that field's
  //! existence and type so the WI3 seam stays stable.
  ```
- **Why non-blocking:** It is a comment only; the code pin is correct and the gate's
  purpose (compile-time surface lock) is unaffected.

## Notes on areas explicitly checked and found correct

- **Self-hosted runner / fork-PR exposure (research: GHA self-hosted runner security).**
  The new checkout steps add no new trigger surface. `ci.yaml`'s `kvm-intel` job retains
  its guard `if: github.event_name == 'push' || github.event.pull_request.head.repo.full_name == github.repository`,
  so fork PRs still never reach the box. `nightly-drift.yaml` is `schedule` +
  `workflow_dispatch` only (no `pull_request` trigger), so its self-hosted jobs are
  unreachable from forks. The added `actions/checkout@v4` is first-party at a major tag,
  consistent with the repo's existing pinning policy.
- **Dev-dependency placement (research: Rust integration testing).** `snapstore-client` is
  correctly in `[dev-dependencies]` of `dh-snapshot`, so it never enters the production
  dependency graph. The workspace-level entry is inherited via `snapstore-client.workspace = true`,
  matching the `determinism-proto`/`detguest-host` convention and avoiding version drift.
- **Atomic dep + CI checkout.** All three workspace-building lanes (host x2 via matrix,
  kvm-intel, determinism-canary) gained the `../snapshot-store` checkout in the same commit
  as the dep. This is required because cargo resolves path deps at `cargo-metadata` time;
  the change correctly does not split them.
- **Lock-file integrity (research: cargo workspace path deps).** The `Cargo.lock` diff is
  the expected transitive closure of the new gRPC client (tonic/prost/axum/hyper/tokio/zstd)
  plus build tooling; removals are version re-resolution of the shared closure. Nothing
  unrelated churns.
