# Review — ralph iteration 50: counting_semantics acceptance + single-step MMIO-write trap fix

- **Reviewer:** Claude Opus
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-50-counting-semantics-test`
- **Diff:** `git diff main...HEAD` — 3 files, +394/-8
  - `crates/dh-vmm/src/boundary.rs` (engine fix: re-arm guest_debug after handled exits)
  - `tests/determinism/tests/counting_semantics.rs` (new acceptance + regression test)
  - `.agents/docs/determinism-hypervisor/ARCHITECTURE.md` (§3.1 HLT→measured set; §3.2 re-arm rule)

## Verdict: APPROVE

The engine fix is correct, minimal, and load-bearing. Its premise — that an MMIO-WRITE
exit eats the pending single-step trap on this kernel class while MMIO reads and PIO OUTs
keep it — was **independently reproduced** with a scratch raw-KVM probe (deleted after).
The regression test is a genuine guard: with the fix reverted it fails loudly with
`Overshoot { target: 20, counted: 1003 }`. The full determinism battery (regression 1e9
twice, timer_determinism, if0_deferral, landing_precision, m1_acceptance) and the entire
workspace (209 tests) pass unchanged with the re-arm in place — no blast-radius regression.

## What was executed

- `cargo test -p determinism-tests` — all green (counting_semantics, counting_smoke,
  if0_deferral 32s, landing_precision 67s, m1_acceptance, regression 4s, timer 102s).
- `cargo test -p dh-vmm` (73 unit + blk_fixture) — green; `cargo test -p dh-cli`
  (skid_gate, boot_hello) — green; `cargo test --workspace` — 209 passed, 0 failed.
- `counting_semantics` run 3x — bit-stable.
- **Independent trap-eating probe** (raw KVM_SET_GUEST_DEBUG armed once, never re-armed):
  - PIO OUT(S): trap survives → next Debug after **0 instr**.
  - MMIO READ (0xd0000008): trap survives → next Debug after **0 instr**.
  - MMIO WRITE (0xd0006008, 'M'): trap **EATEN** → free-ran **991 instr** to the next exit.
  - This is exactly the claimed hazard. Premise CONFIRMED on kernel 6.8.0-124, microcode 0xfa.
- **Fix-reverted regression run**: `landing_across_an_mmio_write_does_not_free_run` FAILED
  with `Overshoot { target: 20, counted: 1003 }`; restored → green. The guard bites.
- clippy `--workspace --all-targets` on x86_64 AND aarch64 (with the provided cross env) — 0 warnings.
- Working tree clean after all probes removed.

## Environment recorded (R2 per-class empirics)

- Kernel: `6.8.0-124-generic` (Ubuntu, x86_64)
- Microcode: `0xfa`
- The iteration's comments cite "kernel 6.8" but do not pin the microcode revision; see
  Suggestions — recording `microcode 0xfa` in the test/doc would harden the R2 class definition.

Counts: 0 Critical, 0 Important (blocking), 3 Suggestions, 0 NEEDS_DISCUSSION.
