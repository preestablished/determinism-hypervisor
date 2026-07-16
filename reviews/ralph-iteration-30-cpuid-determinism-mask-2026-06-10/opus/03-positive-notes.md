# Positive Notes

### P1 — Every cleared bit names its nondeterminism (matches §7.2's normative requirement)
§7.2 mandates "every cleared bit gets a comment naming the nondeterminism it closes."
The code honors this precisely: each `const L*_*` carries a tight justification
(MONITOR → "host C-state timing leaks", INVTSC → "guests must use pv-clock", RDRAND →
"nondeterministic by definition"). This is exactly the documentation contract the
architecture asks for, and it makes the mask auditable at a glance.

### P2 — Starts from KVM_GET_SUPPORTED_CPUID, never host CPUID
`masked_cpuid()` derives from `kvm.get_supported_cpuid()` and `mask_in_place` only ever
*clears* bits / removes leaves — it never sets a bit KVM didn't already support. This is
the correct, KVM-sanctioned base (host CPUID would advertise features KVM can't actually
virtualize). The module doc-comment calls this out explicitly.

### P3 — Unconditional `&= !mask` is future-proof
The mask clears MONITOR/TSC_DEADLINE/TM/ACPI even though they aren't advertised on this
particular Coffee Lake host. Because it's a clear-mask (not a conditional toggle), the
same code is correct on any future host that *does* advertise them — no host-specific
branching, no silent gaps.

### P4 — vPMU disabled before vCPU creation, with correct best-effort layering
`KVM_CAP_PMU_CAPABILITY`/`KVM_PMU_CAP_DISABLE` is enabled on the VM **before**
`create_vcpu(0)` — the only point where it takes effect. The `let _ =` best-effort is
the right call: on kernels lacking the cap, leaf 0xA is still zeroed by the CPUID mask,
so the guest cannot discover or program counters regardless. The comment correctly ties
this to the §3.1 pinned host INST_RETIRED counter that a vPMU must never contend with.
Defense in depth done properly.

### P5 — PV-leaf removal is the right mechanism and is deterministic
Removing `0x4000_0000..0x4000_0100` entirely (the `retain`) strips the `KVMKVMKVM`
signature leaf, so a probing guest (incl. a Linux bzImage) never detects KVM and never
enables kvmclock / async-PF / steal-time / PV-EOI — all host-scheduling-coupled. With
the signature gone, a guest reading 0x40000000 gets KVM's deterministic out-of-range
response for this host; verified live that the leaves are absent from the vCPU table.
This is precisely §7.2's "removed entirely" intent. The range end (0x100) is generously
beyond the two leaves KVM actually advertises (0x40000000–0x40000001).

### P6 — Hash canonicalization is order-independent and correctly excludes padding
`cpuid_table_hash` sorts by (function,index) before serializing, so KVM's internal entry
order — which is not machine behavior — cannot perturb the hash. The
`kvm_cpuid_entry2.padding: [u32;3]` field is correctly **not** hashed (it's reserved/
uninitialized and would inject nondeterminism). The order-independence is proven live by
`hash_is_order_independent_and_mask_sensitive_live` (reverses entries, asserts hash
unchanged). Including `flags` is acceptable for a *per-host* table hash (see I1 for the
cross-path consistency note).

### P7 — Live tests, not mocked — and they actually exercise the slot path
All three tests gate on `kvm_usable()` and run against real `/dev/kvm`. The third test
(`slot_vm_vcpu_carries_the_masked_table_live`) builds an actual slot VM and reads back
`get_cpuid2` from the vCPU, asserting the PV leaves are absent — i.e. it verifies the
*end-to-end* wiring through `create_slot_vm`, not just the mask function in isolation.
That is the right level to test at.

### P8 — set_cpuid2 placement is correct relative to the boot flow
CPUID is set in `create_slot_vm` immediately after `create_vcpu`, well before any
`KVM_RUN`. KVM requires `KVM_SET_CPUID2` before the first run; placing it at slot
construction (before SREGS, before MSR-filter-driven runs) satisfies that and keeps it
independent of the MSR filter application order. No ordering hazard.

### P9 — `cpuid-diff` is a genuinely useful M1 acceptance artifact
The BTreeMap-based diff renders per-register before/after with the exact cleared mask,
plus REMOVED leaves and the table hash. The live output is immediately legible for
acceptance review and makes the mask's effect self-documenting on any host it's run on.
`blake3` added as a proper `dh-vmm` workspace dep, consistent with ARCH §1's
external-crates list (blake3 is listed).
