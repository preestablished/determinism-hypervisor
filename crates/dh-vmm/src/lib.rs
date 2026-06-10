#![deny(unsafe_code)] // targeted allows in the x86_64 KVM modules only (raw ioctls, memfd, madvise)

// Architecture split (bead v5w): the pure determinism math — agenda
// scheduling, virtual-time rationals, machine config, block fixtures —
// builds and tests everywhere (the aarch64 CI lane covers it). The
// KVM-touching modules consume x86_64-only kvm-bindings APIs (MSR
// filter, CPUID, VMX-shaped exits) and are gated to x86_64.
pub mod agenda;
pub mod blkfile;
pub mod config;
pub mod vt;

#[cfg(target_arch = "x86_64")]
pub mod boot;
#[cfg(target_arch = "x86_64")]
pub mod boundary;
#[cfg(target_arch = "x86_64")]
pub mod cpuid;
#[cfg(target_arch = "x86_64")]
pub mod hash;
#[cfg(target_arch = "x86_64")]
pub mod inject;
#[cfg(target_arch = "x86_64")]
pub mod kvm;
#[cfg(target_arch = "x86_64")]
pub mod msr;
#[cfg(target_arch = "x86_64")]
pub mod run;
#[cfg(target_arch = "x86_64")]
pub mod runctl;
#[cfg(target_arch = "x86_64")]
pub mod tsc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotState {
    Empty,
    Running,
    Paused,
    Frozen,
}

pub fn initial_slot_state() -> SlotState {
    SlotState::Empty
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KvmCaps {
    pub user_space_msr: bool,
    pub msr_filter: bool,
    pub dirty_ring: bool,
    pub immediate_exit: bool,
    pub no_in_kernel_irqchip: bool,
}

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
        (caps.immediate_exit, "KVM_CAP_IMMEDIATE_EXIT"),
        (caps.no_in_kernel_irqchip, "no in-kernel irqchip"),
    ]
    .into_iter()
    .filter_map(|(present, name)| (!present).then_some(name))
    .collect()
}

// Single source of the M0 preflight summary line: dh-worker's preflight and
// dh-cli both print it, but neither may define it — dh-cli must not depend on
// dh-worker (ARCH §1), so the shared format lives here.
pub fn m0_missing_caps_summary() -> String {
    format!(
        "kvm_m0_missing_caps={}",
        missing_caps(&required_caps_template()).len()
    )
}

pub fn architecture_components_present() -> bool {
    dh_detclock::DET_CLOCK_COMPONENT == "dh-detclock"
        && dh_devices::DEVICE_MODEL_COMPONENT == "dh-devices"
        && dh_inputlog::DHILOG_FORMAT_VERSION == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_satisfies_m0_caps() {
        assert!(missing_caps(&required_caps_template()).is_empty());
    }

    #[test]
    fn arch_components_are_linked() {
        assert!(architecture_components_present());
    }

    #[test]
    fn m0_summary_format_is_stable() {
        assert_eq!(m0_missing_caps_summary(), "kvm_m0_missing_caps=0");
    }
}
