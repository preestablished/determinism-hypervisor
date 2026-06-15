#![forbid(unsafe_code)]

#[cfg(target_arch = "x86_64")]
pub mod fork_engine;
pub mod preflight;
pub mod proto_map;
#[cfg(target_arch = "x86_64")]
pub mod replay_engine;
#[cfg(target_arch = "x86_64")]
pub mod restore_engine;
#[cfg(target_arch = "x86_64")]
pub mod runtime;
pub mod service;
pub mod slot_manager;
#[cfg(target_arch = "x86_64")]
pub mod snapshot_engine;
#[cfg(target_arch = "x86_64")]
pub mod verify_replay;

pub fn preflight_summary() -> String {
    dh_vmm::m0_missing_caps_summary()
}

pub fn workspace_component_count() -> usize {
    [
        dh_detclock::DET_CLOCK_COMPONENT,
        dh_devices::DEVICE_MODEL_COMPONENT,
        dh_verify::VERIFY_COMPONENT,
    ]
    .len()
}

#[cfg(test)]
mod tests {
    #[test]
    fn reports_clean_template() {
        assert_eq!(super::preflight_summary(), "kvm_m0_missing_caps=0");
    }
}
