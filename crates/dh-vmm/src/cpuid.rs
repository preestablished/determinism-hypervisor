//! CPUID determinism mask (ARCH §7.2): the guest sees one fixed, hashed
//! CPUID table — every cleared bit below names the nondeterminism it
//! closes. Start from KVM_GET_SUPPORTED_CPUID, never from host CPUID.
//!
//! The masked table is part of machine behavior: [`cpuid_table_hash`]
//! feeds the MachineConfig determinism class (config bead), and dh-cli's
//! `cpuid-diff` dumps supported-vs-masked for M1 acceptance review.

use kvm_bindings::{CpuId, KVM_MAX_CPUID_ENTRIES};
use kvm_ioctls::Kvm;

use crate::kvm::KvmError;

// Leaf 1 ECX
const L1_ECX_MONITOR: u32 = 1 << 3; // MWAIT/MONITOR: host C-state timing leaks
const L1_ECX_TSC_DEADLINE: u32 = 1 << 24; // TSC-deadline timer: host-clocked interrupts
const L1_ECX_X2APIC: u32 = 1 << 21; // x2APIC: we run with no in-kernel irqchip
const L1_ECX_PDCM: u32 = 1 << 15; // perf/debug capability MSRs: vPMU surface
const L1_ECX_RDRAND: u32 = 1 << 30; // hardware entropy: nondeterministic by definition

// Leaf 1 EDX
const L1_EDX_TM: u32 = 1 << 29; // thermal monitor: package-thermal behavior
const L1_EDX_ACPI: u32 = 1 << 22; // thermal/throttle MSRs

// Leaf 7 (subleaf 0) EBX
const L7_EBX_RDSEED: u32 = 1 << 18; // hardware entropy: nondeterministic by definition

// Leaf 0x80000001 EDX
const L8_1_EDX_RDTSCP: u32 = 1 << 27; // RDTSCP: raw TSC + IA32_TSC_AUX reads

// Leaf 0x80000007 EDX
const L8_7_EDX_INVTSC: u32 = 1 << 8; // invariant-TSC advert: guests must use pv-clock

/// Build the masked table from this host's KVM_GET_SUPPORTED_CPUID.
pub fn masked_cpuid(kvm: &Kvm) -> Result<CpuId, KvmError> {
    let mut cpuid = kvm
        .get_supported_cpuid(KVM_MAX_CPUID_ENTRIES)
        .map_err(|e| KvmError::Open(format!("KVM_GET_SUPPORTED_CPUID: {e}")))?;
    mask_in_place(&mut cpuid);
    Ok(cpuid)
}

/// Apply the §7.2 mask. Public for cpuid-diff and tests.
pub fn mask_in_place(cpuid: &mut CpuId) {
    // KVM paravirt leaves (0x4000_00xx) are REMOVED entirely: kvmclock,
    // async PF, steal time, PV EOI — every one couples guest time/behavior
    // to host scheduling.
    cpuid.retain(|e| !(0x4000_0000..0x4000_0100).contains(&e.function));

    for e in cpuid.as_mut_slice().iter_mut() {
        match (e.function, e.index) {
            (1, _) => {
                e.ecx &= !(L1_ECX_MONITOR
                    | L1_ECX_TSC_DEADLINE
                    | L1_ECX_X2APIC
                    | L1_ECX_PDCM
                    | L1_ECX_RDRAND);
                e.edx &= !(L1_EDX_TM | L1_EDX_ACPI);
            }
            (6, _) => {
                // Thermal & power management leaf: ARAT, turbo, HWP, the
                // lot — all of it reflects host thermal state. Zeroed.
                e.eax = 0;
                e.ebx = 0;
                e.ecx = 0;
                e.edx = 0;
            }
            (7, 0) => {
                e.ebx &= !L7_EBX_RDSEED;
            }
            (0xA, _) => {
                // Architectural PMU leaf: the in-guest vPMU is disabled
                // (KVM_PMU_CAP_DISABLE) and must never be advertised — it
                // would also contend with the host's pinned INST_RETIRED
                // counter (ARCH §3.1).
                e.eax = 0;
                e.ebx = 0;
                e.ecx = 0;
                e.edx = 0;
            }
            (0x8000_0001, _) => {
                e.edx &= !L8_1_EDX_RDTSCP;
            }
            (0x8000_0007, _) => {
                e.edx &= !L8_7_EDX_INVTSC;
            }
            _ => {}
        }
    }
}

