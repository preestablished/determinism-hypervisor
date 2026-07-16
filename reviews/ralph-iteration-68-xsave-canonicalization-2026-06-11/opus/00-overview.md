# Review Overview — XSAVE Canonicalization

- **Branch:** `ralph/iteration-68-xsave-canonicalization` vs `main`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Bead:** ec4 (ARCH §8.1, risk R7) — pure-byte XSAVE canonicalization wired into the state-hash preimage.

## Summary

This change adds `crates/dh-vmm/src/xsave.rs`, a pure byte transform that zeroes every XSAVE component area whose `XSTATE_BV` bit is clear, and wires it into `hash.rs::canonical_vcpu_blob` so logically-equal vCPU state hashes equal (closing the R7 "init-optimization leaves clear-component bytes undefined" hole). I verified the SDM offset facts against the XSAVE chapter: the x87 component is exactly `[0,24) ∪ [32,160)` (FCW/FSW/FTW/rsvd/FOP/FIP/FDP then ST0–7), MXCSR/MXCSR_MASK occupy `[24,32)` and are governed by the SSE/AVX bits of RFBM rather than `XSTATE_BV[1]`, and the SSE component (XMM0–15) is exactly `[160,416)` — every offset in the module is correct. The extended-component table reads CPUID(0xD) with the correct register semantics (EAX=size, EBX=offset; subleaf-0 EAX|EDX<<32 = supported XCR0 mask). The MXCSR "do not zero" decision is sound for GET-side hash stability. The module is correctly ungated except for the CPUID helper, all 91 `dh-vmm --lib` tests pass (including the two live x86 KVM tests), and no pinned/golden hash constants depend on the vCPU blob preimage, so changing it breaks nothing. I found no Critical or Important issues — only minor robustness suggestions.

## Verdict

**APPROVE**

## Stats

- Files changed: 3 (`xsave.rs` new +265, `hash.rs` +17/-4, `lib.rs` +3)
- Tests: 91 passed / 0 failed (`cargo test -p dh-vmm --lib`), incl. `r7_*`, `bounds_are_loud`, `*_idempotent`, and live `live_xsave_canonicalizes_and_is_stable`
- Critical: 0 | Important: 0 | Suggestions: 4
