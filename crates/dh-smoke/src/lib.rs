#![forbid(unsafe_code)]

pub fn kvm_smoke_report() -> String {
    dh_worker::preflight_summary()
}

#[cfg(test)]
mod tests {
    #[test]
    fn reports_clean_template() {
        assert_eq!(super::kvm_smoke_report(), "kvm_m0_missing_caps=0");
    }
}
