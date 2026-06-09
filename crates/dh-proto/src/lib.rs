#![forbid(unsafe_code)]

pub use determinism_proto::hypervisor::v1;

pub fn sample_lease() -> v1::Lease {
    v1::Lease {
        slot_id: 1,
        token: vec![0; 16],
    }
}
