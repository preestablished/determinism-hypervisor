#![forbid(unsafe_code)]

// Local debug CLI (ARCH §1): drives the VMM directly. It must not depend on
// dh-worker — "nothing depends on dh-worker" is a normative dependency rule.
fn main() {
    let missing = dh_vmm::missing_caps(&dh_vmm::required_caps_template());
    println!("kvm_m0_missing_caps={}", missing.len());
}
