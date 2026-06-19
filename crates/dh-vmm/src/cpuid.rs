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
const L1_ECX_FMA: u32 = 1 << 12; // AVX family: unusable with CR4.OSXSAVE off (bead ttk)
const L1_ECX_XSAVE: u32 = 1 << 26; // XSAVE: not enabled for guests
const L1_ECX_OSXSAVE: u32 = 1 << 27; // dynamic mirror of CR4.OSXSAVE; masked for stability
const L1_ECX_AVX: u32 = 1 << 28; // AVX: unusable with CR4.OSXSAVE off
const L1_ECX_F16C: u32 = 1 << 29; // AVX family

// Leaf 1 EDX
const L1_EDX_TSC: u32 = 1 << 4; // RDTSC: host-clocked raw cycle counter
const L1_EDX_TM: u32 = 1 << 29; // thermal monitor: package-thermal behavior
const L1_EDX_ACPI: u32 = 1 << 22; // thermal/throttle MSRs

// Leaf 7 (subleaf 0) EBX
const L7_EBX_RDSEED: u32 = 1 << 18; // hardware entropy: nondeterministic by definition
const L7_EBX_AVX2: u32 = 1 << 5; // unusable with CR4.OSXSAVE off (bead ttk)
/// AVX-512 F/DQ/IFMA/PF/ER/CD/BW/VL: same OSXSAVE-off reasoning.
const L7_EBX_AVX512_GROUP: u32 =
    (1 << 16) | (1 << 17) | (1 << 21) | (1 << 26) | (1 << 27) | (1 << 28) | (1 << 30) | (1 << 31);

// Leaf 7 (subleaf 0) ECX
const L7_ECX_WAITPKG: u32 = 1 << 5; // UMWAIT/TPAUSE: wall-clock waits in userspace

