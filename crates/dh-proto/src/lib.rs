// `forbid` is too strong while tonic codegen is in the tree: include_proto
// expands to code we don't control. Manual code keeps the discipline via deny.
#![deny(unsafe_code)]

//! Contract crate for the `determinism.hypervisor.v1` gRPC surface.
//!
//! Codegen home decision: docs/decisions/proto-seam.md. The schema lives in
//! this repo at `proto/hypervisor.proto` and is compiled by this crate's
//! build.rs (tonic-build + vendored protoc). Currently a SKELETON — the full
//! API.md §2 surface lands with bead bcb.

/// Generated `determinism.hypervisor.v1` types and service stubs.
///
/// Single re-export seam: if/when control-plane's determinism-proto adopts
/// real hypervisor codegen (an adopt-hypervisor-proto-v1 request, mirroring
/// snapshot-store's adopt-snapstore-proto-v1), this module body swaps to a
/// re-export of that crate and nothing else in the workspace changes.
pub mod v1 {
    tonic::include_proto!("determinism.hypervisor.v1");
}

/// Cross-repo facade for the shared non-hypervisor contract types
/// (`determinism.common.v1` placeholders owned by control-plane).
pub use determinism_proto::common;
pub use determinism_proto::PROTO_VERSION;

/// Wire-up smoke fixture retained from the placeholder-facade era: proves a
/// generated message constructs with the documented shapes. In-crate test use
/// only today; whether it stays public is a bead-bcb call.
pub fn sample_lease() -> v1::Lease {
    v1::Lease {
        slot_id: 1,
        token: vec![0; 16],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Codegen skeleton end-to-end pin (bead v8p): prost message round-trip
    /// plus existence of both generated service halves.
    #[test]
    fn skeleton_codegen_is_live() {
        use prost::Message;

        let lease = sample_lease();
        let bytes = lease.encode_to_vec();
        let back = v1::Lease::decode(bytes.as_slice()).expect("lease round-trip");
        assert_eq!(lease, back);

        // Service stubs exist on both halves (client + server).
        let _ =
            v1::hypervisor_worker_client::HypervisorWorkerClient::<tonic::transport::Channel>::new;
        fn _server_trait_is_generated<T: v1::hypervisor_worker_server::HypervisorWorker>() {}

        // §2.8 message shapes.
        let info = v1::GetWorkerInfoResponse {
            worker_id: "host".into(),
            slots_total: 8,
            slots_free: 8,
            class: Some(v1::DeterminismClass {
                cpu_model: "6/158/10".into(),
                microcode: "0xfa".into(),
                host_kernel: "6.8.0-124".into(),
                vmm_version: env!("CARGO_PKG_VERSION").into(),
            }),
            version: env!("CARGO_PKG_VERSION").into(),
        };
        assert_eq!(
            v1::GetWorkerInfoResponse::decode(info.encode_to_vec().as_slice()).unwrap(),
            info
        );
    }
}
