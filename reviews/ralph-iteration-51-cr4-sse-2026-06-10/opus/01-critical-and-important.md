# Critical and Important findings

## CRITICAL — MXCSR escapes the §8.1 state hash (KVM_GET_FPU returns 0x0000)

**Files:** `crates/dh-vmm/src/hash.rs:182,260`; exposed by `crates/dh-vmm/src/boot.rs:241`

The state-hash blob serializes `fpu.mxcsr` (hash.rs:260) from `vcpu.get_fpu()`
(hash.rs:182). On this lab box (kernel **6.8.0-124-generic**) `KVM_GET_FPU` does not
report the guest's live MXCSR — it always returns `0x0000`.

**Live proof (executed):**

- Guest executes `ldmxcsr [0x00007F80]` (round-to-nearest, all exceptions masked is
  0x1F80; I used 0x7F80 = **round-toward-zero** to make it distinctive), then HLTs.
- `KVM_GET_FPU` → `mxcsr = 0x0000`
- `KVM_GET_XSAVE` → `mxcsr = 0x7F80`  ← the correct, live guest value

So the hash sees `0x0000` no matter what rounding mode / exception-mask state the
guest is in. SSE rounding mode changes FP results (round-to-nearest vs
round-toward-zero produce different bits for the same divide). **Two replay runs
that differ only in the guest's SSE rounding mode produce identical state hashes** —
a silent divergence in a product whose sole guarantee is bit-identical replay.

This iteration is what makes the bug reachable: before this change CR4 was PAE-only
(OSFXSR off), so any SSE instruction `#UD`'d and no guest could ever write MXCSR.
Setting `CR4.OSFXSR` turns MXCSR into live, guest-mutable, hash-relevant state — and
the hash captures it from the one ioctl that lies about it. The comments added in
this diff ("guest FP state is exactly what `KVM_GET_FPU` captures", boot.rs:238,
hash.rs module-doc) are therefore **factually wrong for MXCSR** and should not ship
as a determinism assurance.

**Required fix (one of):**

1. Capture FP state from `KVM_GET_XSAVE` (verified to report MXCSR correctly here)
   and extract the legacy FXSAVE-area MXCSR (byte offset 24) for the Phase-1 blob —
   keeping the existing x87/XMM serialization but sourcing MXCSR from XSAVE; **or**
2. Read MXCSR out of the XSAVE legacy region specifically and overwrite the
   GET_FPU `mxcsr` field before hashing; **or**
3. If M4's XSAVE canonicalization is imminent, at minimum add a **live regression
   test** that boots a guest which sets a non-default MXCSR and asserts the state
   hash changes — today such a test would FAIL (hash is MXCSR-blind), which is
   exactly the guard this iteration needs. Do not ship the "exactly what GET_FPU
   captures" wording until the capture path actually sees MXCSR.

A targeted live check to add to `hash.rs` tests: two `canonical_vcpu_blob` captures
after the guest sets MXCSR=0x1F80 vs MXCSR=0x7F80 must differ. They currently won't.

## IMPORTANT — boot/hash comments assert an untrue invariant

**Files:** `crates/dh-vmm/src/boot.rs:234-240`, `crates/dh-vmm/src/hash.rs:11-14,247`

The new boot.rs comment and the existing hash.rs module doc both assert the FP state
"is exactly the x87+SSE set that `KVM_GET_FPU` captures into the §8.1 hash blob —
nothing outside the hash." Given the Critical above, this is misleading: part of the
SSE control state (MXCSR) is *inside* the captured struct field but *outside* the
true machine state because GET_FPU zeroes it. Even after the Critical is fixed by
switching to XSAVE, the comment should not credit GET_FPU. Tighten the wording to
name the capture source actually used and the MXCSR caveat.

## Note (not blocking) — OSXMMEXCPT is currently moot but is a latent triple-fault edge

**File:** `crates/dh-vmm/src/boot.rs:241`

`CR4.OSXMMEXCPT` routes unmasked SSE FP faults to `#XM`. With no guest IDT a `#XM`
triple-faults. Live-verified: a guest that does `ldmxcsr 0` (unmask all) then `0.0/0.0`
**triple-faults (Shutdown)**. This is currently harmless because the guest's *real*
boot MXCSR is `0x1F80` (all exceptions masked — confirmed live via in-guest
`stmxcsr` = bytes 80 1f), so an inexact `1.0/3.0` divide reaches HLT fine. So
OSXMMEXCPT is **moot today** and the choice is defensible (IEEE-correct for compiled
code). But the safety relies entirely on guests never unmasking. A compiled guest
that enables FP exceptions (legitimate C: `feenableexcept`) would triple-fault. Worth
a one-line note in ARCH §2.3 that OSXMMEXCPT + no-IDT means an unmasked SSE fault is
a hard Shutdown, so determinism guests must keep MXCSR exceptions masked.
