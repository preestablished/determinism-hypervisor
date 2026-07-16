## Action Items

### Critical
- [ ] None.

### Important
- [ ] None.

### Suggestions
- [ ] [`.github/workflows/ci.yaml:2-4`] Scope `push:` to `branches: [main]` (and optionally add a `concurrency` group with `cancel-in-progress`) to stop every same-repo-PR commit running the full matrix twice. (S1)
- [ ] [`.github/workflows/ci.yaml` top-level] Add an explicit least-privilege `permissions: { contents: read }` block to document and enforce minimal token scope. (S2)
- [ ] [`.github/workflows/ci.yaml:31,63`] Optionally SHA-pin `dtolnay/rust-toolchain@stable` if/when the repo adopts a supply-chain pinning policy across all workflows; currently consistent with existing convention, leave as-is otherwise. (S3)
- [ ] [`.github/workflows/ci.yaml:15-18`] Consider `fail-fast: false` on the host matrix so an x86_64 failure does not cancel the aarch64 leg, surfacing arch-specific breakage. (S4)
