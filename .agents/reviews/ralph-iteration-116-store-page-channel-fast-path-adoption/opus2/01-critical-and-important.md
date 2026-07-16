# Critical And Important Findings

No blocking findings.

Important checks performed:

- Fallback risk: `Transport::Auto` can fall back to gRPC when the page-channel path is absent or the page-channel connect fails. The new Linux-only regression at `crates/dh-worker/tests/snapstore_large_put.rs:74` uses the same worker fixture shape with `corrupt_cross_check_for_test` enabled and expects `ClientError::BatchBlake3Mismatch`. That assertion would not pass through gRPC fallback, so it is a valid live-path guard for `put_pages`.
- Fixture adoption: `crates/dh-worker/tests/common/mod.rs:130` now configures `page_channel_path: Some(...)`, and `crates/dh-worker/tests/common/mod.rs:154` connects through `Transport::Auto` with that page-channel path. On Linux, `crates/dh-worker/tests/common/mod.rs:149` waits for the path to exist before connecting.
- Non-Linux behavior: the page-channel assertion test is `#[cfg(target_os = "linux")]`, which is appropriate because the sibling fast path is Linux-only. On non-Linux x86_64 builds, the common helper will still pass `page_channel_path: Some(...)`, but snapshot-store ignores it and the client has no page-channel arm, so tests can only exercise gRPC. That is sane as long as the fast-path claim remains explicitly Linux-scoped.
- Readiness comments: `crates/dh-snapshot/tests/snapstore_readiness.rs:13` now describes the page channel as active when a live socket is present and delegates live-path coverage to dh-worker. That matches the current test split.

Residual risk:

The primary 32 MiB regression test at `crates/dh-worker/tests/snapstore_large_put.rs:29` would still pass if its particular fixture instance fell back to gRPC after page-channel socket creation. The companion corrupt-cross-check test covers the shared helper behavior and makes this acceptable for this iteration, but the 32 MiB test itself should not be cited alone as proof of page-channel transport use.
