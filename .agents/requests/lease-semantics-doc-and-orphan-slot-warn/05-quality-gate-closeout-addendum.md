# Addendum — quality-gate follow-ups closed (2026-07-16)

The three quality-gate follow-up beads filed by `04-resolution.md` on
2026-07-10 (`determinism-hypervisor-mmra`, `-lynb`, `-jyp4`) were all
verify-and-closed on 2026-07-16 with **no code change**: commits `dd49ebf`
("Restore hypervisor CI compatibility") and `2bca5d8` ("Fix hypervisor strict
CI checks"), both landed 2026-07-11 — one day after filing — had already fixed
every cited instance.

Evidence (gate host `infra-control`, HEAD `b4358a7`, toolchain stable
2026-07-14: rustc 1.97.1 / clippy 0.1.97 / rustfmt 1.9.0):

- `cargo test --workspace --all-targets`: exit 0 — 67 suites, 771 passed,
  0 failed, 32 ignored (KVM lab-lane `--ignored` tests). Covers `mmra`
  (`ops.rs:181` carries `baseline: None` since `dd49ebf`).
- `cargo clippy --workspace --all-targets -- -D warnings`: exit 0. Covers
  `lynb` (the three `unnecessary_lazy_evaluations` at
  `m9_handoff.rs:1392-1406` fixed by `2bca5d8`; no `unwrap_or_else` remains
  in the file).
- CI-shaped `cargo fmt --check` (per-member `--package` list, not `--all`):
  exit 0. Covers `jyp4` (`runctl.rs` / `rss_regression.rs` reformatted in
  `dd49ebf`).
- Tier-1 corroboration: CI push run 29472974677 green at HEAD.

No execution-path file was touched, so the determinism-suite rerun obligation
is vacuous. Full plan record:
`.agents/plans/quality-gate-closeout-tails/00-overview.md`.
