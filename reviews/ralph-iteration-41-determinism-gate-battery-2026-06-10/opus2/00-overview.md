# Iteration 41 — the Phase-1 determinism gate battery — Review (2nd reviewer)

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-41-determinism-gate-battery` vs `main`
- **Beads:** ksx (zero-divergence harness) + 0zh (timer-determinism) + 3t9 (if0 deferral)
- **Scope:** M3 acceptance battery — `dh-verify::gate`, `dh-cli gate`, the two
  live determinism tests, the `timer_guest.asm` `defer` mode, the shared rig.

## Verdict

**APPROVE.** The battery is correct, the arithmetic checks out under live
verification, and the mechanics reproduce exactly. No Critical findings. The
one finding that rises to **Important** is documentation, not behavior: the
`TimerArm::deadline_vns` doc comment says "segment-relative" but every caller
in this iteration (and the conversion math) treats it as **absolute
counter-space vns from a base of 0**. Today the two coincide because the vns
base is 0 for the whole boot and the clock is 1:1, so nothing is wrong — but
the doc-vs-usage gap becomes a live trap when M4 restore gives segments a
nonzero vns base. Clarify the contract before that lands.

Everything else is suggestions and positive notes.

## What I verified live (/dev/kvm rw, SCALED-DOWN run counts, reverted after)

| Check | Result |
|---|---|
| `cargo build -p dh-verify -p dh-cli` | clean |
| `cargo test -p dh-verify` (gate.rs units) | 6 passed |
| `cargo fmt --all --check` | clean (exit 0) |
| `cargo clippy --workspace --all-targets` | zero warnings |
| `if0_deferral` @ **5 runs** | PASS, 1.96 s |
| `timer_determinism` @ **5 runs** (×10 fires) | PASS, 4.84 s |
| `dh-cli gate --runs 5` | both sub-gates PASS, all fingerprints identical; timer delivered at icount=1234567 exactly |
| `cargo test -p dh-vmm -p dh-detclock -p dh-verify` | 73 / 2 / 6 passed |
| `cargo test -p dh-cli` | 4 + 1 passed |
| `regression::ten_million_twice` (light sibling) | PASS, 0.30 s |
| **Adjudication experiment**: `timer_determinism` with `budget=deadline+1000`, expect `count==FIRES` | PASS — ISR count becomes 10 (FIRES), list still identical |

All scaled edits were reverted; `git status` is clean and run counts are back
at 100. The two heavy tests were **excluded** from a full live run by design.

## Last-verified heavy timings (from checkpoint 195b916, NOT re-run here)

- `dh-cli gate` 100 runs each sub-gate (200 cold boots): **32 s**, zero divergence.
- `if0_deferral` 100 runs: **33 s**.
- `timer_determinism` 100 runs × 10 fires + `if0_deferral` together add **~130 s**
  to the kvm-intel lane.
- 1e9 regression (`one_billion_instructions_twice_equal_final_hash`): part of the
  M3 battery, ALL PASS per checkpoint.

My 5-run extrapolations (if0 ~39 s, timer ~97 s at 100 runs) are consistent
with those figures.

## Finding counts

- Critical: 0
- Important: 1 (TimerArm doc-vs-usage contract)
- Suggestions: 6
- Positive notes: 7
