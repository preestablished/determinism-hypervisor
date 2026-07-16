# Action Items

## Critical

- [ ] None.

## Important

- [ ] **Make the fmt scoping pipeline fail-closed (I-1).** In
      `.github/workflows/ci.yaml:58`, a `jq`/`cargo metadata` failure currently
      cannot abort the step (pipeline exit status comes from `tr`, no
      `pipefail`), so the gate can silently degrade to a partial member scope and
      report a false green. Replace the inline command-substitution with a
      multi-line `shell: bash` block that sets `set -euo pipefail`, captures the
      `--package …` list into a variable, asserts it is non-empty
      (`test -n "$pkgs" || exit 1`), then runs `cargo fmt --check $pkgs` (unquoted
      on purpose for word-splitting). Snippet in `01-critical-and-important.md`.

## Suggestions

- [ ] **(S-1)** Empirically check whether plain `cargo fmt --check` (no `--all`,
      run with `working-directory: repo`) already excludes the sibling path deps —
      they are path *dependencies*, not workspace *members*. If it does, drop the
      `cargo metadata | jq | sed | tr` substitution entirely and use
      `cargo fmt --check`, which removes the `jq` dependency and eliminates I-1's
      hazard at the source. Test: introduce a deliberate misformat in a
      `guest-sdk` file and confirm plain `cargo fmt --check` ignores it.
- [ ] **(S-2)** If the explicit member list stays, filter to
      `.workspace_default_members[]` or `.packages[] | select(.source == null)`
      instead of `.packages[].name`, so future internal/helper crates don't
      silently change fmt scope. Low priority for today's flat 9-member workspace.
- [ ] **(S-3)** Add a trailing comment on
      `crates/dh-detclock/src/counter.rs:241` explaining that
      `unwrap_or(i32::MAX)` is deliberately fail-closed (unparseable level → treat
      as maximally strict → non-root skips), so a later refactor doesn't
      "simplify" it to `unwrap_or(0)` and re-break hosted CI.
- [ ] **(S-4)** No change. Informational: the `<=1` paranoid gate already
      accounts for the sampling-event attr; documented in `02-suggestions.md` for
      whoever later wires real overflow sampling via
      `route_overflow_to_thread`.
