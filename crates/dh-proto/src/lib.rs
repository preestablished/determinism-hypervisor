// `forbid` is too strong while tonic codegen is in the tree: include_proto
// expands to code we don't control. Manual code keeps the discipline via deny.
#![deny(unsafe_code)]

//! Contract crate for the `determinism.hypervisor.v1` gRPC surface.
//!
//! Codegen home decision: docs/decisions/proto-seam.md. The schema lives in
//! this repo at `proto/hypervisor.proto` and is compiled by this crate's
//! build.rs (tonic-build + vendored protoc). Covers the full API.md §2
//! surface (bead bcb): slot lifecycle, input injection, Run/Pause,
//! snapshots, introspection, verification, frame capture, worker/health,
//! the §2.9 ErrorDetail, and §2.10 Quiesce.

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

    /// Full-surface rpc pin (bead bcb): every API.md §2 method exists on the
    /// generated client with its request type (method names + request types
    /// ARE the contract surface; a proto rename breaks compilation here, not
    /// in dh-worker). Never called — the call SHAPES are the assertion;
    /// tonic's `impl IntoRequest<T>` args rule out plain fn-item pins.
    async fn _all_seventeen_rpcs(
        c: &mut v1::hypervisor_worker_client::HypervisorWorkerClient<tonic::transport::Channel>,
    ) {
        // Slot lifecycle (§2.2).
        let _ = c.create_vm(v1::CreateVmRequest::default()).await;
        let _ = c
            .restore_snapshot(v1::RestoreSnapshotRequest::default())
            .await;
        let _ = c.fork(v1::ForkRequest::default()).await;
        let _ = c.destroy_vm(v1::DestroyVmRequest::default()).await;
        // Execution (§2.3–§2.5, §2.10).
        let _ = c.inject_inputs(v1::InjectInputsRequest::default()).await;
        let _ = c.run(v1::RunRequest::default()).await;
        let _ = c.pause(v1::PauseRequest::default()).await;
        let _ = c.take_snapshot(v1::TakeSnapshotRequest::default()).await;
        let _ = c.quiesce(v1::QuiesceRequest::default()).await;
        // Introspection (§2.6).
        let _ = c
            .read_guest_memory(v1::ReadGuestMemoryRequest::default())
            .await;
        let _ = c
            .get_framebuffer(v1::GetFramebufferRequest::default())
            .await;
        let _ = c
            .stream_guest_events(v1::StreamGuestEventsRequest::default())
            .await;
        // Verification & rendering (§2.7).
        let _ = c.verify_replay(v1::VerifyReplayRequest::default()).await;
        let _ = c
            .run_with_frame_capture(v1::RunWithFrameCaptureRequest::default())
            .await;
        // Worker / health (§2.8).
        let _ = c.get_worker_info(v1::GetWorkerInfoRequest::default()).await;
        let _ = c.list_slots(v1::ListSlotsRequest::default()).await;
        let _ = c.watch_slots(v1::WatchSlotsRequest::default()).await;
    }

    #[test]
    fn all_seventeen_rpcs_are_generated() {
        // The assertion is that _all_seventeen_rpcs compiles; reference it
        // so it cannot be deleted as dead code.
        let _ = _all_seventeen_rpcs;
    }

    /// Message-shape pins across the §2 surface: oneofs, nested enums, and
    /// round-trips for the load-bearing request/response pairs.
    #[test]
    fn full_surface_message_shapes() {
        use prost::Message;
        use v1::{run_request, scheduled_event, verify_replay_progress};

        // §2.3 ScheduledEvent: both oneofs.
        let ev = v1::ScheduledEvent {
            at: Some(scheduled_event::At::AtFrame(7)),
            event: Some(scheduled_event::Event::PadSet(v1::PadSet {
                port: 0,
                buttons: 0xF0,
            })),
        };
        let back = v1::ScheduledEvent::decode(ev.encode_to_vec().as_slice()).unwrap();
        assert_eq!(ev, back);

        // §2.4 RunRequest: frame_budget arm (field 8) + capture + hard cap.
        let req = v1::RunRequest {
            lease: Some(sample_lease()),
            until: Some(run_request::Until::FrameBudget(60)),
            hard_icount_cap: 0,
            capture: Some(v1::CaptureSpec {
                ranges: vec![v1::ExtractRange {
                    region: "FRAMEBUFFER".into(),
                    layout_version: 1,
                    offset: 0,
                    len: 4096,
                }],
                framebuffer: true,
            }),
        };
        assert_eq!(
            v1::RunRequest::decode(req.encode_to_vec().as_slice()).unwrap(),
            req
        );

        // §2.5 TakeSnapshotRequest: seal_input_log has presence because the
        // API default is true but explicit false is meaningful.
        let default_seal = v1::TakeSnapshotRequest {
            lease: Some(sample_lease()),
            seal_input_log: None,
            capture: None,
        };
        let explicit_false = v1::TakeSnapshotRequest {
            lease: Some(sample_lease()),
            seal_input_log: Some(false),
            capture: None,
        };
        assert_ne!(default_seal.encode_to_vec(), explicit_false.encode_to_vec());
        assert_eq!(
            v1::TakeSnapshotRequest::decode(explicit_false.encode_to_vec().as_slice()).unwrap(),
            explicit_false
        );

        // §2.4 GoalCondition with the nested MemPredicate::Op enum.
        let goal = v1::GoalCondition {
            all_of: vec![v1::MemPredicate {
                gpa: 0x1000,
                width: 4,
                mask: 0xFF,
                op: v1::mem_predicate::Op::Eq as i32,
                value: 1,
            }],
            poll_period: 1_000_000,
        };
        assert_eq!(
            v1::GoalCondition::decode(goal.encode_to_vec().as_slice()).unwrap(),
            goal
        );

        // Enum number pins — a silent renumber must fail HERE, not on the
        // wire. The PAUSED_S disambiguation is load-bearing (C++ scoping;
        // see proto/hypervisor.proto).
        assert_eq!(v1::HashEpochs::Unspecified as i32, 0);
        assert_eq!(v1::HashEpochs::EpochsOn as i32, 1);
        assert_eq!(v1::HashEpochs::FinalOnly as i32, 2);
        assert_eq!(v1::PixelFormat::PfUnspecified as i32, 0);
        assert_eq!(v1::PixelFormat::Xrgb8888 as i32, 1);
        assert_eq!(v1::PixelFormat::Rgb565 as i32, 2);
        assert_eq!(v1::StopReason::StopUnspecified as i32, 0);
        assert_eq!(v1::StopReason::BudgetReached as i32, 1);
        assert_eq!(v1::StopReason::GoalSatisfied as i32, 2);
        assert_eq!(v1::StopReason::NextSdkEvent as i32, 3);
        assert_eq!(v1::StopReason::HardCap as i32, 4);
        assert_eq!(v1::StopReason::Paused as i32, 5);
        assert_eq!(v1::StopReason::GuestHalted as i32, 6);
        assert_eq!(v1::StopReason::Faulted as i32, 7);
        assert_eq!(v1::SlotState::SlotUnspecified as i32, 0);
        assert_eq!(v1::SlotState::Empty as i32, 1);
        assert_eq!(v1::SlotState::PausedS as i32, 2);
        assert_eq!(v1::SlotState::Running as i32, 3);
        assert_eq!(v1::SlotState::Frozen as i32, 4);
        assert_eq!(v1::SlotState::FaultedS as i32, 5);
        assert_eq!(v1::QuiesceMode::Unspecified as i32, 0);
        assert_eq!(v1::QuiesceMode::Coop as i32, 1);
        assert_eq!(v1::QuiesceMode::Forced as i32, 2);
        assert_eq!(v1::mem_predicate::Op::Unspecified as i32, 0);
        assert_eq!(v1::mem_predicate::Op::Eq as i32, 1);
        assert_eq!(v1::mem_predicate::Op::Ne as i32, 2);
        assert_eq!(v1::mem_predicate::Op::Ge as i32, 3);
        assert_eq!(v1::mem_predicate::Op::Le as i32, 4);

        // §2.1 MachineConfig: append-only canonical MCFG fields. The
        // message must carry the cpuid_table/device_set bytes needed to
        // reconstruct the exact domain MachineConfig recovered from DHSNAP.
        let cfg = v1::MachineConfig {
            version: 1,
            mem_bytes: 64 * 1024 * 1024,
            vcpus: 1,
            clock_num: 1,
            clock_den: 1,
            base_image_hash: vec![0x11; 32],
            boot: Some(v1::BootSpec {
                kind: Some(v1::boot_spec::Kind::Elf(v1::ElfBoot {
                    kernel_hash: vec![0x22; 32],
                    cmdline: b"console=none".to_vec(),
                })),
            }),
            epoch_len: 50_000_000,
            hash_epochs: v1::HashEpochs::EpochsOn as i32,
            skid_margin: 32_768,
            cpuid_table: vec![v1::CpuidLeaf {
                function: 1,
                index: 0,
                flags: 0,
                eax: 1,
                ebx: 2,
                ecx: 3,
                edx: 4,
            }],
            device_set: vec![1, 4, 7],
        };
        let cfg_bytes = cfg.encode_to_vec();
        let cfg_back = v1::MachineConfig::decode(cfg_bytes.as_slice()).unwrap();
        assert_eq!(cfg_back, cfg);
        assert!(cfg_bytes
            .windows([0x5A, 0x0A].len())
            .any(|w| w == [0x5A, 0x0A]));
        assert!(cfg_bytes
            .windows([0x62, 0x03, 0x01, 0x04, 0x07].len())
            .any(|w| w == [0x62, 0x03, 0x01, 0x04, 0x07]));
        let restore = v1::RestoreSnapshotResponse {
            lease: Some(sample_lease()),
            config: Some(cfg),
            state_hash: Some(v1::StateHash {
                hash: vec![0xAA; 32],
            }),
            frame_counter: 9,
        };
        let restore_back =
            v1::RestoreSnapshotResponse::decode(restore.encode_to_vec().as_slice()).unwrap();
        let restored_cfg = restore_back.config.unwrap();
        assert_eq!(restored_cfg.cpuid_table.len(), 1);
        assert_eq!(restored_cfg.device_set, vec![1, 4, 7]);

        // §2.2 ForkRequest: per-child segment seeds are explicit wire bytes.
        // The all-zero entry means "continue fork-point PRNG"; non-zero starts
        // a fresh deterministic stream for that child.
        let fork = v1::ForkRequest {
            parent: Some(sample_lease()),
            count: 2,
            entropy_seeds: vec![vec![0; 32], vec![0xA7; 32]],
        };
        let fork_bytes = fork.encode_to_vec();
        assert_eq!(
            v1::ForkRequest::decode(fork_bytes.as_slice()).unwrap(),
            fork
        );
        assert!(fork_bytes
            .windows([0x1A, 0x20].len())
            .any(|w| w == [0x1A, 0x20]));

        // frame_budget rides wire tag 8 (varint key 0x40) — pinned at the
        // byte level since the non-contiguous number is easy to "tidy".
        let only_frame_budget = v1::RunRequest {
            until: Some(run_request::Until::FrameBudget(60)),
            ..Default::default()
        };
        assert_eq!(only_frame_budget.encode_to_vec(), vec![0x40, 60]);

        // §2.7 VerifyReplayProgress oneof: divergence is terminal/P0.
        let prog = v1::VerifyReplayProgress {
            msg: Some(verify_replay_progress::Msg::Divergence(v1::Divergence {
                first_bad_epoch: 3,
                icount_lo: 100,
                icount_hi: 1124,
                rip_expected: 0xFFFF_8000_0000_1000,
                rip_actual: 0xFFFF_8000_0000_1004,
                reg_diff: vec![],
                diff_page_idx: vec![1, 2, 3],
                suspected_cause: "RDTSC at divergent RIP".into(),
            })),
        };
        assert_eq!(
            v1::VerifyReplayProgress::decode(prog.encode_to_vec().as_slice()).unwrap(),
            prog
        );

        // §2.6 NextSdkEvent: proto3 optional ⇒ Option<u32>.
        let any = v1::NextSdkEvent { stream: None };
        let filt = v1::NextSdkEvent { stream: Some(9) };
        assert_ne!(any.encode_to_vec(), filt.encode_to_vec());

        // §2.9 ErrorDetail.
        let detail = v1::ErrorDetail {
            slot_id: 4,
            icount: 1_000_000,
            code: "stale_lease".into(),
        };
        assert_eq!(
            v1::ErrorDetail::decode(detail.encode_to_vec().as_slice()).unwrap(),
            detail
        );
    }

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
            build_profile: "release".into(),
        };
        assert_eq!(
            v1::GetWorkerInfoResponse::decode(info.encode_to_vec().as_slice()).unwrap(),
            info
        );
    }
}
