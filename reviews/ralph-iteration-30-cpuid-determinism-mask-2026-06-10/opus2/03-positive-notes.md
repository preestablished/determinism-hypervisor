# Positive notes

### P-1. Every cleared bit names the nondeterminism it closes — the bead's hardest requirement, met

The bead explicitly demands "Every cleared bit gets a code comment naming the nondeterminism it closes." The constant table delivers exactly that, tersely and accurately (e.g. `L1_ECX_MONITOR // MWAIT/MONITOR: host C-state timing leaks`, `L8_7_EDX_INVTSC // invariant-TSC advert: guests must use pv-clock`). This is the kind of self-documenting determinism code the project wants. The PV-leaf removal comment correctly enumerates *what* couples to host scheduling (kvmclock, async PF, steal time, PV EOI).

### P-2. Starts from `KVM_GET_SUPPORTED_CPUID`, never host CPUID — correct per ARCH §2.1 / §7.2

`masked_cpuid` builds from `kvm.get_supported_cpuid(...)`, matching the spec's explicit "never from host CPUID" instruction. This is the right source: it gives the KVM-emulatable subset, not raw silicon features the guest could not actually use deterministically.

### P-3. Order-independent canonical hash, verified live

`cpuid_table_hash` sorts by `(function, index)` before hashing, so KVM's arbitrary entry order does not leak into machine behavior — correctly called out in the doc comment ("KVM's own entry order is not part of machine behavior"). I confirmed live: reversing the entry vector and rehashing yields the identical digest, and the masked table is stable across repeated hashing.

### P-4. PV-leaf removal via `retain` is correct and the entry count actually shrinks

`cpuid.retain(|e| !(0x4000_0000..0x4000_0100).contains(&e.function))` removes both KVM leaves; the live `cpuid-diff` shows `supported entries: 42 -> masked entries: 40`, confirming `retain` shrinks `nent` rather than zeroing in place. The half-open range `0x4000_0000..0x4000_0100` correctly spans the KVM PV leaf block without over-reaching into `0x4000_0100+` (Hyper-V/other hypervisor ranges KVM does not emit here).

### P-5. PMU disabled before the vCPU exists — correct ordering

`KVM_PMU_CAP_DISABLE` is enabled on the VM **before** `create_vcpu`, which is required (the cap is a VM-level capability that must be set prior to vCPU creation to take effect). The ordering mirrors the existing dirty-ring "must be enabled before any vCPU" discipline. (Failure handling is the subject of I-3, but the placement is right.)

### P-6. Live test actually proves the property end-to-end

`slot_vm_vcpu_carries_the_masked_table_live` does not just test the mask function — it builds a real slot VM through `create_slot_vm` and reads back the vCPU's CPUID via `KVM_GET_CPUID2`, asserting no PV leaves survive. That closes the loop between "the mask function is correct" and "the wiring in `create_slot_vm` actually applies it." Verified passing here. RDTSC retention (L1 EDX[4] left set) is consistent with ARCH §4.1's deliberate design — the mask correctly does *not* over-reach there.
