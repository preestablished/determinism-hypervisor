# Positive notes

- **Protection contexts match reality exactly.** The single most dangerous
  failure mode for this iteration — required-check context strings not
  matching GitHub's actual matrix display names — was verified against the
  live `commits/main/check-runs`: the names are byte-identical
  (`host (ubuntu-latest, --workspace)`, `host (ubuntu-24.04-arm,
  --workspace)`, `kvm-intel`). The matrix `include` uses `cargo-args` as a
  matrix dimension, which is exactly why the display name carries
  `, --workspace`; the lock author got the include-key naming right.

- **The comparator is fail-closed on every adversarial input tried:**
  doctored value, bogus/unknown key, CRLF, trailing whitespace, and a
  zero-keys-parsed guard. The `checked -eq 0` guard in particular catches
  "comparator silently passed because the lock got truncated/empty" — a
  classic green-but-broken trap.

- **`set -e` hazard is correctly (if subtly) avoided.** The `live_value`
  call lives in an assignment, which bash exempts from `set -e` even when
  the inner pipeline fails under pipefail. A missing live field degrades to
  empty-string → drift, not an unclean abort. Confirmed by direct test.

- **`split on FIRST '='` contract is honored** (`${line%%=*}` / `${line#*=}`),
  so `cpu_brand=Intel(R) Core(TM) i5-8400 CPU @ 2.80GHz` round-trips
  byte-exact even though the value contains `(`, `)`, `@`, spaces. Live
  green run proves it.

- **`needs:` chain is right.** A drifted host makes its canary results
  meaningless, so skipping the canary on drift (job 2 `needs` job 1) is the
  correct semantics, not a missed signal.

- **Sibling checkouts in the nightly are correct and complete.**
  `determinism-tests` pulls `detguest-host` via a workspace path dep into
  `../guest-sdk`, and `determinism-proto` into `../control-plane`; the
  canary job checks out both. The build would fail without them, and they
  are present. dtolnay toolchain usage is consistent with `ci.yaml`.

- **CONTRIBUTING.md is accurate.** Every factual claim cross-checks: the
  protection JSON (`required_status_checks`, `strict=false`,
  `enforce_admins` exemption), the fmt-scoping rationale (matches
  `ci.yaml`'s per-member `cargo fmt --check`, NOT `--all`), and the arm
  lane's cfg-gating story. The "red determinism = P0, never worked around"
  framing matches the product thesis.

- **The lock header documents its own parse contract** and explicitly notes
  `vmm_version` is deliberately absent (code-side, runtime-reported) — so a
  future reader doesn't "fix" the lock by adding a 4th tuple field the
  comparator can't source.

- **shellcheck clean, YAML valid, canary green, clippy clean** — all
  verified live this session.
