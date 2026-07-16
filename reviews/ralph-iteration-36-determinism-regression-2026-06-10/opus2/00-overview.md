# Review Overview — M3 1e9 Determinism Regression Gate

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-36-determinism-regression` vs `main`
- **Bead:** determinism-hypervisor-p9g (P0, IN_PROGRESS)
- **Scope reviewed:** `tests/determinism/{Cargo.toml,src/lib.rs,tests/regression.rs}`, root `Cargo.toml` workspace member add, `Cargo.lock`, `.github/workflows/ci.yaml` arm-exclude hunk, IMPLEMENTATION-PLAN M3 accept criteria.

## Verdict

**APPROVE.** The gate is real, deterministic, and correctly wired. I tortured it as instructed and it did not flinch: 5/5 standalone 1e9 runs passed, serial and parallel runs passed, and the full workspace passed twice. No Critical or Important findings. Only minor Suggestions (mostly cosmetic / drift-hygiene).

This is an independent assessment performed without reference to the first reviewer's notes.

## What this gate actually proves

`one_billion_instructions_twice_equal_final_hash` runs the nanokernel landing-loop guest to **exactly 1e9 retired instructions twice from cold boot** with identical seed material (`[7;32]`), and asserts the **full 5-tuple** `(icount, rip, rcx, vns, state_hash)` is bit-identical. With the default `DEFAULT_EPOCH_LEN = 50_000_000` and `HashEpochs::EpochsOn`, the chain folds **20 epoch hash links** (1e9 / 50e6) plus the final — so the assertion compares the *trajectory*, not just the endpoint. Confirmed against `crates/dh-vmm/src/config.rs:50,99,100` and the epoch-walk in `crates/dh-vmm/src/runctl.rs`.

This matches the bead scope verbatim — "nanokernel (landing-loop program scaled to 1e9) twice from cold boot with fixed seed; final state hash equal" — and IMPLEMENTATION-PLAN M3 accept bullet 3.

## Torture results (all on the lab box, /dev/kvm rw, nproc=2)

| Test campaign | Result |
|---|---|
| 1e9 test x5 sequential (timed) | **5/5 PASS**, 4.38s–5.38s test-internal each (each invocation = 2x 1e9 cold boots = 4e9 instrs) |
| both tests `--test-threads=1` (serial) | **2/2 PASS** (4.49s) |
| both tests default threads (parallel, 2 live PMU counters) | **2/2 PASS** (4.17s) |
| full `cargo test --workspace` x2 | **PASS x2**, zero failures, zero warnings (170 tests/run) |
| clean rebuild of crate | zero warnings |
| `ci.yaml` YAML parse | OK |

Zero flakes across every campaign.

## Stats

- Files added: 3 (`Cargo.toml`, `src/lib.rs`, `tests/regression.rs`) + workspace/lockfile membership.
- Findings: **0 Critical, 0 Important, 4 Suggestions, 0 blocking.**
- Test code: 127 lines, 2 tests (1e9 gate + 1e7 smoke).
