# Positive Notes

### P-1 — The R7 fix is genuinely live, and the test fixture proves it the right way

The headline risk (R7: undefined init-optimization bytes flipping the state hash)
is not hypothetical on this box. My live probe shows a *fresh* vCPU returns
`XSTATE_BV=0` with `FCW=0x037f` sitting in the clear x87 area — exactly the kind of
nonzero-but-logically-init garbage that would poison the hash. The
`r7_uncanonicalized_garbage_changes_the_hash_canonical_does_not` test
(`xsave.rs`) reproduces precisely this shape (different garbage in a clear
component → different un-canonicalized hash → identical canonical hash). The test
asserts the *fault first* (`assert_ne!` un-canonicalized) before asserting the
fix, which is the correct way to pin a determinism guard.

### P-2 — Component offsets/sizes validated against real silicon, not guessed

`extended_components_follow_the_table` hard-codes AVX = bit2 / offset 576 / size
256. This box's CPUID 0xD reports exactly that (and BNDREGS 960/64, BNDCSR
1024/64). The transform is table-driven from `host_component_layout()` rather than
hard-coding offsets in the hot path, so it tracks whatever silicon enumerates.
`crates/dh-vmm/src/xsave.rs`.

### P-3 — Correct scoping of the XCR0-vs-XSS distinction

`host_component_layout()` iterates only XCR0-supported bits
(`d0.eax | d0.edx << 32`), correctly excluding supervisor/XSS components (e.g. PT
at bit 8, which this host's leaf 0xD sub1 enumerates). `KVM_GET_XSAVE` returns the
*user* XSAVE area governed by `XSTATE_BV`; folding in XSS components would have
been a subtle bug. The author got the boundary right.

### P-4 — Loud, bounds-checked failure modes; no silent truncation

`TooShort` and `ComponentOutOfBounds` are returned (not panicked, not ignored),
and `canonical_vcpu_blob` maps them to `KvmError`. This is what makes the
otherwise-Important big-host truncation risk (see 01) fail *closed*: a component
beyond the 4096 region surfaces as an error rather than a silently incomplete
hash. `bounds_are_loud` covers both. `crates/dh-vmm/src/xsave.rs`.

### P-5 — Clean ungating and aarch64 portability

The pure transform is host-runnable and the CPUID/KVM helpers are `#[cfg(target_arch
= "x86_64")]`-gated with a clear module-doc rationale. Verified:
`cargo check -p dh-vmm --target aarch64-unknown-linux-gnu` compiles. The
`live_tests` module is `#[cfg(all(test, target_arch = "x86_64"))]` and gracefully
skips when `/dev/kvm` is unusable. `crates/dh-vmm/src/xsave.rs`, `lib.rs:12`.

### P-6 (bonus) — The MXCSR/reserved-bytes carve-outs are documented and correct

MXCSR/MXCSR_MASK `[24,32)` and legacy reserved `[416,512)` are explicitly NOT
zeroed, with a doc note explaining why (MXCSR is real state not governed by an
XSTATE_BV bit; reserved bytes are KVM-zero-filled). My live read confirms MXCSR =
`80 1f 00 00 ff ff 00 00` (0x1f80 default + 0xffff mask) and only one reserved
byte ([464]=0x1f) was nonzero — left untouched, as documented. The doc even
points at the R7 fault-injection shape as the mechanism to extend the rule if a
kernel ever leaks reserved-byte variance. `crates/dh-vmm/src/xsave.rs`.
