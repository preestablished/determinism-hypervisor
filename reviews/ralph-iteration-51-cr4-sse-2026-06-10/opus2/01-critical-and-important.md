# Critical and Important findings

## Critical

**None.** The shipped code is correct. All seven verification angles in the brief came
back clean or low-severity. Live execution (sse_probe, counting_semantics, full
workspace, both-arch clippy) passed.

---

## Important

### I1. The mask regression test does NOT assert any of the new bits are cleared

`crates/dh-vmm/src/cpuid.rs` test `mask_clears_the_documented_bits_live`
(lines 183-230) checks RDRAND/TSC_DEADLINE/x2APIC/MONITOR/PDCM (leaf 1),
RDSEED/WAITPKG (leaf 7), and the zeroed leaves 6/0xA/0x15/0x16/0x1A — but it does
**not** assert that any of the bits this iteration added are cleared:

- leaf 1 ECX: FMA (12), XSAVE (26), OSXSAVE (27), AVX (28), F16C (29)
- leaf 7 EBX: AVX2 (5), the AVX-512 group
- leaf 0xD: all four registers zeroed across subleaves

Consequence: the determinism guarantee these bits provide (a compiled guest must not
feature-detect XSAVE/AVX and then `#UD`) is not protected by a test. A future refactor
of `mask_in_place` could silently drop one of these masks and every test would still
pass. This is the weakest seam in the change: the *behavior* is correct today, but it
is **unguarded**. The hash test (`hash_is_order_independent_and_mask_sensitive_live`)
only proves masking changes *some* bit, not these bits.

Note: leaf 0xD is the one mask that is **load-bearing on this host** — leaf 1/7 AVX bits
were already absent or zero in `KVM_GET_SUPPORTED_CPUID` on the i5-8400 (Coffee Lake,
no AVX512; OSXSAVE already 0 in the supported table), so the only mask that actually
*clears live bits* here is leaf 0xD (18 nonzero lines in the artifact) and leaf 1
FMA/XSAVE/AVX/F16C. That makes the absence of a leaf-0xD assertion the most consequential
gap. **Recommended:** extend the existing live test with `(0xD, _) => assert all zero`
and a leaf-1/leaf-7 bit check using the new constants. Severity is Important (not
Critical) because it is a test-hardening gap, not a behavior bug — verified by my own
out-of-band check (`mask cleared 0x74209000` on leaf1 ECX = FMA|PDCM|x2APIC|XSAVE|AVX|
F16C|RDRAND; leaf7 EBX cleared 0x00040020 = RDSEED|AVX2; leaf 0xD all subleaves → 0).

### I2. cpuid-diff artifact: line set is run-to-run unstable (inherited from iter-48), hash is not

Regenerating live (`dh-cli cpuid-diff`) on this box produces a diff that does **not**
byte-match the committed `docs/ops/cpuid-diff-infra-control.txt`:

```
> leaf 0x00000001.0 ebx: 0x02100800 -> 0x00100800 (cleared 0x02000000)
> leaf 0x0000000b.0 edx: 0x00000002 -> 0x00000000 (cleared 0x00000002)
```

These two lines appear in *my* run but are absent from the committed artifact. This is
the exact host-placement nondeterminism root-caused in the iteration-48 review: leaf 1
EBX[31:24] is the initial APIC ID and leaf 0xB EDX is the executing LP's x2APIC ID —
both depend on which host logical processor the `KVM_GET_SUPPORTED_CPUID` ioctl happened
to run on. The mask **correctly zeroes both** (`e.ebx &= 0x00FF_FFFF` for leaf 1;
leaf 0xB fully zeroed), so:

- the **masked-table hash is deterministic** (`f19610e1…`, stable across 5 runs,
  matches the committed artifact) — the thing that actually feeds `MachineConfig` is
  fine; and
- only the *pre-mask supported-side display lines* in the diff vary, because the diff
  prints `supported -> masked` and the supported side is the unstable one.

So this is **not a correctness regression** and not introduced by this iteration — but
the committed artifact is a snapshot of one particular ioctl placement and will fail a
naive `diff` byte-match. If anything ever turns this artifact into a gate, it must
compare on the **hash line only** (or canonicalize the APIC/x2APIC fields out of the
supported side before diffing). Flagging as Important so the artifact's non-byte-stable
nature is on record; no code change required this iteration.

### I3. (forward-looking, low) Snapshot/restore FPU symmetry — no live asymmetry today

The brief asked whether the §8.3 restore path handles `kvm_fpu` symmetrically with the
§8.1 capture. Finding: **there is no vCPU/FPU restore path implemented yet.** The only
`snapshot`/`restore` machinery in the tree is the device-model `Device` trait (serial,
clock, entropy, blk, pad, detchannel) — none of it touches vCPU registers or FPU. The
full DHSNAP vCPU capture/restore (`KVM_GET_FPU`/`KVM_GET_XSAVE2`/`KVM_GET_XCRS` ↔
`KVM_SET_*`) is documented as M4 (ARCH §8.1/§8.3; `hash.rs:247` comment "XSAVE proper
is M4"). So there is **zero restore asymmetry to flag today** — capture (the §8.1 hash
blob) already includes xmm[] + mxcsr, which is the correct subset for an OSXSAVE-off
guest. The forward note for whoever builds M4: with OSXSAVE off the guest cannot write
XCR0, so the §8.3 `KVM_SET_XCRS`/`KVM_SET_XSAVE` steps must restore a *fixed* XCR0
(x87|SSE only) consistent with the masked CPUID — do not blindly replay a captured XCR0
that could carry stale AVX state-component bits. This is a design reminder, not a defect.
