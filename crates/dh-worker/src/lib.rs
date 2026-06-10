#![forbid(unsafe_code)]

pub mod preflight;

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
