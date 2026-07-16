# Critical & Important Findings

**None.** No Critical and no Important findings.

This section documents the rigor checks I ran for each high-risk dimension the
review brief flagged, and why each came back clean. These are not findings —
they are the audit trail justifying the APPROVE verdict.

---

## §8.3 restore ordering — VERIFIED CORRECT

ARCH §8.3 states the normative constraints (lines 681–683 of
`.agents/docs/determinism-hypervisor/ARCHITECTURE.md`):

> "SREGS2 before REGS before VCPU_EVENTS; XSAVE before XCRS is **wrong**, set
> XCRS then XSAVE; set MSRs last, IA32_TSC ← vns"

Implemented order in `restore()` (`vcpu_state.rs:172–208`):

```
SREGS(172) → REGS(174) → FPU(176) → XCRS(179) → XSAVE(187)
           → DEBUGREGS(189) → VCPU_EVENTS(191) → MSRs(197) → TSC_OFFSET(208)
```

Constraint-by-constraint:

- **SREGS before REGS before VCPU_EVENTS**: 172 < 174 < 191 ✓
- **XCRS before XSAVE**: 179 < 187 ✓ (the comment correctly explains the
  reverse would let XSETBV re-init enabled components after XRSTOR)
- **MSRs last** (of the SET_* group): 197 is after every other SET ✓
- **IA32_TSC ← vns**: programmed at 208, after MSRs ✓

## FPU-before-XSAVE — the unstated implementation decision is SOUND

The spec does not order FPU vs XSAVE. The code does FPU (176) **before** XSAVE
(187), with the comment "XSAVE is authoritative for the x87/SSE overlap." This
is the correct choice and the only safe one: `KVM_SET_FPU` writes the legacy
x87/SSE region (FCW/FSW/MXCSR/ST/XMM), which is exactly the area the XSAVE
legacy header also covers. If FPU were set *after* XSAVE, `SET_FPU` would
clobber the XSAVE-restored x87/SSE state with the separately-captured `kvm_fpu`
snapshot — a latent divergence whenever the two disagree. Doing FPU first means
the authoritative canonical XSAVE wins on overlap. **The code does FPU first —
correct.** (In practice the two are consistent because both were captured at the
same boundary, but ordering it safely costs nothing and removes the hazard.)

`DEBUGREGS` (189) and `VCPU_EVENTS` (191) land after XSAVE and before MSRs.
Neither is constrained by §8.3 beyond "VCPU_EVENTS after REGS" (satisfied), and
neither has a documented interaction with XSAVE/XCRS/MSR ordering. Safe.

## Unsafe struct byte-copy codec — PADDING AUDIT CLEAN (6/6 structs)

This was the highest-risk surface and the exact bug class iteration 69 fixed for
XSAVE. I dumped every struct from kvm-bindings 0.13.0
(`.../kvm-bindings-0.13.0/src/x86_64/bindings.rs`) and checked the bindgen
layout asserts. **No struct has compiler-inserted (implicit) padding** — every
gap is a *named* field (`padding`/`pad`/`reserved`) that:

1. `Default` (used by `read_struct`'s `T::default()` and the synthetic-state
   builders) initializes to zero, and
2. the kernel zero-fills on `KVM_GET_*` (GET ioctls write the full fixed struct
   from kernel memory; KVM zeroes reserved fields).

Per-struct verification:

| Struct | repr | Padding situation | Verdict |
|---|---|---|---|
| `kvm_regs` | `repr(C)` | 18×u64, size 144, all 8-aligned | no padding |
| `kvm_segment` | `repr(C)` | named `padding: u8` fills to size 24 | no implicit padding |
| `kvm_dtable` | `repr(C)` | named `padding: [u16;3]` fills to 16 | no implicit padding |
| `kvm_sregs` | `repr(C)` | 8×segment + 2×dtable + u64s, all 8-aligned | no padding |
| `kvm_fpu` | `repr(C)` | explicit `pad1: u8`, `pad2: u32`, size 416 | no implicit padding |
| `kvm_xcrs` | `repr(C)` | named `padding: [u64;16]` + `kvm_xcr.reserved` | no implicit padding |
| `kvm_vcpu_events` | `repr(C)` | size 64; `reserved[26]`@29→55, `exception_has_payload`@55, `exception_payload`@56 — gaps all named | no implicit padding |
| `kvm_debugregs` | `repr(C)` | all u64 arrays incl. `reserved[9]` | no padding |

