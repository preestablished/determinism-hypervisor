#![forbid(unsafe_code)]

pub fn preflight_summary() -> String {
    let missing = dh_vmm::missing_caps(&dh_vmm::required_caps_template());
    format!("kvm_m0_missing_caps={}", missing.len())
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
