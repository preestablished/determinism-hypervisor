# Critical and Important Issues

## Critical

None. This is a docs-only change to an ops runbook; it introduces no code,
no executable instructions that destroy data, and no security regression. The
documented commands are read/install operations an operator runs deliberately.

## Important

### I1 — Security posture not cross-referenced for writable tool locations (consistency gap)

- **Severity:** Important (documentation completeness / security hygiene)
- **Location:** `docs/ops/github-runner.md:60–69` (section intro) and `:71–77` (table)
- **Description:** The same file, at `:34–48` ("Security: public repo +
  privileged runner"), establishes that this runner is on a **public** repo
  where fork-PR code execution is the canonical self-hosted hazard, and that
  `kvm-intel` jobs must be gated off fork PRs entirely. The new section then
  documents that jobs inherit a PATH including `~/go/bin`, `~/.local/bin`, and
  `~/.cargo/bin` — directories that are **writable by any job that runs on the
  runner**. On a persistent public-repo runner, a job can overwrite
  `~/go/bin/grpcurl` or `~/.cargo/bin/cargo-fuzz` with a trojaned binary that
  the next job silently executes. The existing fork-PR gate (`:45–48`) is the
  mitigation, but the new section never points back to it, so a reader
  provisioning tools here gets no signal that "user-local and writable" is a
  deliberate trust boundary rather than an incidental convenience.
- **Research reference:** Persistent runners accumulate poisoned caches and
  credentials; tools installed user-local (`~/go/bin`, `~/.cargo/bin`) are
  writable by any job that runs on the runner — documenting user-writable tool
  locations on a public-repo runner deserves a caveat.
- **Suggested fix:** Add one sentence to the section intro tying the writable
  PATH back to the existing fork-PR gate, e.g.:

  ```markdown
  These directories are writable by any job that runs on the runner, so a
  job could overwrite a tool the next job executes. That risk is bounded by
  the fork-PR gate in "Security: public repo + privileged runner" above —
  `kvm-intel` jobs never run untrusted fork code — which is the assumption
  this whole section rests on.
  ```

  This is the only Important-level item; it is a hardening/clarity addition,
  not a correctness defect — hence the overall APPROVE verdict.
