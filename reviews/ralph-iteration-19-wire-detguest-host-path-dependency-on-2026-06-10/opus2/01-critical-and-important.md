# Critical and Important Issues

## Critical

**None.** This is an additive `[dev-dependencies]` + integration-test change.
There is no production code path, no `unsafe`, no data-loss surface, and no
crash risk introduced. The new test links and passes.

## Important

**None blocking.** The items below are the closest to "important" but each
resolves to a low-severity note; I am deliberately not inflating severity for a
small additive change. They are restated here so a fixer sees the reasoning, then
carried into Suggestions / Action Items.

### I-1 (Low) — Reproducibility/CI gap is inherent to sibling path deps, only partially owned

- **File:** `Cargo.toml:27-28`
- **Severity:** Low (process, not code)
- **Description:** Per the cargo-workspace-path-deps research: sibling-repo path
  deps couple checkouts — "builds are not reproducible from this repo alone (no
  version/rev pinning — HEAD of whatever is on disk wins)" and "CI must check out
  the sibling at a compatible revision." This change adds two such deps. The inline
  comment documents *why* both crates are needed, and a bead (nln) tracks the
  production promotion, which is good. What I could not confirm from this diff is
  that **CI owns the `../guest-sdk` checkout** at a compatible revision — if CI
  does not check out the sibling repo at the expected relative path/depth, the
  `dh-devices` test target will fail to resolve the path dep. The
  `determinism-proto -> ../control-plane` precedent presumably already solved this,
  so the new deps likely inherit a working CI story, but it is worth an explicit
  confirmation.
- **Suggested fix:** Confirm the CI workflow that runs `cargo test` checks out
  `../guest-sdk` at a compatible commit (same as it does for `../control-plane`).
  If not, file/extend a bead so the sibling-checkout step covers guest-sdk. No
  code change required if CI already clones siblings generically.
- **Research reference:** `cargo-workspace-path-deps.md` — "Path deps that work
  locally but break CI (sibling not checked out / wrong path depth). Verify a
  clean-state build."

### I-2 (Low) — `read_region` happy path and `OutOfBounds` variant are not exercised

- **File:** `crates/dh-devices/tests/detguest_host_smoke.rs:121-129`
- **Severity:** Low (coverage)
- **Description:** The smoke test only exercises the `RegionReadError::NameNotFound`
  path of `read_region`. The documented `OutOfBounds` variant and the actual
  extent-stitching success path (the M1 acceptance behavior — reading bytes out of
  a resolved region across extents) are not touched here. The
  rust-integration-testing research flags "Asserting only the happy path; missing
  the error/boundary variants the API documents (e.g. each documented error enum
  variant should be reachable)." This is a *smoke* test by design (the doc comment
  scopes it to "linkage + contract check"), and the sibling crate's own unit tests
  cover stitching thoroughly (`read_region_stitches_three_discontiguous_extents`),
  so duplicating that here has limited value. I rate this Low, not Important,
  because the contract this repo most cares about (name resolution failure surfaces
  as a typed error, not a panic) *is* covered, and the real read-region wiring lands
  in bead nln where a fuller test belongs.
- **Suggested fix (optional):** When bead nln lands the real detchannel host side,
  add a `read_region` success + `OutOfBounds` case there. Not needed in this
  smoke test.
- **Research reference:** `rust-integration-testing.md` — failure/boundary
  variant coverage.
