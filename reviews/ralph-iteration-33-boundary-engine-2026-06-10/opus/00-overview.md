# Review: §3.2 boundary engine (iteration 33)

- **Reviewer:** Claude Opus
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-33-boundary-engine` vs `main`
- **Bead:** determinism-hypervisor-rng
- **Scope:** `crates/dh-vmm/src/boundary.rs` (new), wiring in `lib.rs` / `Cargo.toml`
- **Read for context (unchanged):** `crates/dh-vmm/src/run.rs`, `crates/dh-detclock/src/counter.rs`, `crates/dh-vmm/src/config.rs`, ARCHITECTURE.md §3.1/§3.2

## Verdict

**APPROVE.** This is correct, faithful to §3.2, and the riskiest-path logic
(the far→near transition, the EINTR-is-a-request contract, the REP rule, the
singlestep-drop-on-all-paths invariant) is implemented exactly as the doc and
the run.rs/counter.rs contracts require. The four live tests genuinely ran on
this box (KVM rw + paranoid=1) and passed first-run with zero variance in the
determinism test. No Critical or Important correctness defects found.

The findings are: one Important *integration gap* (the boundary `Margins`
struct is a second, unconnected source of truth from `MachineConfig`'s
`skid_margin`/`resync_slack`), and a handful of doc/test/forward-looking
suggestions. None block merge of this foundation slice; the gap should be a
tracked bead before run-control (§3.3) wires `land_at` to a real config.

## Stats

- Files reviewed: 1 new (`boundary.rs`, 335 lines) + 2 wiring edits
- Tests: **58 passed, 0 failed, 0 ignored** (`cargo test -p dh-vmm`)
  - 4 live boundary tests RAN (not skipped): `lands_exactly_via_pmi_then_step_live`,
    `lands_exactly_with_pure_single_step_live`, `landing_is_deterministic_across_boots_live`,
    `stale_target_is_a_loud_overshoot_live`
- `cargo clippy -p dh-vmm`: clean (no warnings)
- Findings: **0 Critical, 1 Important, 6 Suggestions, 9 Positive notes**

## Finding counts

| Severity   | Count |
|------------|-------|
| Critical   | 0     |
| Important  | 1     |
| Suggestion | 6     |
| Positive   | 9     |
