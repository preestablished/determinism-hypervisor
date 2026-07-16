# Review Overview — snapstore-store readiness verification

- **Branch:** `ralph/iteration-59-snapshot-store-readiness-verification`
- **Base:** `main`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus (2nd reviewer)
- **Bead / risk:** 4nj / R12 (M4 store-integration readiness gate)

## Summary

This change wires `snapstore-client` (from sibling repo `../snapshot-store`) in as the
third sibling-repo path dependency, exactly mirroring the existing
`determinism-proto -> ../control-plane` and `detguest-host/detguest-wire -> ../guest-sdk`
precedent. The dependency is declared once under `[workspace.dependencies]`, consumed in
exactly one place (`dh-snapshot`'s `[dev-dependencies]`), and exercised by a new
compile-time surface-pin test (`tests/snapstore_readiness.rs`) that fails the build with a
readable name if snapshot-store renames or removes any method on the TakeSnapshot /
RestoreSnapshot / input-log surface. All three CI lanes that build the workspace
(`ci.yaml` host + kvm-intel, `nightly-drift.yaml` canary) gain the matching
`actions/checkout` of `snapshot-store`, landing atomically with the manifest change so
`cargo metadata` resolves. The docs update flags the new C cross-compile burden
(`zstd-sys` via snapstore-client) for the aarch64 lane.

I independently verified: `cargo metadata --no-deps` resolves; the readiness test
compiles and passes; every pinned method exists in the sibling crate with a matching
signature; the `page_channel_path` field referenced by the test is **not** `cfg`-gated, so
the pin compiles on macOS even though `snapstore-localpath` is Linux-only; and no
non-test consumer of the dep exists in this repo.

## Verdict

**APPROVE**

The change is correct, minimal, follows established precedent precisely, and is verified
green locally. The findings below are all non-blocking: a few are forward-looking gaps in
what the surface-pin actually guarantees (it pins method *existence* broadly but *signatures*
only narrowly), plus minor doc/consistency nits. None of them should hold the merge.

## Stats

- **Files changed:** 6 (excluding `Cargo.lock`)
- **Lines (excluding lock):** +104 / −3
- **Lines (including lock):** +1662 / −99 (lock churn is the transitive closure of
  snapstore-client: tonic/prost/zstd/tower/hyper-util etc. — expected and matches the
  manifest change)
- **Commits:** 1 (`6ab5924` ralph: iteration 59 checkpoint — snapstore-client workspace
  dep + readiness gate)
