# Critical & Important Findings

## Critical

None.

## Important

### I-1. fmt scoping pipeline can silently shrink to a partial scope on `jq`/metadata failure

**File:** `.github/workflows/ci.yaml:58`

```yaml
- run: cargo fmt --check $(cargo metadata --no-deps --format-version 1 | jq -r '.packages[].name' | sed 's/^/--package /' | tr '\n' ' ')
  working-directory: repo
```

**Why this matters.** The whole point of this step is to gate formatting across
*all* workspace members. But the command substitution swallows failures:

- The pipeline has no `set -o pipefail`. GitHub Actions `run:` uses
  `bash -e {0}` by default — `-e` aborts on a failed *simple* command, but the
  exit status of a pipeline is the **last** command's status (`tr`), so if
  `cargo metadata` or `jq` fails mid-pipe, `tr` still exits 0 and `-e` does
  **not** fire.
- A non-empty-but-truncated `jq` result would scope `cargo fmt` to a *subset* of
  members, and `cargo fmt --check` would pass on that subset — a **green check
  that silently skipped crates**. This is the dangerous failure mode: a fmt
  regression in an un-scoped crate ships unnoticed.
- The fully-empty case is merely loud, not silent: `echo '' | sed 's/^/--package /'`
  emits a bare `--package` token, so the command becomes
  `cargo fmt --check --package` and fails — verified locally. So empty → red
  (acceptable), partial → green (the real hazard).

**Severity rationale.** Not Critical because the happy path is exercised and
correct, and a totally-broken `cargo metadata` already aborts an earlier step
(`cargo clippy`/`build` would also fail). It is Important because the gate can
degrade to a false pass without anyone noticing, defeating its purpose.

**Fix.** Make the pipeline fail-closed and assert non-empty before running fmt:

```yaml
- name: fmt (workspace members only)
  working-directory: repo
  shell: bash
  run: |
    set -euo pipefail
    pkgs="$(cargo metadata --no-deps --format-version 1 \
      | jq -r '.packages[].name' \
      | sed 's/^/--package /')"
    test -n "$pkgs" || { echo "::error::no workspace packages resolved for fmt"; exit 1; }
    # shellcheck disable=SC2086
    cargo fmt --check $pkgs
```

`set -o pipefail` turns a `jq`/`cargo metadata` failure into a hard CI failure;
the `test -n` guard rejects an empty/partial-to-empty expansion. Word-splitting
the `--package …` list across crates is intentional, hence the unquoted `$pkgs`.

**Alternative (simpler, no jq):** `cargo fmt --check -p` does not take a
glob, but `cargo fmt --check` run from each member, or
`taplo`-free reliance on the fact that **`cargo fmt` without `--all` already
formats only the current package + path deps reachable *as members*** — note
that plain `cargo fmt --check` (no `--all`) at the workspace root formats only
the *default-members* / current package, which may itself be the intended scope.
Worth confirming whether `--all` was ever needed; if default-member behavior
already excludes siblings, the whole `jq` pipeline could be dropped. (Suggestion
S-1 expands on this.)
