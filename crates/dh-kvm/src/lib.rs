#![forbid(unsafe_code)]

use determinism_proto::hypervisor::v1::KvmCaps;

pub fn required_caps_template() -> KvmCaps {
    KvmCaps {
        user_space_msr: true,
        msr_filter: true,
        dirty_ring: true,
        immediate_exit: true,
        no_in_kernel_irqchip: true,
    }
}

pub fn missing_caps(caps: &KvmCaps) -> Vec<&'static str> {
    [
        (caps.user_space_msr, "KVM_CAP_X86_USER_SPACE_MSR"),
        (caps.msr_filter, "KVM_MSR_EXIT_REASON_FILTER"),
        (caps.dirty_ring, "KVM_CAP_DIRTY_LOG_RING"),
        (caps.immediate_exit, "immediate_exit"),
        (caps.no_in_kernel_irqchip, "no in-kernel irqchip"),
    ]
    .into_iter()
    .filter_map(|(present, name)| (!present).then_some(name))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_satisfies_m0_caps() {
        assert!(missing_caps(&required_caps_template()).is_empty());
    }
}
