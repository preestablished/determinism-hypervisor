# Review Overview — ralph/iteration-36-determinism-regression

- **Reviewer:** Claude Opus
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-36-determinism-regression` vs `main`
- **Bead:** determinism-hypervisor-p9g (M3 acceptance gate — the 1e9 determinism regression)
- **Environment:** /dev/kvm present and rw; tests RUN live (including the 1e9 gate, timed and re-run for flakiness).

## Verdict

**APPROVE.** This is a clean, correct, minimal landing of the M3 acceptance gate. The
test matches the IMPLEMENTATION-PLAN M3 wording exactly ("run nanokernel 1e9
instructions twice from cold boot with fixed seed, final state hash equal"), runs in
~4s, and is not flaky across repeated live runs. No Critical or Important findings. A
small number of optional suggestions only.

## What the change does

Adds a new workspace member `tests/determinism` (lib stub + `tests/regression.rs`):

- `cold_run(budget)` performs a full cold boot — fresh `KvmSystem::open()`, fresh
  `create_slot_vm`, fresh ELF load of `nanokernel::landing_loop_elf()` with cmdline
  `125000000` (= 1e9 / 8 instrs-per-iter), a fresh per-thread `InstRetired` counter,
  a fresh `StateHashChain`, and a fixed-seed `MachineConfig` (all seed material `[7;32]`).
- Runs `run_segment(Until::IcountBudget(budget))`, asserts `StopReason::BudgetReached`
  and `out.boundary.icount == budget` exactly, and returns the 5-tuple
  `(icount, rip, rcx, vns, state_hash)`.
- `one_billion_instructions_twice_equal_final_hash` (the gate) compares two cold runs at
  budget = 1e9. `ten_million_twice_equal_final_hash` is the fast smoke variant.
- Both self-skip when `/dev/kvm` is not usable (probes via OpenOptions rw).
- `ci.yaml` arm lane adds `--exclude determinism-tests` (the crate links the x86-only
  `dh-vmm`, which the arm lane already excludes).

## Verification performed (this box)

| Check | Result |
|---|---|
| `cargo test -p determinism-tests` (debug) | 2 passed, ~3.99s test / 4.12s wall |
| `cargo test -p determinism-tests --release` | 2 passed, 3.51s test |
| 1e9 gate re-run x3 (flakiness) | 3/3 ok — not flaky |
| `cargo test --workspace` (kvm lane equivalent) | all green, 0 failures, 0 errors |
| `cargo clippy -p determinism-tests --tests` | exit 0, no warnings |
| `cargo fmt --all -- --check` | exit 0, clean |
| Cargo.toml parse — does `libc` land under dev-deps? | YES, correctly under `[dev-dependencies]` |
| Epoch grid claim (20 intermediate hashes) | confirmed: `DEFAULT_EPOCH_LEN = 50_000_000`, `EpochsOn` default → 1e9/50M = 20 links |
| Margin: budget lands before guest HLT | confirmed via `landing_loop.asm` (prologue + 1e9 loop + epilogue > 1e9) |

## Stats

- Files changed: 6 (`+157 / -1`)
- New crate: `tests/determinism` (Cargo.toml, src/lib.rs stub, tests/regression.rs 127 lines)
- Findings: **0 Critical, 0 Important, 3 Suggestions**
