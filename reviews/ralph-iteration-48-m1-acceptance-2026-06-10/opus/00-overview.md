# M1 Acceptance Review — Overview

- **Reviewer:** Claude Opus
- **Date:** 2026-06-10
- **Branch:** ralph/iteration-48-m1-acceptance
- **Bead:** 40q (P0 — M1 acceptance)
- **Verdict:** REQUEST_CHANGES

## Scope reviewed

`git diff main...HEAD` (4 files, +343):
- `tests/determinism/tests/m1_acceptance.rs` (NEW, 324 lines) — the M1 end-to-end test
- `tests/determinism/Cargo.toml` + `Cargo.lock` — x86-gated dev-deps (blake3, detguest-host, dh-devices, dh-inputlog)
- `docs/ops/cpuid-diff-infra-control.txt` (NEW) — committed dh-cli cpuid-diff artifact

Plus the supporting surface the test accepts: `device_exercise.asm`, dh-devices `{bus,clock,pad,entropy,blk,detchannel,ctx}.rs`, dh-vmm `{runctl,boundary,hash,cpuid}.rs`, `tools/dh-cli/src/cpuid.rs`, and the vendored `detguest-wire/src/ports.rs`.

## What I executed (live, on the Intel lab box, /dev/kvm rw)

- M1 test: PASSED, 6 runs, zero variance. Instrumented: **exactly 5 DHILOG records, icount 739, 1 beacon** — bit-stable across runs.
- `dh-devices` + `dh-vmm` unit/integration suites: all pass (66/10/73/3).
- Full workspace `cargo test`: pass (no failures).
- clippy x86_64 (changed packages + all-targets): clean.
- clippy aarch64 (`determinism-tests` and full workspace, with the lab cross env): clean — the new x86-gated dev-deps do NOT break the arm check.
- **cpuid-diff artifact reproduction: FAILED.** The masked-table hash is NOT deterministic on this host — it flips between `4dac1b7a…` (the committed value) and `65be8075…` across runs of the same binary (5 of 6 runs produced the OTHER hash). Root-caused live (see 01).

## Headline

The test itself is excellent: real device surface, real KVM, run-twice compare, immutability proof, loud failure modes, live-passing and bit-stable. **But the committed cpuid-diff artifact exposed a genuine product-level determinism hole**: the §7.2-masked CPUID table — which feeds `MachineConfig`'s determinism class — leaks host-CPU-dependent values (initial APIC ID, x2APIC ID) and is therefore not the "one fixed, hashed CPUID table" the module promises. That is the one Critical; everything else is Important/Suggestion.

## Action item count

- Critical: 1
- Important: 2
- Suggestions: 5
