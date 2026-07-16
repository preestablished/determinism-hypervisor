# Critical & Important findings

## Critical

**None.** The code that exists is correct and live-verified. No bit is mapped to the wrong leaf/register; the hash is order-independent; the slot vCPU demonstrably carries the masked table.

---

## Important

### I-1. WAITPKG / UMWAIT (leaf 7.0 ECX[5]) is a wall-clock-bounded wait that the mask does not clear

`mask_in_place` touches leaf 7 subleaf 0 EBX only (`e.ebx &= !L7_EBX_RDSEED`). It never touches leaf 7.0 **ECX**, where **WAITPKG** lives (ECX[5]). WAITPKG enables `UMWAIT`/`TPAUSE`/`UMONITOR`: a userspace instruction that **blocks the core until a TSC deadline**. That is precisely the MWAIT/MONITOR class of "host C-state / wall-clock timing leak" that §7.2 closes with `L1_ECX_MONITOR`, and it is a far more direct nondeterminism source than MONITOR because `TPAUSE` takes an explicit TSC deadline in EDX:EAX and the wake time depends on host timing.

- On **this** lab box (Coffee Lake i5-8400) leaf 7.0 ECX = `0x00000004` — bit5 is **clear** (no WAITPKG; the `0x4` is UMIP), so there is no leak here and the omission is invisible. But the mask is meant to be host-agnostic, and any Tremont/Tiger-Lake-or-later host has WAITPKG. On those hosts `UMWAIT` would be advertised straight through.
- Recommendation: clear leaf 7.0 ECX[5] (WAITPKG) in the same `(7, 0)` arm, with a comment naming it as the MWAIT-class wall-clock wait. While in that arm, also consider clearing the remaining entropy/RNG-adjacent and timing bits that §7.2's spirit covers (see I-2).

Why Important and not a Suggestion: §7.2 explicitly names "MWAIT/MONITOR" as a closed source, and WAITPKG is the modern, deadline-bearing form of exactly that. A determinism mask that closes MONITOR but leaves TPAUSE open on capable hardware has a real, exploitable hole the moment the fleet includes a newer SKU.

### I-2. Host-specific frequency leaves 0x15 / 0x16 flow into the masked table and the hash unmasked — cross-host determinism-class hazard

Leaf **0x15** (TSC / core-crystal clock ratio) and leaf **0x16** (processor base/max/bus frequency, in MHz) carry **host-model-specific** values and are not touched by the mask. On this box KVM happens to return them all-zero (`leaf 0x15.0` and `0x16.0` are `eax=ebx=ecx=edx=0` in the raw dump), so there is no leak *here* and the hash is incidentally clean. But:

1. On hosts/kernels where KVM **does** populate 0x16 (many do — it is a simple passthrough of the host's leaf 0x16), the base/max frequency in MHz becomes part of the guest-visible table **and part of `cpuid_table_hash`**. Two otherwise-identical determinism-class hosts that differ only in SKU frequency would then produce **different machine-config hashes** and different guest-visible CPUID — a determinism-class identity split that §7.2's "fleet's lowest common denominator" wording is meant to prevent.
2. Read §7.2's determinism *class* carefully: AVX512 masking "to the fleet's lowest common denominator" is called out as a "determinism-class concern, not a correctness one." Frequency leaves 0x15/0x16 are the same kind of concern — they should be **fixed to a canonical value** (or zeroed) so the determinism class is host-SKU-independent, not left as host passthrough.

The current code is correct for "per-host reproducibility" (the hash is per-host and the host kernel/microcode are pinned per §7.4), but it does **not** give cross-host determinism-class identity for these leaves. If the design intends the masked table + hash to be a fleet-stable identity (the bead says "Masked table goes into MachineConfig and is hashed (determinism class)"), 0x15/0x16 should be canonicalized. At minimum, document the decision: "frequency leaves are host-passthrough; cross-host determinism class is established by the pinned-kernel tuple in §7.4, not by CPUID."

Recommendation: either zero leaves 0x15 and 0x16 (cheapest, matches the "no host-specific values in the table" intent) or explicitly document them as an accepted determinism-class input. Same logic applies to **leaf 0x1A** (hybrid/core-type) — absent on this non-hybrid box, but on a hybrid host it would leak P/E core type, which matters for a snapshot **restored on a different host**.

### I-3. `KVM_PMU_CAP_DISABLE` is best-effort (`let _ =`) while every sibling cap in `create_slot_vm` hard-fails — inconsistent with the §2.1 hard-fail philosophy and the determinism stakes

In `create_slot_vm`, the dirty-ring enable and the `USER_SPACE_MSR` enable both propagate failure with `?` (→ `KvmError::VmCreate`). The new PMU-disable enable is swallowed:

```rust
let _ = vm.enable_cap(&pmu_cap);
```

The inline comment justifies this as "best-effort on kernels without the cap (the CPUID mask still hides leaf 0xA either way)." Two problems:

1. **The fallback claim is only half-true.** Hiding leaf 0xA stops the guest from *discovering* the vPMU via CPUID, but `KVM_PMU_CAP_DISABLE` is what actually *disables* the in-kernel vPMU emulation and — per the comment's own §3.1 rationale — what prevents the guest vPMU from **contending with the host's pinned INST_RETIRED counter**. CPUID masking does not close that contention; only the cap does. So on a kernel where the cap is absent or the call fails, you silently lose the property the comment says matters, with no signal.
2. **Inconsistent with the established pattern.** `KVM_CAP_PMU_CAPABILITY` exists since kernel 5.18; this is a 6.8 box (and the project pins kernel versions, §7.4). There is no realistic "kernel without the cap" in the supported fleet, so the defensive `let _ =` buys nothing while diverging from the hard-fail discipline that REQUIRED_CAPS/REQUIRED_RAW_CAPS enforce everywhere else.

I verified on this kernel the cap **is** present and the slot-VM test (which exercises `create_slot_vm`) passes, so the call is succeeding here — but silently.

Recommendation (pick one):
- **Preferred:** add `KVM_CAP_PMU_CAPABILITY` to `REQUIRED_RAW_CAPS` and `?`-propagate the `enable_cap` in `create_slot_vm` (`KvmError::VmCreate(format!("PMU disable: {e}"))`). This matches the determinism-first, fail-loud stance the rest of the module takes.
- **If best-effort is a deliberate policy:** keep `let _ =` but emit a `tracing::warn!` on `Err` so a missing/failed vPMU-disable is visible, and add a sentence to §7.2 documenting that the cap is best-effort and the residual (guest-vPMU/host-counter contention) is monitored by verification mode. A determinism property silently degrading to "off" is exactly what verification mode exists to catch, but you still want the operational signal.