// Leaf 7 (subleaf 0) EDX
const L7_EDX_ARCH_CAPABILITIES: u32 = 1 << 29; // host vulnerability/mitigation MSR surface

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
                    | L1_ECX_RDRAND
                    // CR4.OSXSAVE is OFF (boot.rs, bead ttk): the
                    // XSAVE/AVX families are unusable, so they must not
                    // be advertised — compiled guests feature-detect.
                    | L1_ECX_FMA
                    | L1_ECX_XSAVE
                    | L1_ECX_OSXSAVE
                    | L1_ECX_AVX
                    | L1_ECX_F16C);
                e.edx &= !(L1_EDX_TSC | L1_EDX_TM | L1_EDX_ACPI);
                // EBX[31:24] is the initial APIC ID: which HOST LP the
                // ioctl ran on. It flipped the masked-table hash between
                // runs of the same binary (iteration-48 review, live).
                // Host placement, never machine identity: zeroed.
                e.ebx &= 0x00FF_FFFF;
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
                e.ebx &= !(L7_EBX_RDSEED | L7_EBX_AVX2 | L7_EBX_AVX512_GROUP);
                e.ecx &= !L7_ECX_WAITPKG;
                e.edx &= !L7_EDX_ARCH_CAPABILITIES;
            }
            (0xD, _) => {
                // XSAVE enumeration: host-specific state-area layout,
                // and OSXSAVE is off — zeroed like leaves 6/0xA/0xB.
                e.eax = 0;
                e.ebx = 0;
                e.ecx = 0;
                e.edx = 0;
            }
            (0x15, _) | (0x16, _) => {
                // TSC/crystal and processor frequency leaves: host-specific
                // timing constants. Guests own no wall clock — virtual time
                // is pv-clock's — and a guest calibrating from these would
                // bind its behavior to the recording host's silicon. Zeroed
                // so the table (and its hash) is frequency-blind.
                e.eax = 0;
                e.ebx = 0;
                e.ecx = 0;
                e.edx = 0;
            }
            (0x1A, _) => {
                // Hybrid core-type leaf: P-core/E-core identity is host
                // placement, not machine behavior. Zeroed.
                e.eax = 0;
                e.ebx = 0;
                e.ecx = 0;
                e.edx = 0;
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
            (0xB, _) | (0x1F, _) => {
                // Extended topology leaves: EDX is the executing LP's
                // x2APIC ID — host placement, run-to-run unstable
                // (iteration-48 review, live). x2APIC is masked out of
                // leaf 1 anyway, so the enumeration means nothing to the
                // guest: zeroed like leaves 6/0xA.
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
    crate::config::cpuid_leaves_hash(&to_leaves(cpuid))
}

/// The kvm table as sorted [`crate::config::CpuidLeaf`]s — the bridge
/// into MachineConfig's canonical representation (bead nq5: ONE
/// preimage; this is also how bead 8jx wires the masked table into the
/// config).
pub fn to_leaves(cpuid: &CpuId) -> Vec<crate::config::CpuidLeaf> {
    let mut entries: Vec<_> = cpuid.as_slice().to_vec();
    entries.sort_by_key(|e| (e.function, e.index));
    entries
        .iter()
        .map(|e| crate::config::CpuidLeaf {
            function: e.function,
            index: e.index,
            flags: e.flags,
            eax: e.eax,
            ebx: e.ebx,
            ecx: e.ecx,
            edx: e.edx,
        })
        .collect()
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
                    assert_eq!(e.edx & L1_EDX_TSC, 0, "TSC");
                    assert_eq!(
                        e.ecx
                            & (L1_ECX_FMA
                                | L1_ECX_XSAVE
                                | L1_ECX_OSXSAVE
                                | L1_ECX_AVX
                                | L1_ECX_F16C),
                        0,
                        "XSAVE/AVX family (CR4.OSXSAVE off)"
                    );
                    assert_eq!(e.ebx & 0xFF00_0000, 0, "initial APIC ID byte");
                }
                (6, _) => {
                    assert_eq!((e.eax, e.ebx, e.ecx, e.edx), (0, 0, 0, 0), "leaf 6");
                }
                (7, 0) => {
                    assert_eq!(e.ebx & L7_EBX_RDSEED, 0, "RDSEED");
                    assert_eq!(e.ecx & L7_ECX_WAITPKG, 0, "WAITPKG");
                    assert_eq!(e.edx & L7_EDX_ARCH_CAPABILITIES, 0, "ARCH_CAPABILITIES");
                    assert_eq!(
                        e.ebx & (L7_EBX_AVX2 | L7_EBX_AVX512_GROUP),
                        0,
                        "AVX2/AVX-512 (CR4.OSXSAVE off)"
                    );
                }
                (0x15, _) | (0x16, _) | (0x1A, _) => {
                    assert_eq!(
                        (e.eax, e.ebx, e.ecx, e.edx),
                        (0, 0, 0, 0),
                        "freq/hybrid leaves"
                    );
                }
                (0xA, _) => {
                    assert_eq!((e.eax, e.ebx, e.ecx, e.edx), (0, 0, 0, 0), "leaf 0xA");
                }
                (0xB, _) | (0x1F, _) | (0xD, _) => {
                    assert_eq!(
                        (e.eax, e.ebx, e.ecx, e.edx),
                        (0, 0, 0, 0),
                        "topology/XSAVE-enumeration leaves"
                    );
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
    fn linux_cpu_compat_rejects_host_time_entropy_and_kvmclock_live() {
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
            "KVM paravirt leaves, including kvmclock, must be removed"
        );
        for e in masked.as_slice() {
            match (e.function, e.index) {
                (1, _) => {
                    assert_eq!(e.ecx & L1_ECX_RDRAND, 0, "RDRAND");
                    assert_eq!(e.ecx & L1_ECX_TSC_DEADLINE, 0, "TSC_DEADLINE");
                    assert_eq!(e.ecx & L1_ECX_X2APIC, 0, "x2APIC");
                    assert_eq!(e.ecx & L1_ECX_PDCM, 0, "PDCM");
                    assert_eq!(e.edx & L1_EDX_TSC, 0, "TSC");
                }
                (7, 0) => {
                    assert_eq!(e.ebx & L7_EBX_RDSEED, 0, "RDSEED");
                    assert_eq!(e.edx & L7_EDX_ARCH_CAPABILITIES, 0, "ARCH_CAPABILITIES");
                }
                (0x8000_0001, _) => assert_eq!(e.edx & L8_1_EDX_RDTSCP, 0, "RDTSCP"),
                (0x8000_0007, _) => assert_eq!(e.edx & L8_7_EDX_INVTSC, 0, "INVTSC"),
                _ => {}
            }
        }
    }

    #[test]
    fn linux_cpu_compat_cpuid_table_is_in_state_hash_preimage_live() {
        if !kvm_available() {
            eprintln!("skipping: /dev/kvm not usable");
            return;
        }
        let kvm = Kvm::new().unwrap();
        let masked = masked_cpuid(&kvm).unwrap();
        let leaves = to_leaves(&masked);
        let Some(leaf7) = leaves.iter().position(|e| e.function == 7 && e.index == 0) else {
            eprintln!("skipping: KVM CPUID table has no leaf 7 subleaf 0");
            return;
        };

        let boot = crate::config::BootSpec::Elf {
            kernel_hash: [0x11; 32],
            cmdline: Vec::new(),
        };
        let mut config =
            crate::config::MachineConfig::new(2 * 1024 * 1024, [0x22; 32], boot.clone());
        config.cpuid_table = leaves.clone();
        let base_config_hash = config.config_hash().unwrap();
        let base_chain = crate::hash::StateHashChain::new(&base_config_hash, &[0x33; 32]);

        let mut changed_leaves = leaves;
        changed_leaves[leaf7].edx ^= L7_EDX_ARCH_CAPABILITIES;
        let mut changed = crate::config::MachineConfig::new(2 * 1024 * 1024, [0x22; 32], boot);
        changed.cpuid_table = changed_leaves;
        let changed_config_hash = changed.config_hash().unwrap();
        let changed_chain = crate::hash::StateHashChain::new(&changed_config_hash, &[0x33; 32]);

        assert_ne!(base_config_hash, changed_config_hash);
        assert_ne!(base_chain.value(), changed_chain.value());
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
