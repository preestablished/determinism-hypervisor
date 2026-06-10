//! `dh-cli cpuid-diff` (bead 8jx, M1 acceptance review): dump every
//! difference between this host's KVM_GET_SUPPORTED_CPUID and the §7.2
//! masked table the guests actually see, plus the masked-table hash that
//! feeds MachineConfig.

use std::collections::BTreeMap;

pub fn cpuid_diff() -> Result<String, String> {
    let kvm = kvm_ioctls::Kvm::new().map_err(|e| format!("open /dev/kvm: {e}"))?;
    let supported = kvm
        .get_supported_cpuid(kvm_bindings::KVM_MAX_CPUID_ENTRIES)
        .map_err(|e| format!("KVM_GET_SUPPORTED_CPUID: {e}"))?;
    let masked = dh_vmm::cpuid::masked_cpuid(&kvm).map_err(|e| format!("{e:?}"))?;

    let key = |e: &kvm_bindings::kvm_cpuid_entry2| (e.function, e.index);
    let sup: BTreeMap<_, _> = supported.as_slice().iter().map(|e| (key(e), *e)).collect();
    let msk: BTreeMap<_, _> = masked.as_slice().iter().map(|e| (key(e), *e)).collect();

    let mut out = String::new();
    out.push_str(&format!(
        "supported entries: {}   masked entries: {}\n",
        sup.len(),
        msk.len()
    ));
    for (k, s) in &sup {
        match msk.get(k) {
            None => out.push_str(&format!(
                "leaf {:#010x}.{}: REMOVED (was eax={:#x} ebx={:#x} ecx={:#x} edx={:#x})\n",
                k.0, k.1, s.eax, s.ebx, s.ecx, s.edx
            )),
            Some(m) => {
                for (reg, sv, mv) in [
                    ("eax", s.eax, m.eax),
                    ("ebx", s.ebx, m.ebx),
                    ("ecx", s.ecx, m.ecx),
                    ("edx", s.edx, m.edx),
                ] {
                    if sv != mv {
                        out.push_str(&format!(
                            "leaf {:#010x}.{} {}: {:#010x} -> {:#010x} (cleared {:#010x})\n",
                            k.0,
                            k.1,
                            reg,
                            sv,
                            mv,
                            sv & !mv
                        ));
                    }
                }
            }
        }
    }
    out.push_str(&format!(
        "masked table hash: {}\n",
        hex(&dh_vmm::cpuid::cpuid_table_hash(&masked))
    ));
    Ok(out)
}

fn hex(b: &[u8; 32]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
