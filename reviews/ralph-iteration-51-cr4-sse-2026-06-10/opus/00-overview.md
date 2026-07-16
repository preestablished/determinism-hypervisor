# Review: iteration-51 CR4 SSE enablement (bead ttk)

- **Reviewer:** Claude Opus
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-51-cr4-sse`
- **Diff:** `git diff main...HEAD`
- **Verdict:** **REQUEST_CHANGES**

## Summary

This iteration sets `CR4 = PAE | OSFXSR | OSXMMEXCPT` so compiled (Rust/C) guests'
baseline SSE2 works, expands the §7.2 CPUID mask to clear the XSAVE/AVX surface
(leaf 1 ECX FMA/XSAVE/OSXSAVE/AVX/F16C; leaf 7 EBX AVX2 + AVX-512 group; leaf 0xD
zeroed), adds an `sse_probe` guest + live boot test, and regenerates the cpuid-diff
artifact (masked hash `f19610e1…`).

The CPUID mask, the artifact, the `sse_probe` guest, and the whole determinism
battery are **correct and verified live** (see 03-positive-notes). XMM and x87
register state are confirmed inside the §8.1 state-hash blob.

## Why REQUEST_CHANGES

Enabling SSE makes **MXCSR** a live, guest-mutable, hash-relevant register for the
first time. The §8.1 state-hash captures MXCSR via `KVM_GET_FPU` — but on this
host's 6.8 kernel **`KVM_GET_FPU` returns `mxcsr = 0x0000` regardless of the
guest's actual MXCSR** (live-proven: guest set `0x7F80`, GET_FPU reported `0x0000`,
`KVM_GET_XSAVE` reported the correct `0x7F80`). The guest's SSE **rounding mode and
exception masks therefore escape the state hash**: two replay runs differing only in
SSE rounding mode (which changes FP results) hash identically — an undetected
divergence in the product whose entire value is bit-identical replay. The module
comment in `boot.rs`/`hash.rs` claiming "guest FP state is exactly what
`KVM_GET_FPU` captures" is **false for MXCSR**.

This was latent before (SSE was off, no guest could touch MXCSR); this iteration is
what exposes it. It is a Critical determinism hole gated to land alongside the SSE
enablement, not a pre-existing bug to wave through.

XMM (16 regs) and x87 (`fpr`/`fcw`/`fsw`/`ftwx`) **are** captured correctly by
`KVM_GET_FPU` on this kernel (live-verified) — only MXCSR is broken on the GET_FPU
path.

## Confirmation requested in the prompt

- **XMM in the hash blob?** YES — `hash.rs:257-259` serializes all 16 `fpu.xmm`, and
  GET_FPU captures them faithfully (live: xmm3 round-trip exact).
- **MXCSR in the hash blob?** Serialized at `hash.rs:260`, **but the captured value
  is wrong** (always 0x0000 via GET_FPU) — the rounding mode does NOT reach the hash.
