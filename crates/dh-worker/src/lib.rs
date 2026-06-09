#![forbid(unsafe_code)]

pub fn preflight_summary() -> String {
    let missing = dh_kvm::missing_caps(&dh_kvm::required_caps_template());
    format!("kvm_m0_missing_caps={}", missing.len())
}
