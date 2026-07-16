# Iteration 47 — Doc Reconciliation — Adversarial Review (Overview)

- **Reviewer:** Claude Opus
- **Date:** 2026-06-10
- **Branch:** ralph/iteration-47-doc-reconciliation
- **Scope:** Documentation + comment reconciliation (beads 0sc, 20g, 5l7) — but docs are normative here, so claims were verified against code and tests.
- **Verdict:** REQUEST_CHANGES

## What this iteration does

1. ARCH §3.1: replaces the "CPUID/HLT/MMIO-exiting instructions retire exactly once on the completing resume" rule with the measured rule — VM-exiting instructions retire **zero** guest instructions under `exclude_host=1` (KVM completes them host-side via RIP skip). Backed by iteration-45 empirics (`COUNTING_DELTA_AT_OUT_EXITS = 997`).
2. ARCH §6.2: TIMER_DEADLINE documented as **absolute guest vns**, "run control subtracts its segment base internally," mirroring §6.4 `at_frame`.
3. guest-sdk ARCH layout table: ring W data `0x1E0000` → `0x100000` (1,048,576 B), plus a `0x120000 reserved` row.
4. Comment updates in `counting.asm`, `device_exercise.asm`, `lib.rs`.

## Verification performed

- `cargo test --workspace` (KVM present): **all pass**, including `counting_smoke::marker_window_is_exactly_the_region_minus_its_exiting_instructions` (997, bit-stable across two cold boots), `channel_interop`, `timer_determinism`, `if0_deferral`.
- `cargo clippy --workspace --all-targets`: **clean** (no warnings/errors).
- Working tree clean.
- Cross-checked each claim against: `tests/nanokernel/src/lib.rs` (COUNTING_* constants), `tests/nanokernel/asm/counting.asm`, `tests/determinism/tests/counting_smoke.rs`, ARCH §3.2 boundary pseudocode, `crates/dh-devices/src/clock.rs`, `crates/dh-vmm/src/runctl.rs`, `crates/dh-vmm/src/vt.rs`, `tools/dh-cli/src/gate.rs`, sibling `../guest-sdk/crates/detguest-wire/src/header.rs`, and bead `gfb` notes.

## Headline result

The three core reconciliations are **substantively correct and test-backed** — this is good, careful work. But three problems block a clean approval:

- **§3.1 overstates HLT** as "MEASURED" when bead `gfb` explicitly records HLT retirement as **not yet measured** (the smoke ends at HLT without bracketing it). The other three constructs (CPUID, MMIO r/w) are genuinely measured; PIO OUT is measured at the window edges. HLT is not. (Critical — this is exactly the "wrong spec sentence" failure mode the iteration exists to prevent.)
- **`counting.asm` is internally contradictory**: the CPUID line was fixed to "retires ZERO," but the two MMIO lines (and the file header was fixed, yet) lines 22/24 still say MMIO "retires once on the completing resume." A doc-reconciliation diff left the exact stale claim it set out to kill. (Important.)
- **§6.2's "run control subtracts its segment base internally" does not match the merged code.** `runctl::timer_to_injection` is origin-0 and subtracts no base; the base subtraction is a documented future *caller* responsibility (M4), and `clock.rs` and `runctl.rs` already disagree about who does it. The new vendored sentence picks the wrong one. No live bug today (vns_base == 0), but it is a normatively false mechanism description. (Important.)

Plus an out-of-scope but now-known-false vendored claim: **IMPLEMENTATION-PLAN.md M2 accept says "counter delta exactly 1,000"** over a sequence including CPUID/MMIO exits — the merged §3.1 makes that 997 (the smoke asserts 997). Flagged as Important since the iteration's stated goal is reconciling exactly this empiric.

See 01-critical-and-important.md for detail; 04-action-items.md for the checklist.
