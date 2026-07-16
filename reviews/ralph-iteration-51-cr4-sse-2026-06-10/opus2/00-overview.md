# Review: iteration-51 CR4 OSFXSR / SSE for compiled guests (2nd reviewer)

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** ralph/iteration-51-cr4-sse
- **Bead:** ttk
- **Verdict:** APPROVE

## Scope

This iteration enables guest SSE2 for compiled (Rust/C x86_64-ABI) guests by setting
`CR4.OSFXSR | CR4.OSXMMEXCPT` (plus the pre-existing PAE) at long-mode entry, while
*deliberately keeping `CR4.OSXSAVE` OFF*. To keep CPUID consistent with the absent
XSAVE/AVX surface, the §7.2 mask is expanded: leaf 1 ECX (FMA, XSAVE, OSXSAVE, AVX,
F16C), leaf 7 EBX (AVX2 + the AVX-512 F/DQ/IFMA/PF/ER/CD/BW/VL group), and leaf 0xD
(all subleaves zeroed). A new `sse_probe` nanokernel guest + a live `dh-cli` test
prove OSFXSR actually takes; the `cpuid-diff` artifact and ARCH §2.3 are updated.

## Bottom line

The design is coherent and the determinism reasoning is correct: with OSXSAVE off, the
guest-visible FP state is exactly the x87+SSE set that `KVM_GET_FPU` already captures
into the §8.1 hash blob (xmm[] + mxcsr are hashed — verified in `hash.rs:247-260`), and
the CPUID mask now removes every XSAVE/AVX feature bit so a compiled guest's
feature-detection won't reach for an instruction that would `#UD`. I verified by
execution:

- `sse_probe_proves_osfxsr` PASSES (serial `V`).
- `counting_semantics` still pins **997** with OSFXSR on — confirming the CR4 write is
  host-side/pre-entry and injects zero guest instructions.
- The cpuid mask tests, the **full workspace** test suite, and **clippy on both
  x86_64 and aarch64** are all clean; tree clean afterward.
- The masked-table hash `f19610e1…` is **stable across 5 live runs** and matches the
  committed artifact.

No Critical or Important defects that block merge. Two Important-class observations are
documented (a test-coverage gap on the new mask bits; a cpuid-diff artifact line-set
nondeterminism inherited from iteration-48), neither of which is a correctness bug in
the shipped code. See 01 for detail, 02 for suggestions, 04 for the action list.

## Re-baseline question (asked in the brief)

`ci/determinism-class.lock` does **NOT** pin the CPUID/masked-table hash. It pins only
host identity (`cpu_vendor/family/model_id/stepping/brand`, `microcode`, `host_kernel`).
The old masked hash `4dac1b7a…` appears **only in prior review docs** (iterations 30/48),
never in a live test, lock file, or source constant. Therefore this change does **not**
trip the documented re-baseline procedure. The committed cpuid-diff artifact is a
review aid, not a pinned gate.