/// Canonical hash of a CPUID table (MachineConfig determinism class):
/// entries sorted by (function, index), fields serialized LE. KVM's own
/// entry order is not part of machine behavior.
pub fn cpuid_table_hash(cpuid: &CpuId) -> [u8; 32] {
    let mut entries: Vec<_> = cpuid.as_slice().to_vec();
    entries.sort_by_key(|e| (e.function, e.index));
    let mut bytes = Vec::with_capacity(entries.len() * 24);
    for e in &entries {
        bytes.extend_from_slice(&e.function.to_le_bytes());
        bytes.extend_from_slice(&e.index.to_le_bytes());
        bytes.extend_from_slice(&e.flags.to_le_bytes());
        bytes.extend_from_slice(&e.eax.to_le_bytes());
        bytes.extend_from_slice(&e.ebx.to_le_bytes());
        bytes.extend_from_slice(&e.ecx.to_le_bytes());
        bytes.extend_from_slice(&e.edx.to_le_bytes());
    }
    *blake3::hash(&bytes).as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kvm_available() -> bool {
        crate::kvm::kvm_usable()
    }

    #[test]
    fn mask_clears_the_documented_bits_live() {
        if !kvm_available() {
            eprintln!("skipping: /dev/kvm not usable");
            return;
        }
        let kvm = Kvm::new().unwrap();
        let masked = masked_cpuid(&kvm).unwrap();

        assert!(
            !masked
                .as_slice()
                .iter()
                .any(|e| (0x4000_0000..0x4000_0100).contains(&e.function)),
            "KVM paravirt leaves must be removed"
        );
        for e in masked.as_slice() {
            match (e.function, e.index) {
                (1, _) => {
                    assert_eq!(e.ecx & L1_ECX_RDRAND, 0, "RDRAND");
                    assert_eq!(e.ecx & L1_ECX_TSC_DEADLINE, 0, "TSC_DEADLINE");
                    assert_eq!(e.ecx & L1_ECX_X2APIC, 0, "x2APIC");
                    assert_eq!(e.ecx & L1_ECX_MONITOR, 0, "MONITOR");
                    assert_eq!(e.ecx & L1_ECX_PDCM, 0, "PDCM");
                }
                (6, _) => {
                    assert_eq!((e.eax, e.ebx, e.ecx, e.edx), (0, 0, 0, 0), "leaf 6");
                }
                (7, 0) => assert_eq!(e.ebx & L7_EBX_RDSEED, 0, "RDSEED"),
                (0xA, _) => {
                    assert_eq!((e.eax, e.ebx, e.ecx, e.edx), (0, 0, 0, 0), "leaf 0xA");
                }
                (0x8000_0001, _) => assert_eq!(e.edx & L8_1_EDX_RDTSCP, 0, "RDTSCP"),
                (0x8000_0007, _) => assert_eq!(e.edx & L8_7_EDX_INVTSC, 0, "INVTSC"),
                _ => {}
            }
        }
    }

    #[test]
    fn hash_is_order_independent_and_mask_sensitive_live() {
        if !kvm_available() {
            eprintln!("skipping: /dev/kvm not usable");
            return;
        }
        let kvm = Kvm::new().unwrap();
        let supported = kvm.get_supported_cpuid(KVM_MAX_CPUID_ENTRIES).unwrap();
        let masked = masked_cpuid(&kvm).unwrap();
        // Masking changes the table (RDRAND exists on every lab-class host).
        assert_ne!(cpuid_table_hash(&supported), cpuid_table_hash(&masked));
        // Same table hashed twice is stable.
        assert_eq!(cpuid_table_hash(&masked), cpuid_table_hash(&masked));

        // Order independence: reverse the entries, hash must not move.
        let mut reversed_entries: Vec<_> = masked.as_slice().to_vec();
        reversed_entries.reverse();
        let reversed = CpuId::from_entries(&reversed_entries).unwrap();
        assert_eq!(cpuid_table_hash(&masked), cpuid_table_hash(&reversed));
    }

    #[test]
    fn slot_vm_vcpu_carries_the_masked_table_live() {
        if !kvm_available() {
            eprintln!("skipping: /dev/kvm not usable");
            return;
        }
        let sys = crate::kvm::KvmSystem::open().unwrap();
        let slot = sys.create_slot_vm(2 * 1024 * 1024).unwrap();
        let vcpu_cpuid = slot.vcpu.get_cpuid2(KVM_MAX_CPUID_ENTRIES).unwrap();
        assert!(
            !vcpu_cpuid
                .as_slice()
                .iter()
                .any(|e| (0x4000_0000..0x4000_0100).contains(&e.function)),
            "vCPU must run with the masked table (no PV leaves)"
        );
    }
}
