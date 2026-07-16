# Iteration 41 Review — The M3 Determinism Gate Battery (ksx + 0zh + 3t9)

- **Reviewer:** Claude Opus
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-41-determinism-gate-battery` vs `main`
- **Beads:** determinism-hypervisor-ksx (Phase-1 gate harness), -0zh (timer determinism test), -3t9 (IF=0 deferral test) — a deliberate cluster: the M3 acceptance battery.

## Verdict

**APPROVE.** This is a clean, tightly-scoped cluster. All three deliverables meet their bead
text, the cluster boundary against the deferred work (40q device-bus arming, 8n7 1e9 CI
regression) is honest and correctly drawn, and the subtle determinism arithmetic
(budget==deadline merge, queued-vector ISR accounting) is self-consistent and empirically
proven by the 100-run identity. Live verification reproduced: unit tests green, `dh-cli gate
--runs 3` PASS with byte-identical hashes, both cluster tests PASS at reduced runs, clippy and
fmt clean.

Findings are advisory (one Important about CI reachability of the `dh-cli gate` command, the
rest Suggestions/nits). None block the merge.

## What was reviewed

- `crates/dh-verify/src/gate.rs` (new, 101 lines + unit tests) — the generic `zero_divergence`
  harness and `GateReport` artifact.
- `tools/dh-cli/src/gate.rs` (new, 94 lines) + `dh-cli gate [--runs N]` wiring in `main.rs`.
- `tests/determinism/tests/common/mod.rs` (new rig), `timer_determinism.rs` (0zh),
  `if0_deferral.rs` (3t9).
- `tests/nanokernel/asm/timer_guest.asm` `defer` mode hunk.
- Cross-read for correctness: `crates/dh-vmm/src/runctl.rs` (segment loop, agenda merge, timer
  delivery), `crates/dh-vmm/src/inject.rs` (queue-vs-execute semantics),
  `crates/dh-vmm/src/agenda.rs` (coincident-point merge).
- Normative sources: the three bead texts, the iteration-41 commit message, IMPLEMENTATION-PLAN
  M3 accept items, phase-1 doc "Exit gate" item 1.

## Verification performed (live, /dev/kvm rw)

| Check | Result |
|---|---|
| `cargo test -p dh-verify gate` (unit) | 4 passed, 0 failed |
| `cargo fmt --check` | clean (exit 0) |
| `cargo clippy -p dh-verify -p dh-cli --tests` | no warnings |
| `dh-cli gate --runs 3` (live, 6 cold boots) | both sub-gates PASS, identical hashes, timer delivered=1234567 |
| `timer_determinism` + `if0_deferral` at 5 runs (local patch, reverted) | both PASS (1.66s + 5.12s) |
| working tree after revert | clean |

## Stats

- Files added: 4 (`dh-verify/src/gate.rs`, `dh-cli/src/gate.rs`, two test files + `common/mod.rs`).
- Files modified: 6 (two `lib.rs`, `main.rs`, `timer_guest.asm`, two `Cargo.toml`/`Cargo.lock`).
- Net new lines: ~480 (incl. tests).
- Findings: **0 Critical, 1 Important, 6 Suggestions, several positive notes.**
