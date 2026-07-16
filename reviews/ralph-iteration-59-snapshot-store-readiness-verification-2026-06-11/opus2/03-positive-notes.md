# Positive Notes

### P1 — Dependency declared once in `[workspace.dependencies]`, inherited via `workspace = true`

`Cargo.toml:44` declares `snapstore-client` once, and
`crates/dh-snapshot/Cargo.toml:10` consumes it as `snapstore-client.workspace = true`. This
is exactly the pattern the research flags as correct ("Declare shared deps once under
`[workspace.dependencies]` and inherit with `dep.workspace = true`" / "Forgetting `workspace
= true` … cargo silently treats the dep as independent"). The diff does it right.

### P2 — Test-only dependency correctly scoped to `[dev-dependencies]`

The only consumer is `dh-snapshot`'s `[dev-dependencies]`, so `snapstore-client` (and its
heavy tonic/prost/zstd transitive closure) never enters this repo's *production* dependency
graph. The manifest comment (`crates/dh-snapshot/Cargo.toml:8-9`) states this intent
plainly. Matches research best practice: "Keep test-only external deps in
`[dev-dependencies]` so the production dependency graph stays clean." I confirmed there is
no non-test consumer anywhere in `crates/`, `tools/`, or `tests/`.

### P3 — Atomic landing of manifest + CI checkout — no broken intermediate state

The same commit adds the path dep *and* the `actions/checkout` of `snapshot-store` to all
three workspace-building lanes (ci.yaml host, ci.yaml kvm-intel, nightly-drift canary). The
top-of-file CI comment was updated in lockstep (ci.yaml:18-21). Because path deps resolve at
`cargo-metadata` time, splitting these would have produced a CI-red intermediate commit;
keeping them atomic is the correct call and the commit message says so explicitly.

### P4 — Self-hosted runner fork-PR guard left intact and is correct

`ci.yaml:90` retains
`if: github.event_name == 'push' || github.event.pull_request.head.repo.full_name == github.repository`
on the `kvm-intel` self-hosted job. Adding a *third* repo checkout to a self-hosted runner is
exactly the scenario the research warns about ("a self-hosted runner's host access … is
itself the prize"), and the guard correctly ensures fork PRs never reach the box to trigger
that extra checkout. The nightly-drift self-hosted jobs run only on schedule/dispatch
(default branch), so they're not fork-reachable either. No regression here.

### P5 — Precise, scoped CI checkout — only the workspace-building lanes get the sibling

The `nightly-drift.yaml` `determinism-class` job (which only runs
`ci/check-determinism-class.sh`, no cargo) and the `alert-on-failure` job (pure `gh` CLI)
were correctly **not** given the `snapshot-store` checkout — they don't build the workspace,
so they don't need it. Only the `determinism-canary` job (which runs `cargo test`) got it.
This shows the change was reasoned per-job rather than blanket-applied, which keeps the
nightly drift-measurement job minimal.

### P6 — The pin test exists as a named `#[test]` so the gate shows up in `cargo test` output

`snapstore_readiness.rs:73-77` adds an empty `#[test] fn
snapstore_client_surface_is_present()` whose real work is the compile-time references. The
comment explains *why* the empty body exists ("so the gate shows up in `cargo test` output
as an explicit pass"). This is a thoughtful touch: a pure compile-only file would pass
silently and be easy to forget; the named test makes the gate visible in CI logs. I
verified it runs and passes (`1 passed`).

### P7 — Comments consistently explain the *why*, including the temporary-consumer rationale

Both manifest comments (`Cargo.toml:39-44`, `dh-snapshot/Cargo.toml:8-9`) and the test
module doc explain that the dev-dep is the *only* consumer "until the M4 store integration
lands a runtime consumer." This pre-empts the obvious reviewer question ("why is a
production-looking workspace dep only used by a test?") and ties the change back to bead 4nj
/ risk R12. Good provenance hygiene for a Ralph-loop iteration.

### P8 — Doc update honestly surfaces the new cross-compile cost

`docs/ops/test-partitioning.md:18` proactively documents that the aarch64 lane now needs a C
cross-toolchain for `zstd-sys` (in addition to blake3's NEON C), and even provides a no-sudo
clang fallback. I verified `zstd` arrives transitively via `snapstore-manifest`, so the
attribution "zstd-sys via snapstore-client" is accurate. Surfacing a hidden build cost in the
same change that introduces it is exactly right.