The `kvm_vcpu_events` case is the one that *could* have had implicit padding
before the trailing `u64 exception_payload`; the bindgen asserts confirm
`exception_has_payload`@55 + `exception_payload`@56 with no gap (the preceding
`reserved[26]` byte array absorbs what would otherwise be alignment slack).
**Verified via the `offset_of!` / `size_of!` const asserts in the binding.**

Conclusion: the `struct_bytes` / `read_struct` byte-copy is deterministic and
hash-stable for all six structs. The `#[allow(unsafe_code)]` SAFETY comments
accurately describe POD with valid bit patterns.

## TSC restore — HONORS the decided mechanism, skew is intended

`restore()` computes `offset = vns.wrapping_sub(rdtsc())` and calls
`crate::tsc::set_tsc_offset` (the `KVM_VCPU_TSC_OFFSET` device attribute) —
exactly `docs/decisions/tsc-alignment.md`'s decision ("Restore computes `offset
= vns − host_tsc_at_resume` and issues one `KVM_SET_DEVICE_ATTR`"). The
per-entry `KVM_SET_MSRS{IA32_TSC}` path the decision explicitly forbids is *not*
wired in. The userspace-rdtsc → KVM-entry skew (guest TSC ≠ exactly vns at first
entry) is **expected and accepted** by the decision doc: "After resume the guest
TSC advances at the HOST rate while vns advances per the clock rational — the
drift is intended (§4 defense 4: guests must take time from pv-clock; the TSC is
merely monotonic)." No reopening of the decision; computation matches verbatim.

## MSR restore — list & ordering CORRECT, EFER double-set is benign

- `RESTORE_MSR_LIST` (`vcpu_state.rs:60–75`) is order-identical to
  `hash.rs::MSR_CAPTURE_LIST` and correctly **omits IA32_TSC** (written via the
  offset attribute, never carried as captured MSR data). Cross-checked field by
  field.
- `set_msrs` return value is checked against `st.msrs.len()`, naming the first
  unwritable index on mismatch (`197–204`) — matches the fail-loud posture of
  `hash.rs`.
- **EFER double-set**: EFER is written by `SET_SREGS` (172) *and* is the first
  entry of the MSR list (197). Because MSRs run last, the MSR-list EFER value
  wins. Both came from the same capture boundary (SREGS EFER and MSR EFER are
  the same register read twice), so the values agree and the double-set is a
  harmless idempotent write. No conflict.

## XSAVE2 fail-closed guard — placement CORRECT

The guard lives in `capture()` only (`112–119`): if `KVM_CAP_XSAVE2` reports a
required area > 4096 it errors rather than truncating. `restore()` does **not**
re-check the cap; it instead validates `st.xsave.len() == XSAVE_AREA_LEN`
(`164–169`) and `decode_section` rejects any other length (`302–306`). This is
the right split: capture is where host-size truncation could silently lose data;
restore trusts the codec-validated 4096-byte canonical blob. Correct.

## Section codec totality — VERIFIED

`decode_section` rejects: wrong version, truncation (every `get(..)` is checked,
`read_struct` does a `checked_add` bounds check), trailing bytes (`at !=
bytes.len()`), wrong XSAVE length, MSR count divergence, per-MSR index
divergence from the code-versioned list, and nonzero MSR `_pad`. The
`decode_rejects_malformed_sections` test exercises version/truncation/trailing/
MSR-mismatch. Round-trip purity and the live GET→SET→GET fixed-point are tested.
