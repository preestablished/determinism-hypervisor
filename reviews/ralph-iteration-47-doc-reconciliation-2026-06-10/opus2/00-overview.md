# Review — ralph iteration 47 (doc reconciliation cluster)

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** ralph/iteration-47-doc-reconciliation
- **Scope:** documentation-only — 3 vendored-doc reconciliations (beads 0sc, 20g, 5l7) + stale-comment updates
- **Verdict:** **REQUEST_CHANGES**

## What I verified by running, not just reading

- `cargo build --workspace` + forced nasm rebuild of both edited `.asm` files (`touch` then
  `cargo build -p nanokernel`) — counting.asm and device_exercise.asm still assemble; ELFs rebuild clean.
- `cargo clippy --workspace` — clean, zero warnings.
- `cargo test --workspace` — all suites green, including the live KVM tests:
  `counting_smoke` (1 passed), `channel_interop` (1 passed), `timer_determinism`
  (`delivered_icount_lists_identical_across_100_runs`, 100 runs zero-divergence, 119s),
  nanokernel lib (7 passed), dh-devices clock tests (66 passed), dh-vmm runctl/timer (73 passed).
- Cross-checked the layout table row-by-row against the authoritative
  `../guest-sdk/crates/detguest-wire/src/header.rs` constants and its compile-time
  layout-invariant `const _` block.
- Read all three §6.2 timer surfaces: `dh-devices/src/clock.rs`, `dh-vmm/src/runctl.rs`,
  `tools/dh-cli/src/gate.rs`, plus `tests/determinism/tests/timer_determinism.rs`.

## Bottom line

The three substantive doc changes are **correct in direction and well-grounded**: the §3.1
"retire zero" rewrite matches the measured empirics and the COUNTING_DELTA constant; the §6.2
absolute-vns annotation matches `clock.rs`; the layout-table W→0x100000 + reserved row matches
`RING_W_SIZE`/`OFF_RESERVED_TAIL` exactly. The verification all passes.

But I am requesting changes for two reasons, both in the "easy-to-overlook normative wording"
category this review was asked to focus on:

1. **(Important) The new §3.1 sentence overclaims its evidence scope.** It asserts
   `CPUID, HLT, PIO, MMIO` all "retire zero — MEASURED on the kvm-intel class." Only OUT, CPUID,
   MMIO-read and MMIO-write were actually isolated by the counting region. **HLT is explicitly
   flagged unmeasured by bead gfb** ("the smoke ends at HLT without bracketing it — measure it
   here before relying on it"), and **PIO-IN was never isolated** (the counting region contains no
   `IN`; reconciliation bead 0sc lists only "PIO OUT, CPUID, MMIO access"). The word "MEASURED"
   now covers two classes that were reasoned-about, not measured. See 01, finding I1.

2. **(Important) The iteration's own stated goal — "update stale comments" — is only half done.**
   `counting.asm` lines 21–24 STILL describe the in-region MMIO read and MMIO write as
   "exits, retires **once** on the completing resume," directly contradicting line 13 ("retire
   ZERO") and line 20 ("CPUID … retires ZERO (measured)") in the *same file*, and contradicting
   the new ARCH §3.1 rule the iteration just landed. These are the exact `XI` (exiting-macro)
   instructions that the new rule says retire zero. See 01, finding I2.

Neither is a correctness bug in shipped behavior (tests prove determinism is unaffected), but in a
repo where "docs are normative and a wrong sentence breeds implementation bugs," an overclaimed
"MEASURED" and a self-contradicting source comment are exactly what a future implementer trips on.

## Action-item counts

- Critical: 0
- Important: 2
- Suggestions: 3
