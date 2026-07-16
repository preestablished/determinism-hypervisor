# Suggestions

## S1 — Set CR0.MP alongside NE for compiled guests (boot.rs:242)

CR0 is `0x8000_0021` = PG|NE|PE. `EM=0` (good, FXSAVE/SSE require it), `TS=0` (good),
but `MP` (bit 1) is **not** set. The Intel-recommended config for FP-using code is
`MP=1, NE=1, EM=0, TS=0`. With NE already set and no x87 emulation, MP=0 is benign
today (WAIT/FWAIT with TS=0 won't fault either way), but the canonical "compiled code
runs FP" CR0 sets MP. Since this iteration's stated motivation is future Rust/C
guests, recommend `cr0 = 0x8000_0023` (add MP). Low risk; matches real-CPU reset-ish
expectations and removes a future surprise. Not blocking.

## S2 — Add a live MXCSR-sensitivity test to the hash suite

Independent of how the Critical is fixed, the state-hash suite should gain a live
test that proves MXCSR (rounding mode) perturbs the hash, mirroring the existing
`final_link_sees_guest_ram_live` "one flipped byte changes the hash" pattern. This is
the regression guard that would have caught this iteration's hole. (Today it fails —
which is the point.)

## S3 — sse_probe could also assert the masked CPUID surface (deferred is fine)

The prompt floated having `sse_probe` CPUID and report whether OSXSAVE/AVX read as
masked. I confirmed the table is pinned at SET_CPUID2 time (the masked-table hash is
invariant and the diff shows OSXSAVE bit 27 cleared in leaf 1 ECX). KVM serves the
guest from the SET table, so the table pin is authoritative for a no-irqchip,
no-CR4.OSXSAVE guest. A guest-side CPUID assertion would be belt-and-suspenders but
is not necessary; skipping it is fine. Noted only.

## S4 — Document the GET_FPU/GET_XSAVE MXCSR discrepancy with bd remember

This GET_FPU-returns-0-mxcsr behavior on 6.8 is a sharp, non-obvious kernel fact that
will bite the M4 XSAVE codec author too. Worth a `bd remember` entry so the next
session doesn't re-derive it. (Adjacent to bead nq5's cpuid_table_hash preimage
concern — unrelated, not worsened by this diff.)
