# Positive Notes

### Atomic dep + CI checkout, correctly reasoned in the commit

The change lands the `../snapshot-store` checkout in **all three** workspace-building lanes
(`ci.yaml` host matrix, `ci.yaml` kvm-intel, `nightly-drift.yaml` determinism-canary) in the
same commit as the `Cargo.toml` dep. The `Cargo.toml` comment states *why* this must be
atomic — "path deps resolve at cargo-metadata time" — which is exactly the cargo-workspace
pitfall ("path deps that work locally but break CI") called out in the research. This is the
correct, complete fix; splitting it would have red-CI'd every lane.

`Cargo.toml:39-44`, `.github/workflows/ci.yaml:54-57,104-107`,
`.github/workflows/nightly-drift.yaml:47-51`.

### Compile-time surface pin is the right shape for a readiness gate

`crates/dh-snapshot/tests/snapstore_readiness.rs` references sibling methods as function
items (`let _ = SnapstoreClient::put_pages;`) so the gate fails *at compile time* with a
readable symbol name if the sibling renames/removes/privatizes a method — instead of failing
"deep inside the M4 snapshot engine" later. This is a textbook contract pin (research: test
the documented contract, not internals) and avoids the tautological-test pitfall: it asserts
nothing about behavior, only that the seam exists. Every pin was verified against the real
sibling crate and the gate compiles and passes locally.

`crates/dh-snapshot/tests/snapstore_readiness.rs:105-130`.

### The one signature that the M4 engine will depend on *is* pinned by type

`_put_pages_signature` pins not just existence but the full shape
`(&SnapstoreClient, Vec<(u64, Vec<u8>)>) -> impl Future<Output = Result<(u64, u64), ClientError>>`.
That matches the real `put_pages` exactly (verified in `client.rs:95`). Pinning the one
signature the engine relies on — the (deduped, total) page-upload counts — while leaving
existence-only pins for the rest is a well-judged cost/benefit split, explained in the
comment.

`crates/dh-snapshot/tests/snapstore_readiness.rs:152-157`.

### Dev-dependency hygiene

`snapstore-client` is a `[dev-dependencies]` entry on `dh-snapshot`, inherited via
`workspace = true`. The heavy gRPC/tonic closure therefore never enters the production
dependency graph — only test builds pull it. This is exactly the test-only-dep discipline the
research flags, and it mirrors the existing sibling-dep convention.

`crates/dh-snapshot/Cargo.toml:7-10`, `Cargo.toml:44`.

### Self-hosted runner exposure left intact, not weakened

Adding checkouts to the `kvm-intel` job did not touch its fork-PR guard, and the change
introduces no new `pull_request`-triggered self-hosted path. The single highest-risk item in
a public-repo + self-hosted-runner setup stays correctly closed.

`.github/workflows/ci.yaml:88-90`.

### Documentation kept honest

The CI header comment, the `Cargo.toml` dep comment, and the `dh-snapshot/Cargo.toml`
dev-dep comment were all updated to name the third sibling and explain the seam — so the
"sibling-repo path deps are documented" review check passes. The `test-partitioning.md`
update proactively records the new `zstd-sys` C dependency in the aarch64 cross-build recipe,
including a no-sudo clang fallback, so an arm dev hitting a missing-header build error has the
fix in hand.

`.github/workflows/ci.yaml:18-21`, `Cargo.toml:39-44`,
`crates/dh-snapshot/Cargo.toml:8-9`, `docs/ops/test-partitioning.md:172`.
