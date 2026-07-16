# Action Items

### Critical

_None._

### Important

- [ ] **Make the fmt step fail-closed.** In `.github/workflows/ci.yaml:58`, the
  `cargo fmt --check $(cargo metadata … | jq … | sed … | tr …)` step silently
  becomes a **no-op that exits 0** if the command substitution is ever empty
  (jq missing/erroring, or `cargo metadata` hiccup) — because the repo root is a
  **virtual manifest**, so `cargo fmt --check` with no `--package` args checks
  nothing and returns 0. Default GHA Linux shell is `bash -e` with **pipefail
  OFF**, so a mid-pipe jq failure does not trip errexit. Reproduced locally.
  Currently dormant (jq is preinstalled on `ubuntu-latest` and
  `ubuntu-24.04-arm`), but it is a fail-open gate with no signal. Fix by adding
  `set -euo pipefail` **and** asserting the package list is non-empty before
  invoking `cargo fmt`:
  ```yaml
  - run: |
      set -euo pipefail
      pkgs=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[].name | "--package " + .')
      test -n "$pkgs" || { echo "::error::no workspace members resolved for fmt"; exit 1; }
      cargo fmt --check $pkgs
    working-directory: repo
  ```
  Acceptance: a run with `jq` removed must **fail**, not pass green.

### Suggestions

- [ ] **Simplify the member-list derivation (S-1).** Fold `sed`+`tr` into the jq
  template (`jq -r '.packages[].name | "--package " + .'`) to remove two silent-
  failure surfaces, or replace the metadata pipeline with an explicit
  `-p <crate>` list / a `for d in crates/* tools/dh-cli` loop. Trade-off:
  explicit lists need updating when crates are added, but a missing crate then
  fails loudly elsewhere rather than silently.
- [ ] **Close the CAP_PERFMON gap or stop modelling policy (S-2/S-3).**
  `pmu_available()` skips a non-root user holding `CAP_PERFMON`, who could
  actually open the counter. Moot on the §7.4 lab box (paranoid=1 already
  passes), but to remove the modelling entirely, switch the tests to *attempt
  the open* and skip only on `EACCES` (match by `raw_os_error() == Some(EACCES)`,
  which needs threading errno through `CounterError::Open`), treating all other
  errors as hard failures.
- [ ] **Document the `unwrap_or(i32::MAX)` intent (S-4).** Add an inline note in
  `crates/dh-detclock/src/counter.rs:241` —
  `// unparseable => MAX => strictest => skip (do NOT change to unwrap_or(0))` —
  to prevent a future fail-open "simplification."
