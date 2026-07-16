# Positive Notes

### P-1 — The core decision is correct and the "slower" framing is genuinely dismantled

The decision doc's central insight — that "build-and-spawn" only looks slow when
you imagine a second binary + rebuild, and that `serve_for_tests` collapses it to
a milliseconds-startup in-process server with no second artifact — is right. I
verified: the server builds once as a normal dev-dependency, and each
`spawn_store` is a sub-millisecond runtime spin-up. Rejecting the provisioned
long-running service (ops surface + shared mutable state in a *determinism* suite)
is the correct trade. This is a well-reasoned ADR.

### P-2 — Every citation in the decision doc checks out

`serve_for_tests` (build_server.rs:242), `serve_for_tests_with_metrics`
(build_server.rs:249), and the sibling's `page_channel_fallback.rs` integration
test (which uses the *exact* same `serve_for_tests` seam at line 66) all exist
and match the doc's description. The "same sibling path-dep pattern as everything
else" and "HEAD-wins coupling holds them together by construction" claims are
accurate. No hand-waving.

### P-3 — Cargo target-gating is exactly right and verified

Placing the heavy deps under `[target.'cfg(target_arch = "x86_64")'.dev-dependencies]`
is deliberate and correct. `cargo metadata --filter-platform aarch64-...` confirms
`snapstore-server` and `snapstore-manifest` are **completely absent** from the
aarch64 dependency graph, and `determinism-tests` is the *only* workspace crate
that pulls them in (on x86 only). The aarch64 cross-clippy finished in 2.82s
without compiling the server closure. The comment "every test target here gates to
empty on other arches, so the KVM plumbing need not even compile there" extends
cleanly to the new server deps. This is the right way to keep the aarch64 lane
cheap.

### P-4 — Hermeticity is real and holds under stress

`stores_are_hermetically_isolated` is a genuine isolation test (a ref minted in
store A is `is_err()` in store B), and the property held under stress: 5
iterations at `--test-threads=8` produced **zero flakes**. Per-test `TempDir` +
per-test UDS path inside that dir means no path collisions, no TempDir races, no
shared state. The decision doc's hermeticity promise is backed by the
implementation.

### P-5 — The page() distinctness guard and FULL-container helper show care

`page(fill)` deliberately perturbs `p[0]` "so dedup can't collapse" same-fill
pages, and `full_container` uses the client's *own* `build_snapshot_container`
helper rather than hand-rolling a `.spm` — so the round-trip test exercises the
real manifest surface, not a test-only encoding. The
`re_put_is_deduped_and_ref_stable` test correctly asserts the strict `(0, 2)`
dedup shape and ref-stability, which is the right content-addressing invariant.

### P-6 — Pure-gRPC path is correctly selected

The helper uses `Transport::Uds(uds_path)` with `page_channel_path: None` in the
server config, so the client's Linux page-channel fast path is not engaged
(`try_connect_page_channel` only fires on `Transport::Auto`). The tests exercise
the gRPC-over-UDS path deterministically, which is the right seam for an R12
"real store, real durability" acceptance.
