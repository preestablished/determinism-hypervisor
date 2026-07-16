# Suggestions (non-blocking)

### S1 — Duplicate `push` + `pull_request` triggers double-run every branch PR
`ci.yaml:2-4`
```yaml
on:
  pull_request:
  push:
```
A commit pushed to a branch that has an open same-repo PR triggers the full
matrix twice (once for `push`, once for `pull_request`). This pre-exists on
`main` (not introduced here), but it now costs 2× across a 2-runner host matrix
plus the self-hosted box. Common fix is to scope `push` to protected branches
and let `pull_request` cover everything else:
```yaml
on:
  pull_request:
  push:
    branches: [main]
```
Optionally add a concurrency group to cancel superseded runs on the same ref:
```yaml
concurrency:
  group: ci-${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true
```
Be careful with `cancel-in-progress` on the self-hosted lane if you want every
push fully exercised; scoping it per-ref is usually fine.

### S2 — No top-level `permissions:` block
`ci.yaml` (file-level). The security research recommends minimal token scope.
The workflow only reads code, so an explicit least-privilege block documents
intent and avoids inheriting a broad repo/org default:
```yaml
permissions:
  contents: read
```
Add it at the top level (applies to both jobs). Non-blocking because public-repo
fork PRs already get a read-only token, but explicit is better.

### S3 — `dtolnay/rust-toolchain@stable` is unpinned (consistent with repo convention)
`ci.yaml:31, 63`. Pinning to a tag/branch rather than a commit SHA is a
supply-chain consideration, but this matches the existing repo convention (it
was already `@stable` on `main`) and `dtolnay/rust-toolchain` is a well-known
author — the research notes this as a commonly accepted middle ground. If the
project later hardens its supply chain, pin to a SHA with a version comment:
```yaml
- uses: dtolnay/rust-toolchain@<sha>  # stable, vX.Y.Z
```
Leave as-is unless the repo adopts a SHA-pinning policy across all workflows.

### S4 — Consider a fail-fast-disabled host matrix
`ci.yaml:15-18`. With the default `fail-fast: true`, an x86_64 failure cancels
the in-flight aarch64 leg (and vice versa), hiding arch-specific breakage. For a
project that explicitly cares about arm portability, surfacing both results is
useful:
```yaml
strategy:
  fail-fast: false
  matrix:
    runner: [ubuntu-latest, ubuntu-24.04-arm]
```
Minor; only matters when a failure is arch-specific.

### Note (no change needed)
The `/dev/kvm` readability check (`ci.yaml:64`) is a nice belt-and-suspenders
guard — it fails the `kvm-intel` lane loudly with a `::error::` annotation if the
box loses `/dev/kvm`, rather than silently letting the live-KVM tests self-skip
and giving a false green. Good as written.
