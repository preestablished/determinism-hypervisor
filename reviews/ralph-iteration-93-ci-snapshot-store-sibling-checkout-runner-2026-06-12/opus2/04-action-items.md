# Action Items

### Critical
- [ ] None. (Docs-only checkpoint; all factual claims independently verified accurate against the live runner box and the repo.)

### Important
- [ ] [docs/ops/github-runner.md:74-77] Pin the `grpcurl` and `cargo-fuzz` install commands to the verified versions already shown in the Status column (`go install …@v1.9.3`; `cargo install cargo-fuzz --version 0.13.2 --locked`), keeping the bare `@latest`/unpinned forms only as a documented "bump" path. Unpinned installs contradict this repo's own pinning ethos (`ci/determinism-class.lock`, `docs/decisions/proto-seam.md` vendored-protoc "no-op"). Nightly stays unpinned by design — leave it.
- [ ] [docs/ops/github-runner.md:60-69 + 79-91] Add a Notes bullet acknowledging that the captured-PATH tool dirs (`~/go/bin`, `~/.cargo/bin`, `~/.local/bin`) are writable by every job on this privileged public-repo box — including neighbor repos' jobs and approved fork PRs (cross-ref the Security section, lines 34-48) — and tell operators to treat the table's versions as a fingerprint and re-verify (`go version -m`, `cargo-fuzz --version`) after any incident rather than trusting an in-place binary.

### Suggestions
- [ ] [docs/ops/github-runner.md:93-108] In the Registration ("for rebuilds") section, add a one-line pointer that the Tool provisioning step is part of a from-scratch rebuild and that tools should be installed before `config.sh` (or `config.sh` re-run after) so the captured `.path` includes them.
- [ ] [docs/ops/github-runner.md:71] Add a "how to re-verify this table" pointer so the date-stamped Status column can be refreshed without archaeology; bump the header date when refreshed.
- [ ] [docs/ops/github-runner.md:77] Pin the `stress-ng` apt install to the recorded candidate (`stress-ng=0.17.06-1build1`), and note an optional `apt-mark hold` if soak-load determinism matters (parity with the kernel/microcode hold pattern).
- [ ] [docs/ops/github-runner.md:84] Make the grpcurl-version verification grep portable: `grep -E '^[[:space:]]*mod'` (or `grep mod`) instead of the GNU-only `grep '^\s*mod'`.
- [ ] [docs/ops/github-runner.md:74,77] Add a half-sentence that `grpcurl`/`cargo-fuzz`/`stress-ng` are pre-staged for M5/M6/M7 lanes that no workflow invokes yet (verified: `ci.yaml` kvm-intel job runs only `cargo build/test --workspace`; no `crates/*/fuzz` targets exist) — so "installed" is not yet "exercised in a runner job." The `protoc` "Not needed" row, by contrast, is exercised by every build.
