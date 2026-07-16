# Review 00 — Overview

- **Branch:** `ralph/iteration-69-xsave-allowlist-hotfix` (vs `main`)
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus (2nd reviewer)
- **Diff:** `/tmp/ralph69-diff.txt` — 1 file, `crates/dh-vmm/src/xsave.rs`, +158 / −25
- **Box:** lab box with `/dev/kvm`, kernel 6.8.0-124, host XCR0 mask `0x1f` (x87, SSE, AVX bit2, MPX BNDREGS bit3, BNDCSR bit4)

## What the hotfix does

Iteration 68 put a canonicalized XSAVE blob into the state-hash preimage (`crates/dh-vmm/src/hash.rs:277-280`) using a **subtractive** rule: "for each clear XSTATE_BV bit, zero that component's area." Under parallel-suite load the determinism gates flaked `DIVERGED`. Iteration 69 replaces the subtractive rule with an **allowlist** rebuild — `canon = zeros; copy only {MXCSR[24,32), kept component areas, header words}` — plus **init-state normalization**: a set bit whose area carries the architectural init pattern (x87 init pattern, or all-zero SSE/extended area) is rewritten to clear so both KVM encodings of logically-init state canonicalize identically.

## Verdict

**APPROVE.** The fix is correct, the allowlist shape is strictly safer than the subtractive rule, and the empirical evidence supports it. One caveat on the *stated* root-cause mechanism (see below), but it does not change the verdict — the fix is correct regardless of which of the two mechanisms actually drove the flake, and it provably closes both.

## Empirical results (this box, this review)

| Experiment | Result |
|---|---|
| Dual-encoding probe — 200k tight `GET_XSAVE` reads under 3 FPU-burn threads | XSTATE_BV **stable at `0b1`**, 1 distinct value. **Flip NOT reproduced.** |
| Dual-encoding probe — 32k reads over 4000 fresh VMs, run-interleaved, under 4 FPU-burn threads | XSTATE_BV **stable at `0b1`**, 1 distinct value. **Flip NOT reproduced.** |
| Stress: `if0_deferral` + `skid_gate` concurrent under CPU burn, 3 rounds | **6/6 pass, zero divergence.** if0_deferral ran its full 100-run `zero_divergence` each round. |
| xsave unit battery (`cargo test -p dh-vmm xsave`) | **10/10 pass** incl. live `live_xsave_canonicalizes_and_is_stable`. |
| Full workspace (`cargo test --workspace`) | **293 passed, 0 failed**, no panics, exit 0. |

## Key finding on root cause

I could **not** reproduce the claimed XSTATE_BV bit-flip ("preemption timing decides bit-clear vs bit-set") on this box, across two strong configurations totaling ~232k reads under heavy host-FPU contention. KVM here **consistently** reports init-state x87 as **bit-SET with the init pattern** (encoding B), never flipping to bit-clear.

The more defensible — and directly demonstrable — root cause of the iteration-68 flake is the **non-component-gap garbage** the subtractive rule never touched. With this host's real layout there is a 128-byte gap `[832, 960)` between AVX (bit2, `[576,832)`) and BNDREGS (bit3, `[960,1024)`), plus legacy-reserved `[416,512)`, header-reserved `[528,576)`, and the tail — all kernel-buffer bytes the old rule left untouched and which can vary run-to-run. The allowlist zeroes every one of them by construction. This is almost certainly what was flaking, and the fix nails it. The init-normalization is still correct and harmless belt-and-braces, but its stated mechanism is unproven here. See `02-suggestions.md` for a doc-precision note.

## Stats

- Critical findings: **0**
- Important findings: **0**
- Suggestions: **4**
- Positive notes: **6**
