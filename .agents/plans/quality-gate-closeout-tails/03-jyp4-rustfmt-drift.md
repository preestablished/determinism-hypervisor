# Package 03 — `jyp4`: rustfmt drift in `runctl.rs` / `rss_regression.rs`

Bead: `determinism-hypervisor-jyp4`
Filed: `.agents/requests/lease-semantics-doc-and-orphan-slot-warn/04-resolution.md`
(2026-07-10): "`cargo fmt --all -- --check` was already red on clean main in
`crates/dh-vmm/src/runctl.rs` and `crates/dh-worker/tests/rss_regression.rs`".

## Expected outcome (read first)

**Likely already fixed.** At HEAD, `rustfmt --check --edition 2021` on both
files exits 0 (verified 2026-07-15, stable rustfmt aarch64-apple-darwin), and
commit `dd49ebf` ("Restore hypervisor CI compatibility", 2026-07-11) touched
exactly these two files. Expect to verify-and-close.

Two grounding caveats to carry into verification:

- The bead was filed against `cargo fmt --all -- --check`, but the **CI bar is
  narrower**: `ci.yaml` deliberately avoids `--all` because it would also
  check the sibling path-dep checkouts. Use the CI shape (below) as the
  acceptance command. Sibling formatting must not gate this repo.
- The workspace edition is **2021** (root `Cargo.toml`). Running rustfmt with
  `--edition 2024` produces spurious import-reordering diffs (observed while
  drafting) — do not be fooled by that into "fixing" style-2024 drift.

## Step 1 — Verify

On the Linux gate host, current stable rustfmt:

```bash
set -euo pipefail
members=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[].name')
test -n "$members"
cargo fmt --check $(printf -- '--package %s ' $members)
```

## Step 2a — If clean (expected)

```bash
bd close determinism-hypervisor-jyp4 -r "No longer reproduces at HEAD <sha>: runctl.rs and rss_regression.rs were reformatted in dd49ebf (2026-07-11); CI-shaped cargo fmt --check clean on <host> with rustfmt <version>. No code change."
```

## Step 2b — If still red

Run `cargo fmt --package <the failing package(s)>` and commit the result as a
formatting-only change:

- The diff must be produced by rustfmt alone — no manual edits riding along.
  Verify with `git diff` that the change is whitespace/ordering only.
- `crates/dh-vmm/src/runctl.rs` **is execution-path code**. rustfmt is
  semantics-preserving, but the repo convention is that touched execution-path
  code requires a determinism-suite rerun — honor it (cheap insurance, and it
  keeps the convention unconditional):

  ```bash
  cargo test -p determinism-tests   # on the KVM reference host
  ```

- Commit message should say "formatting only, produced by cargo fmt <version>".

## Acceptance

```bash
# CI-shaped fmt check exits 0:
members=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[].name')
cargo fmt --check $(printf -- '--package %s ' $members)

cargo test --workspace --all-targets                    # unchanged / green
cargo clippy --workspace --all-targets -- -D warnings   # unchanged / clean
```

Plus `cargo test -p determinism-tests` if 2b actually reformatted `runctl.rs`.

## Failure guidance

- **Diff appears only under a newer rustfmt** than CI's: CI uses current
  stable (`dtolnay/rust-toolchain@stable`), so if your stable is newer than
  the runner's, a diff you see may not gate CI yet — but it will as soon as
  the runner updates. Apply it anyway (it is still rustfmt-canonical output)
  and note the rustfmt version in the bead closure.
- **Drift in files other than the two cited**: fix them in the same
  formatting-only commit (the bead is about the fmt gate being red, the two
  files were just the instances), but list every reformatted file in the bead
  note.
- **No Linux gate host reachable**: follow 00-overview's "Where To Execute" —
  the HEAD CI run's fmt lane is first-line evidence; otherwise record
  advisory macOS results in the plan dir and stop without closing the bead.
