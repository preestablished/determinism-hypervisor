# Critical & Important Findings

## Critical

None.

---

## Important

### I-1. fmt step silently degrades to a no-op (exit 0) if the arg substitution is ever empty

**File:** `.github/workflows/ci.yaml:58`

```yaml
- run: cargo fmt --check $(cargo metadata --no-deps --format-version 1 | jq -r '.packages[].name' | sed 's/^/--package /' | tr '\n' ' ')
  working-directory: repo
```

**The trap.** The repo root `Cargo.toml` is a **virtual manifest** (`[workspace]`
with no `[package]`). I verified empirically on this box that
`cargo fmt --check` with **no `-p`/`--package` arguments** at a virtual-manifest
root **exits 0 and checks nothing** — there is no "current crate" to fall back
to. (Contrast a normal crate root, where bare `cargo fmt` formats that crate.)

So if `$(...)` ever expands to empty, the step does not error — it *passes
green while checking zero files*. The fmt gate silently disappears. Ways the
substitution can go empty, none of which fail the step:

1. **`jq` missing or erroring.** GitHub's default Linux `run:` shell is
   `bash -e {0}` — errexit **on**, but **pipefail OFF** (I confirmed the
   default). In `cargo metadata | jq | sed | tr`, the pipeline's exit status is
   `tr`'s (always 0). A failing/missing `jq` mid-pipe does **not** trip
   errexit, the command substitution yields empty, and `cargo fmt --check`
   no-ops to 0. I reproduced the empty-substitution → exit 0 path directly.
2. **`cargo metadata` non-fatal hiccup** producing empty/garbage that jq turns
   into nothing.

**Why it's "Important" not "Critical":** jq *is* preinstalled on both
`ubuntu-latest` and `ubuntu-24.04-arm` GitHub-hosted images, so the failure mode
is **latent**, not active. The gate works today. But it is a fail-*open* design
for the one thing the step exists to enforce, with zero signal when it breaks —
exactly the kind of regression that goes unnoticed until unformatted code merges.

**Fix (make it fail-closed). Pick one:**

- Add `set -euo pipefail` and assert non-empty, e.g.:
  ```yaml
  - run: |
      set -euo pipefail
      pkgs=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[].name | "--package " + .')
      test -n "$pkgs" || { echo "::error::no workspace members resolved for fmt"; exit 1; }
      cargo fmt --check $pkgs
    working-directory: repo
  ```
  (Folding `sed`+`tr` into the jq template also removes two processes from the
  pipe and one more silent-failure surface.)
- Or, simpler and arguably better: drop the metadata/jq machinery entirely and
  run `cargo fmt --check` **per crate via a glob over `crates/*` + `tools/*`**,
  or run rustfmt directly on the workspace's own source roots. The whole reason
  for `--all`-avoidance is path-dep leakage; an explicit member list that does
  not depend on a JSON pipeline is more robust. See S-1.

**Acceptance:** a CI run with `jq` deliberately removed (or `cargo metadata`
stubbed to empty) must **fail**, not pass.
