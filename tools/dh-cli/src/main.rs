#![forbid(unsafe_code)]

// Local debug CLI (ARCH §1): drives the VMM directly. It must not depend on
// dh-worker — "nothing depends on dh-worker" is a normative dependency rule.
fn main() {
    println!("{}", dh_vmm::m0_missing_caps_summary());
}
