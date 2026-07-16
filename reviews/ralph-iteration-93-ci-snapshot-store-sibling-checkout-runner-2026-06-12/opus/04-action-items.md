# Action Items

### Critical
- [ ] None.

### Important
- [ ] [docs/ops/github-runner.md:60–69] Add one sentence to the "Tool
  provisioning" intro that ties the writable user-local PATH directories
  (`~/go/bin`, `~/.local/bin`, `~/.cargo/bin`) back to the existing fork-PR
  gate in the "Security: public repo + privileged runner" section (:34–48).
  These dirs are writable by any job on this persistent public-repo runner, so
  a job could overwrite a tool the next job runs; the doc should state that this
  risk is bounded by `kvm-intel` jobs never running untrusted fork code.
  Non-blocking — verdict is APPROVE — but the most valuable single addition.

### Suggestions
- [ ] [docs/ops/github-runner.md:74–75] Add a "Notes:" bullet flagging that the
  installs are unpinned (`grpcurl ...@latest`, `cargo install cargo-fuzz`
  without `--version`); point readers to the Status column versions
  (grpcurl v1.9.3, cargo-fuzz v0.13.2) and the pinned-install forms for a
  reproducible rebuild. (Supply-chain / reproducibility.)
- [ ] [docs/ops/github-runner.md:73–77] Anchor the M5/M6/M7 milestone
  shorthands with a single pointer (e.g. "see IMPLEMENTATION-PLAN") in the
  section intro so future readers can resolve them.
- [ ] [docs/ops/github-runner.md:77] Qualify the `stress-ng` apt candidate as
  "candidate 0.17.06-1build1 as of 2026-06-12" since the archive candidate may
  drift before the sudo-gated install happens.
- [ ] [docs/ops/github-runner.md:84] Tighten the grpcurl verify grep to
  `grep -E '^\s+mod\s'` (or `grep '^\s*mod\b'`) so it does not also match
  `=> mod` / other indented `mod` tokens. Cosmetic; current command works.
