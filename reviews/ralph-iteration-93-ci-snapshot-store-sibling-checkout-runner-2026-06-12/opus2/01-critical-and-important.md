# Critical and Important Issues

## Critical

**None.** This is a docs-only checkpoint. Every factual claim in the new section was
independently verified against the live runner box and the repo (see 00-overview). Nothing
in the diff is wrong or dangerous as written.

---

## Important

### I-1 — Unpinned install commands contradict the runbook's own reproducibility stance
**Severity:** Important
**File:** `docs/ops/github-runner.md:74-77` (Install column of the tool table)

The install commands pin nothing:

```
go install github.com/fullstorydev/grpcurl/cmd/grpcurl@latest
cargo install cargo-fuzz
rustup toolchain install nightly
```

`@latest` (grpcurl), bare `cargo install` (resolves to the newest crates.io release at run
time), and `nightly` (a moving channel) all mean "rebuild from this runbook on a different
day → different binaries." That is in direct tension with the document this very change
keeps citing — `ci/determinism-class.lock`, whose whole premise is byte-exact host pinning —
and with `docs/decisions/proto-seam.md`'s deliberate choice to *vendor* protoc precisely so
"runner provisioning is a no-op." The Status column already records the exact versions that
are live and known-good (`v1.9.3`, `0.13.2`, `1.98.0-nightly (2026-06-08)`), so the
reproducible form is one edit away.

For tools whose output feeds a *determinism* product, a rebuild that silently swaps
`cargo-fuzz` or grpcurl versions is exactly the class of drift the rest of this repo works to
prevent. Nightly is explicitly carved out as not-determinism-class (correct — see I-2 caveat
below), but `grpcurl` and `cargo-fuzz` are not, and their install lines should be pinnable.

**Suggested fix** — pin to the verified versions, keep `@latest`/bare forms only as the
documented "bump" path:

```
| `grpcurl` | M6 smoke tests | ✅ v1.9.3 … | `go install github.com/fullstorydev/grpcurl/cmd/grpcurl@v1.9.3` |
| `cargo-fuzz` | M5 DHILOG fuzz | ✅ v0.13.2 … | `cargo install cargo-fuzz --version 0.13.2 --locked` |
```

(`--locked` additionally makes `cargo install` honor the published Cargo.lock, removing
transitive-dep drift.) A one-line note — "bump = re-run without the version pin, then update
the Status column and re-verify the fuzz lane green" — preserves the upgrade story.

---

### I-2 — Privileged public-repo runner + user-writable tool dirs: integrity assumption is left implicit
**Severity:** Important
**File:** `docs/ops/github-runner.md:60-69` (PATH/`.path` paragraph) in light of the
existing Security section at lines 34-48

The new section establishes that runner jobs execute with `~/go/bin`, `~/.local/bin`, and
`~/.cargo/bin` on PATH. Combined with this file's own threat model — a **public** repo, a
runner with `/dev/kvm` and a relaxed perf-paranoid level, three neighbor repos' runners on
the same box (lines 36-38) — those directories are a poisoning surface: any job that runs on
this box (including a neighbor repo's job, or an approved fork PR) can overwrite
`~/go/bin/grpcurl` or `~/.cargo/bin/cargo-fuzz`, and the *next* job will silently pick up the
trojaned binary because the dir is on the captured PATH. The existing fork-PR-approval gate
(line 41-44) mitigates the fork vector but does nothing about cross-repo neighbors sharing
`infra-admin`'s home, and approval-gated fork PRs still run real code once approved.

The new content makes this surface concrete without acknowledging it. A runbook that tells an
operator "user-local installs are visible to jobs without touching the service unit" should,
on a box it itself flags as privileged-and-public, also say how an operator detects/recovers
a tampered tool dir.

**Suggested fix** — add one bullet to the Notes block tying back to the Security section:

```
- **Tool dirs are job-writable.** `~/go/bin`, `~/.cargo/bin`, and `~/.local/bin` are on the
  captured runner PATH (above) and writable by every job on this box, including neighbor
  repos' jobs and approved fork PRs (see Security, §"public repo + privileged runner"). Treat
  the verified versions in the table as a fingerprint: re-verify (`go version -m`,
  `cargo-fuzz --version`) after any incident, and prefer re-installing from the pinned
  commands over trusting an in-place binary.
```

This is the one place where the *new* content materially expands the attack surface the file
already worries about, so it is the highest-value addition. Not a merge blocker for a
checkpoint, but should land before this section is relied on operationally.
