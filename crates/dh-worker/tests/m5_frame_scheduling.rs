//! M5 ACCEPT (bead 5yo): `at_frame` resolution and `frame_budget`
//! stops use the pv-pad FRAME_MARK table's absolute FRAME_COUNTER basis,
//! including across a snapshot/restore seam.
//!
//! This intentionally exercises the lower contract before the worker API
//! grows the public `InjectInputs.at_frame` mapper: the fake-frame guest
//! writes strictly increasing absolute frame indices, run control stops
//! on the requested count of MORE frames, and the restored PADD section
//! carries the absolute counter forward.
//!
//! HARDWARE-GATED: kvm-intel lane + lab/reference box; self-skips
//! elsewhere.

#![cfg(target_arch = "x86_64")]

mod common;

use std::path::Path;
use std::sync::atomic::AtomicBool;

use common::{gettid, kvm_available, spawn_store_blocking, TestResult, VmMem};
use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};
use dh_devices::ctx::VecGuestMem;
use dh_devices::entropy::{DetEntropy, PvEntropy};
use dh_devices::pad::{PvPad, PV_PAD_BASE, REG_FRAME_COUNTER};
use dh_devices::{DevCtx, MmioBus};
use dh_inputlog::dhilog::{LogWriter, SegmentHeader};
use dh_inputlog::reader::{LogReader, RecordBody};
use dh_proto::v1 as proto;
use dh_proto::v1::hypervisor_worker_server::HypervisorWorker;
use dh_vmm::boundary::BoundaryError;
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::hash::StateHashChain;
use dh_vmm::kvm::{KvmSystem, SlotVm};
use dh_vmm::recording::DeviceRail;
use dh_vmm::runctl::{run_segment, Segment, SegmentOutcome, StopReason, Until};
use dh_vmm::SlotState;
use dh_worker::restore_engine::restore_snapshot;
use dh_worker::snapshot_engine::{take_snapshot, BoundaryState, PageSource};
use snapstore_manifest::input_log::InputLogContainer;
use tokio_stream::StreamExt;
use tonic::Request;

const MEM: u64 = 16 << 20;
const FIRST_FRAMES: u64 = 3;
const AFTER_RESTORE_FRAMES: u64 = 2;
/// Measured 2026-07-07 against reference-workload dist workload-image-0.1.0
/// (built_from 7b0c7b2, refwork >= 40eaf4f; initramfs blake3 36f50484…):
/// max ~27.7M instr/frame (fresh f1=7.77M, f2/f3=27.68M; post-restore
/// f4/f5=27.68M; NOP-game diagnostic frame 9.2M), identical across two
/// consecutive runs. Cap = 27.7M/frame × 3 frames (FIRST_FRAMES) × ~1.8
/// margin ≈ 150M — a safety net for runaway boots, not a perf assertion;
/// the real-path normal stop is BUDGET_REACHED. Retune by re-running:
/// cargo test -p dh-worker --release --test m5_frame_scheduling -- \
///   --ignored linux_m5 --nocapture
const FRAME_HARD_CAP: u64 = 150_000_000;
const DETCHANNEL_FRAME_HARD_CAP: u64 = 1_000_000;
const AT_FRAME_TARGET: u32 = 2;
const DETCHANNEL_FRAME_TEST: &str =
    "m5_frame_scheduling::detchannel_frame_budget_drains_sdk_frame_marks_across_restore";

#[test]
#[ignore = "M9 Linux acceptance: requires KVM dirty-ring support and staged DH_M9_* artifacts"]
fn linux_m5_frame_budget_records_post_ready_frame_marks() -> TestResult<()> {
    let Some(ready) = common::m9_linux_ready_snapshot(
        "m5_frame_scheduling::linux_m5_frame_budget_records_post_ready_frame_marks",
        2,
    )?
    else {
        return Ok(());
    };

    let start_frame = ready.ready_snapshot.frame_counter;
    let first_expected = expected_frame_table(start_frame, FIRST_FRAMES)?;
    let first_end = *first_expected
        .last()
        .ok_or_else(|| "first expected frame table is empty".to_string())?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("test runtime: {e}"))?;
    let (first_run, first_snapshot, restored_frame_counter, second_run, second_snapshot) = rt
        .block_on(async {
            let first_run = ready
                .svc
                .run(Request::new(proto::RunRequest {
                    lease: Some(ready.lease.clone()),
                    until: Some(proto::run_request::Until::FrameBudget(FIRST_FRAMES as u32)),
                    hard_icount_cap: FRAME_HARD_CAP,
                    capture: None,
            }))
            .await
            .map_err(|e| format!("Run first Linux frame budget: {e}"))?
            .into_inner();
            if let Err(e) = assert_worker_frame_budget(&first_run, FIRST_FRAMES, "first Linux run")
            {
                let diagnostic_snapshot = ready
                    .svc
                    .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                        lease: Some(ready.lease.clone()),
                        seal_input_log: Some(false),
                        capture: None,
                    }))
                    .await
                    .ok()
                    .map(|response| response.into_inner());
                let events = stream_guest_event_summaries(&ready.svc, &ready.lease, 64)
                    .await
                    .unwrap_or_else(|err| vec![format!("StreamGuestEvents failed: {err}")]);
                let _ = ready
                    .svc
                    .destroy_vm(Request::new(proto::DestroyVmRequest {
                        lease: Some(ready.lease.clone()),
                    }))
                    .await;
                return Err(format!(
                    "{e}; ready_icount={} ready_frame_counter={} post_run_icount={} post_run_frame_counter={:?} buffered_events={events:?}",
                    ready.ready_snapshot.icount,
                    start_frame,
                    first_run.icount,
                    diagnostic_snapshot.as_ref().map(|snapshot| snapshot.frame_counter),
                ));
            }

            let first_snapshot = ready
                .svc
                .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                    lease: Some(ready.lease.clone()),
                    seal_input_log: Some(true),
                    capture: None,
                }))
                .await
                .map_err(|e| format!("TakeSnapshot first Linux frame budget: {e}"))?
                .into_inner();
            let first_ref = first_snapshot
                .snapshot
                .clone()
                .ok_or_else(|| "first frame snapshot returned no snapshot ref".to_string())?;

            let restored = ready
                .svc
                .restore_snapshot(Request::new(proto::RestoreSnapshotRequest {
                    snapshot: Some(first_ref),
                    entropy_seed: Vec::new(),
                }))
                .await
                .map_err(|e| format!("RestoreSnapshot first frame boundary: {e}"))?
                .into_inner();
            let restored_frame_counter = restored.frame_counter;
            let restored_lease = restored
                .lease
                .ok_or_else(|| "RestoreSnapshot returned no lease".to_string())?;

            let second_run = ready
                .svc
                .run(Request::new(proto::RunRequest {
                    lease: Some(restored_lease.clone()),
                    until: Some(proto::run_request::Until::FrameBudget(
                        AFTER_RESTORE_FRAMES as u32,
                    )),
                    hard_icount_cap: FRAME_HARD_CAP,
                    capture: None,
                }))
                .await
                .map_err(|e| format!("Run restored Linux frame budget: {e}"))?
                .into_inner();
            assert_worker_frame_budget(&second_run, AFTER_RESTORE_FRAMES, "restored Linux run")?;

            let second_snapshot = ready
                .svc
                .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                    lease: Some(restored_lease.clone()),
                    seal_input_log: Some(true),
                    capture: None,
                }))
                .await
                .map_err(|e| format!("TakeSnapshot restored Linux frame budget: {e}"))?
                .into_inner();

            let _ = ready
                .svc
                .destroy_vm(Request::new(proto::DestroyVmRequest {
                    lease: Some(restored_lease),
                }))
                .await;
            let _ = ready
                .svc
                .destroy_vm(Request::new(proto::DestroyVmRequest {
                    lease: Some(ready.lease.clone()),
                }))
                .await;

            Ok::<_, String>((
                first_run,
                first_snapshot,
                restored_frame_counter,
                second_run,
                second_snapshot,
            ))
        })?;

    assert_eq!(
        first_snapshot.frame_counter, first_end,
        "first Linux frame-budget snapshot must persist the absolute pv-pad counter"
    );
    assert_eq!(
        restored_frame_counter, first_end,
        "RestoreSnapshot must report the PADD-restored absolute frame counter"
    );
    let first_log = input_log_payload(&ready.store, &first_snapshot.input_log_id)?;
    let first_marks = frame_marks(&first_log);
    assert_strict_frame_table(&first_marks, &first_expected);
    assert!(
        resolve_at_frame(&first_marks, first_end).is_some(),
        "first segment frame table must resolve the final absolute frame"
    );

    let second_expected = expected_frame_table(first_end, AFTER_RESTORE_FRAMES)?;
    let second_end = *second_expected
        .last()
        .ok_or_else(|| "second expected frame table is empty".to_string())?;
    assert_eq!(
        second_snapshot.frame_counter, second_end,
        "restored Linux frame-budget snapshot must continue the absolute pv-pad counter"
    );
    let second_log = input_log_payload(&ready.store, &second_snapshot.input_log_id)?;
    let second_marks = frame_marks(&second_log);
    assert_strict_frame_table(&second_marks, &second_expected);
    assert!(
        resolve_at_frame(&second_marks, AFTER_RESTORE_FRAMES as u32).is_none(),
        "restored frame table must use absolute frame numbers, not segment-relative values"
    );

    eprintln!(
        "linux-m5 frames start={} first_icount={} first_frames={:?} restored_icount={} restored_frames={:?}",
        start_frame,
        first_run.icount,
        first_marks,
        second_run.icount,
        second_marks
    );

    Ok(())
}

#[test]
#[ignore = "M9 Linux diagnostic: requires KVM dirty-ring support and real-emulator DH_M9_* artifacts"]
fn linux_m5_real_emulator_nop_game_frame_budget_diagnostic() -> TestResult<()> {
    let Some(artifacts) = common::m9_artifacts(
        "m5_frame_scheduling::linux_m5_real_emulator_nop_game_frame_budget_diagnostic",
    )?
    else {
        return Ok(());
    };
    common::assert_m9_real_emulator_initramfs(&artifacts.initramfs)?;
    let nop_game_hash = write_cache_blob(&artifacts.image_cache, &nop_rom())?;
    eprintln!(
        "m5_frame_scheduling::linux_m5_real_emulator_nop_game_frame_budget_diagnostic: nop_game_blake3={}",
        hex_hash(&nop_game_hash)
    );
    let Some(ready) = common::m9_linux_ready_snapshot_with_config(
        "m5_frame_scheduling::linux_m5_real_emulator_nop_game_frame_budget_diagnostic",
        2,
        |config| {
            config.base_image_hash = nop_game_hash;
        },
    )?
    else {
        return Ok(());
    };

    let start_frame = ready.ready_snapshot.frame_counter;
    let expected = expected_frame_table(start_frame, 1)?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("test runtime: {e}"))?;
    let (run, streamed, snapshot) = rt.block_on(async {
        let run = ready
            .svc
            .run(Request::new(proto::RunRequest {
                lease: Some(ready.lease.clone()),
                until: Some(proto::run_request::Until::FrameBudget(1)),
                hard_icount_cap: FRAME_HARD_CAP,
                capture: None,
            }))
            .await
            .map_err(|e| format!("Run real-emulator NOP-game frame budget: {e}"))?
            .into_inner();
        if let Err(e) = assert_worker_frame_budget(&run, 1, "real-emulator NOP-game run") {
            let diagnostic_snapshot = ready
                .svc
                .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                    lease: Some(ready.lease.clone()),
                    seal_input_log: Some(false),
                    capture: None,
                }))
                .await
                .ok()
                .map(|response| response.into_inner());
            let events = stream_guest_event_summaries(&ready.svc, &ready.lease, 64)
                .await
                .unwrap_or_else(|err| vec![format!("StreamGuestEvents failed: {err}")]);
            let _ = ready
                .svc
                .destroy_vm(Request::new(proto::DestroyVmRequest {
                    lease: Some(ready.lease.clone()),
                }))
                .await;
            return Err(format!(
                "{e}; ready_icount={} ready_frame_counter={} post_run_icount={} post_run_frame_counter={:?} buffered_events={events:?}",
                ready.ready_snapshot.icount,
                start_frame,
                run.icount,
                diagnostic_snapshot.as_ref().map(|snapshot| snapshot.frame_counter),
            ));
        }
        let streamed =
            stream_frame_mark_events(&ready.svc, &ready.lease, &expected, "NOP-game run").await?;
        let snapshot = ready
            .svc
            .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                lease: Some(ready.lease.clone()),
                seal_input_log: Some(true),
                capture: None,
            }))
            .await
            .map_err(|e| format!("TakeSnapshot real-emulator NOP-game frame: {e}"))?
            .into_inner();
        let _ = ready
            .svc
            .destroy_vm(Request::new(proto::DestroyVmRequest {
                lease: Some(ready.lease.clone()),
            }))
            .await;
        Ok::<_, String>((run, streamed, snapshot))
    })?;

    let log = input_log_payload(&ready.store, &snapshot.input_log_id)?;
    let marks = frame_marks(&log);
    assert_strict_frame_table(&marks, &expected);
    assert_sdk_frames_match_frame_marks(
        &streamed,
        &marks,
        ready.ready_snapshot.icount,
        "real-emulator NOP-game run",
    );

    eprintln!(
        "linux-m5 real-emulator NOP-game start={} icount={} sdk_frames={:?} frame_marks={:?}",
        start_frame, run.icount, streamed, marks
    );
    Ok(())
}

#[test]
fn detchannel_frame_budget_drains_sdk_frame_marks_across_restore() -> TestResult<()> {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return Ok(());
    }
    let cpuid_table = match KvmSystem::open() {
        Ok(sys) if sys.dirty_ring => sys
            .masked_cpuid_table()
            .map_err(|e| format!("masked CPUID table: {e:?}"))?,
        Ok(_) => {
            eprintln!("skipping: KVM dirty ring unavailable");
            return Ok(());
        }
        Err(e) => {
            eprintln!("skipping: KVM unavailable: {e:?}");
            return Ok(());
        }
    };

    let image_cache = tempfile::TempDir::new().map_err(|e| format!("image cache tempdir: {e}"))?;
    let base_hash = write_cache_blob(image_cache.path(), &vec![0u8; 4096])?;
    let kernel_hash = write_cache_blob(image_cache.path(), nanokernel::detchannel_frames_elf())?;
    let config = detchannel_frame_machine_config(base_hash, kernel_hash, cpuid_table);

    let store_dir = tempfile::TempDir::new().map_err(|e| format!("snapstore tempdir: {e}"))?;
    let store_sock = "snapstore.sock";
    let (_store_rt, _store_handle, store) =
        common::spawn_store_at(store_dir.path().to_path_buf(), store_sock);
    let snapstore = snapstore_client::Transport::Uds(store_dir.path().join(store_sock));
    let svc = dh_worker::service::WorkerService::new(common::m9_worker_config(
        DETCHANNEL_FRAME_TEST,
        2,
        image_cache.path().to_path_buf(),
        snapstore,
    ))
    .map_err(|e| format!("WorkerService::new: {e:?}"))?;

    let start_frame = 0;
    let first_expected = expected_frame_table(start_frame, FIRST_FRAMES)?;
    let first_end = *first_expected
        .last()
        .ok_or_else(|| "first expected frame table is empty".to_string())?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("test runtime: {e}"))?;
    let (
        first_run,
        first_sdk_frames,
        first_snapshot,
        restored_frame_counter,
        second_run,
        second_sdk_frames,
        second_snapshot,
        initial_ref,
        first_ref,
    ) = rt.block_on(async {
        let created = svc
            .create_vm(Request::new(proto::CreateVmRequest {
                config: Some(dh_worker::proto_map::machine_config_to_proto(&config)),
                entropy_seed: vec![0xD5; 32],
            }))
            .await
            .map_err(|e| format!("CreateVm detchannel frames: {e}"))?
            .into_inner();
        let lease = created
            .lease
            .ok_or_else(|| "CreateVm returned no lease".to_string())?;

        let initial_snapshot = svc
            .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                lease: Some(lease.clone()),
                seal_input_log: Some(true),
                capture: None,
            }))
            .await
            .map_err(|e| format!("initial TakeSnapshot detchannel frames: {e}"))?
            .into_inner();
        let initial_ref = initial_snapshot
            .snapshot
            .ok_or_else(|| "initial detchannel snapshot returned no snapshot ref".to_string())?;

        let first_run = svc
            .run(Request::new(proto::RunRequest {
                lease: Some(lease.clone()),
                until: Some(proto::run_request::Until::FrameBudget(FIRST_FRAMES as u32)),
                hard_icount_cap: DETCHANNEL_FRAME_HARD_CAP,
                capture: None,
            }))
            .await
            .map_err(|e| format!("Run detchannel frame budget: {e}"))?
            .into_inner();
        assert_worker_frame_budget(&first_run, FIRST_FRAMES, "first detchannel run")?;
        let first_sdk_frames =
            stream_frame_mark_events(&svc, &lease, &first_expected, "first detchannel run").await?;

        let first_snapshot = svc
            .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                lease: Some(lease.clone()),
                seal_input_log: Some(true),
                capture: None,
            }))
            .await
            .map_err(|e| format!("TakeSnapshot detchannel frame budget: {e}"))?
            .into_inner();
        let first_ref = first_snapshot.snapshot.clone().ok_or_else(|| {
            "first detchannel frame snapshot returned no snapshot ref".to_string()
        })?;

        let restored = svc
            .restore_snapshot(Request::new(proto::RestoreSnapshotRequest {
                snapshot: Some(first_ref.clone()),
                entropy_seed: Vec::new(),
            }))
            .await
            .map_err(|e| format!("RestoreSnapshot detchannel frame boundary: {e}"))?
            .into_inner();
        let restored_frame_counter = restored.frame_counter;
        let restored_lease = restored
            .lease
            .ok_or_else(|| "RestoreSnapshot returned no lease".to_string())?;

        let second_expected = expected_frame_table(restored_frame_counter, AFTER_RESTORE_FRAMES)?;
        let second_run = svc
            .run(Request::new(proto::RunRequest {
                lease: Some(restored_lease.clone()),
                until: Some(proto::run_request::Until::FrameBudget(
                    AFTER_RESTORE_FRAMES as u32,
                )),
                hard_icount_cap: DETCHANNEL_FRAME_HARD_CAP,
                capture: None,
            }))
            .await
            .map_err(|e| format!("Run restored detchannel frame budget: {e}"))?
            .into_inner();
        assert_worker_frame_budget(&second_run, AFTER_RESTORE_FRAMES, "restored detchannel run")?;
        let second_sdk_frames = stream_frame_mark_events(
            &svc,
            &restored_lease,
            &second_expected,
            "restored detchannel run",
        )
        .await?;

        let second_snapshot = svc
            .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                lease: Some(restored_lease.clone()),
                seal_input_log: Some(true),
                capture: None,
            }))
            .await
            .map_err(|e| format!("TakeSnapshot restored detchannel frame budget: {e}"))?
            .into_inner();

        let _ = svc
            .destroy_vm(Request::new(proto::DestroyVmRequest {
                lease: Some(restored_lease),
            }))
            .await;
        let _ = svc
            .destroy_vm(Request::new(proto::DestroyVmRequest { lease: Some(lease) }))
            .await;

        Ok::<_, String>((
            first_run,
            first_sdk_frames,
            first_snapshot,
            restored_frame_counter,
            second_run,
            second_sdk_frames,
            second_snapshot,
            initial_ref,
            first_ref,
        ))
    })?;

    assert_eq!(
        first_snapshot.frame_counter, first_end,
        "first detchannel frame-budget snapshot must persist the absolute pv-pad counter"
    );
    assert_eq!(
        restored_frame_counter, first_end,
        "RestoreSnapshot must report the PADD-restored absolute frame counter"
    );
    let first_log = input_log_payload(&store, &first_snapshot.input_log_id)?;
    let first_replay = rt.block_on(common::verify_replay_done(
        &svc,
        initial_ref,
        first_snapshot.input_log_id.clone(),
    ))?;
    assert_eq!(
        first_replay.total_icount, first_run.icount,
        "first detchannel replay must cover the recorded segment"
    );
    let first_marks = frame_marks(&first_log);
    assert_strict_frame_table(&first_marks, &first_expected);
    assert_sdk_frames_match_frame_marks(&first_sdk_frames, &first_marks, 0, "first detchannel run");

    let second_expected = expected_frame_table(first_end, AFTER_RESTORE_FRAMES)?;
    let second_end = *second_expected
        .last()
        .ok_or_else(|| "second expected frame table is empty".to_string())?;
    assert_eq!(
        second_snapshot.frame_counter, second_end,
        "restored detchannel frame-budget snapshot must continue the absolute pv-pad counter"
    );
    let second_log = input_log_payload(&store, &second_snapshot.input_log_id)?;
    let second_replay = rt.block_on(common::verify_replay_done(
        &svc,
        first_ref,
        second_snapshot.input_log_id.clone(),
    ))?;
    assert_eq!(
        second_replay.total_icount,
        second_run.icount - first_run.icount,
        "restored detchannel replay must cover the restored segment delta"
    );
    let second_marks = frame_marks(&second_log);
    assert_strict_frame_table(&second_marks, &second_expected);
    assert_sdk_frames_match_frame_marks(
        &second_sdk_frames,
        &second_marks,
        first_run.icount,
        "restored detchannel run",
    );

    eprintln!(
        "detchannel-frame-budget first_icount={} sdk_frames={:?} first_marks={:?} restored_icount={} restored_sdk_frames={:?} restored_marks={:?}",
        first_run.icount,
        first_sdk_frames,
        first_marks,
        second_run.icount,
        second_sdk_frames,
        second_marks
    );

    Ok(())
}

fn config() -> MachineConfig {
    MachineConfig::new(
        MEM,
        [0xF5; 32],
        BootSpec::Elf {
            kernel_hash: [0xF5; 32],
            cmdline: Vec::new(),
        },
    )
}

fn detchannel_frame_machine_config(
    base_hash: [u8; 32],
    kernel_hash: [u8; 32],
    cpuid_table: Vec<dh_vmm::config::CpuidLeaf>,
) -> MachineConfig {
    let mut config = MachineConfig::new(
        MEM,
        base_hash,
        BootSpec::Elf {
            kernel_hash,
            cmdline: Vec::new(),
        },
    );
    config.cpuid_table = cpuid_table;
    config.device_set = vec![
        dh_devices::detchannel::DEVICE_ID_DETCHANNEL,
        dh_devices::clock::DEVICE_ID_PV_CLOCK,
        dh_devices::pad::DEVICE_ID_PV_PAD,
        dh_devices::entropy::DEVICE_ID_PV_ENTROPY,
        dh_devices::serial::DEVICE_ID_DEBUG_SERIAL,
    ];
    config
}

fn write_cache_blob(root: &Path, bytes: &[u8]) -> TestResult<[u8; 32]> {
    let hash = *blake3::hash(bytes).as_bytes();
    std::fs::write(
        root.join(dh_worker::image_resolver::cache_key(&hash)),
        bytes,
    )
    .map_err(|e| format!("write image-cache blob: {e}"))?;
    Ok(hash)
}

fn nop_rom() -> Vec<u8> {
    let mut rom = vec![0xEA; 32 * 1024];
    rom[0x7ffc] = 0x00;
    rom[0x7ffd] = 0x80;
    rom
}

fn hex_hash(bytes: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for b in bytes {
        use std::fmt::Write as _;
        write!(&mut out, "{b:02x}").expect("writing to String cannot fail");
    }
    out
}

fn frame_bus() -> MmioBus {
    let mut bus = MmioBus::new();
    bus.register(PV_PAD_BASE, Box::new(PvPad::new())).unwrap();
    // Entropy is mandatory for DHSNAP ENTR v2, even though fake_frames
    // itself never draws from the pv-entropy device.
    bus.register(0xD000_3000, Box::new(PvEntropy::new()))
        .unwrap();
    bus
}

fn header_for(snapshot_ref: &snapstore_types::SnapshotRef, cfg_hash: [u8; 32]) -> SegmentHeader {
    SegmentHeader {
        base_snapshot_id: snapshot_ref.to_bytes(),
        entropy_seed: [0; 32],
        machine_config_hash: cfg_hash,
        clock_num: 1,
        clock_den: 1,
        encoder_fingerprint: 0,
    }
}

fn zero_header() -> SegmentHeader {
    SegmentHeader {
        base_snapshot_id: [0; 32],
        entropy_seed: [0; 32],
        machine_config_hash: [0; 32],
        clock_num: 1,
        clock_den: 1,
        encoder_fingerprint: 0,
    }
}

fn read_pad_frame_counter(bus: &mut MmioBus) -> u32 {
    let mut log = LogWriter::new(zero_header());
    let mut mem = VecGuestMem(vec![0u8; 4]);
    let mut entropy = DetEntropy::from_seed([0; 32]);
    let mut irqs = Vec::new();
    let mut ctx = DevCtx::new(0, 0, &mut log, &mut mem, &mut entropy, &mut irqs);
    let mut buf = [0u8; 4];
    bus.read(PV_PAD_BASE + REG_FRAME_COUNTER, &mut buf, &mut ctx)
        .unwrap();
    u32::from_le_bytes(buf)
}

fn setup_counter() -> InstRetired {
    let counter = InstRetired::open_for_current_thread().unwrap();
    counter
        .route_overflow_to_thread(gettid(), dh_vmm::run::kick_signal())
        .unwrap();
    counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
    counter.reset().unwrap();
    counter.enable().unwrap();
    counter
}

fn run_frames(
    slot: &mut SlotVm,
    counter: &InstRetired,
    rail: &mut DeviceRail<VmMem>,
    cfg: &MachineConfig,
    chain: &mut StateHashChain,
    frames: u64,
) -> SegmentOutcome {
    let start = counter.read().unwrap();
    let pause = AtomicBool::new(false);
    let mut seg = Segment {
        slot,
        counter,
        chain,
        config: cfg,
        start_icount: start,
        injections: &[],
        timer: None,
        pause: &pause,
        sdk_events: None,
        hash_device_sections: None,
    };
    let out = run_segment(
        &mut seg,
        Until::FrameBudget {
            frames,
            hard_cap: FRAME_HARD_CAP,
        },
        &mut || false,
        &mut |exit| {
            let icount = counter
                .read()
                .map_err(|e| BoundaryError::Exit(format!("counter: {e:?}")))?;
            rail.service_exit(icount, exit)
        },
    )
    .unwrap();
    assert_eq!(out.reason, StopReason::BudgetReached);
    assert_eq!(out.frames_elapsed, frames);
    out
}

fn frame_marks(log: &[u8]) -> Vec<(u64, u32)> {
    LogReader::parse(log)
        .unwrap()
        .aux()
        .filter_map(|rec| match rec.body() {
            RecordBody::FrameMark { frame_index } => Some((rec.icount(), frame_index)),
            _ => None,
        })
        .collect()
}

fn resolve_at_frame(marks: &[(u64, u32)], target: u32) -> Option<u64> {
    marks
        .iter()
        .find_map(|(icount, frame)| (*frame == target).then_some(*icount))
}

fn assert_strict_frame_table(marks: &[(u64, u32)], expected_frames: &[u32]) {
    let frames: Vec<u32> = marks.iter().map(|(_, frame)| *frame).collect();
    assert_eq!(frames, expected_frames);
    assert!(
        marks.windows(2).all(|w| w[0].0 < w[1].0),
        "FRAME_MARK icounts must be strictly increasing: {marks:?}"
    );
}

fn assert_sdk_frames_match_frame_marks(
    sdk_frames: &[(u64, u32)],
    frame_marks: &[(u64, u32)],
    segment_base_icount: u64,
    label: &str,
) {
    let expected: Vec<(u64, u32)> = frame_marks
        .iter()
        .map(|(icount, frame)| (segment_base_icount.saturating_add(*icount), *frame))
        .collect();
    assert_eq!(
        sdk_frames,
        expected.as_slice(),
        "{label}: SDK FrameMark events must be drained at the matching FRAME_COUNTER boundary"
    );
}

fn expected_frame_table(start_frame: u32, frames: u64) -> TestResult<Vec<u32>> {
    let frames =
        u32::try_from(frames).map_err(|_| format!("frame count {frames} does not fit u32"))?;
    let end = start_frame
        .checked_add(frames)
        .ok_or_else(|| format!("frame range overflows u32: start={start_frame} count={frames}"))?;
    Ok((start_frame + 1..=end).collect())
}

fn assert_worker_frame_budget(
    run: &proto::RunResponse,
    expected_frames: u64,
    label: &str,
) -> TestResult<()> {
    if run.reason != i32::from(proto::StopReason::BudgetReached) {
        return Err(format!(
            "{label} stopped with reason {}, expected BudgetReached",
            run.reason
        ));
    }
    if run.frames_elapsed != expected_frames {
        return Err(format!(
            "{label} frames_elapsed {}, expected {expected_frames}",
            run.frames_elapsed
        ));
    }
    Ok(())
}

async fn stream_frame_mark_events(
    svc: &dh_worker::service::WorkerService,
    lease: &proto::Lease,
    expected_frames: &[u32],
    label: &str,
) -> TestResult<Vec<(u64, u32)>> {
    let mut stream = svc
        .stream_guest_events(Request::new(proto::StreamGuestEventsRequest {
            lease: Some(lease.clone()),
            streams: vec![detguest_wire::record::EventKind::FrameMark as u32],
        }))
        .await
        .map_err(|e| format!("{label}: StreamGuestEvents(FrameMark): {e}"))?
        .into_inner();
    let mut frames = Vec::new();
    while let Some(event) = stream.next().await {
        let event = event.map_err(|e| format!("{label}: stream FrameMark event: {e}"))?;
        if event.stream != detguest_wire::record::EventKind::FrameMark as u32 {
            return Err(format!(
                "{label}: unexpected stream {}, expected FrameMark",
                event.stream
            ));
        }
        if event.payload.len() != 8 {
            return Err(format!(
                "{label}: FrameMark payload length {}, expected 8",
                event.payload.len()
            ));
        }
        let frame = u32::from_le_bytes(
            event.payload[0..4]
                .try_into()
                .map_err(|_| format!("{label}: FrameMark payload prefix"))?,
        );
        frames.push((event.icount, frame));
    }
    let actual: Vec<u32> = frames.iter().map(|(_, frame)| *frame).collect();
    if actual != expected_frames {
        return Err(format!(
            "{label}: streamed FrameMark frames {:?}, expected {:?}",
            actual, expected_frames
        ));
    }
    if !frames.windows(2).all(|w| w[0].0 < w[1].0) {
        return Err(format!(
            "{label}: streamed FrameMark icounts must increase strictly: {frames:?}"
        ));
    }
    Ok(frames)
}

async fn stream_guest_event_summaries(
    svc: &dh_worker::service::WorkerService,
    lease: &proto::Lease,
    limit: usize,
) -> TestResult<Vec<String>> {
    let mut stream = svc
        .stream_guest_events(Request::new(proto::StreamGuestEventsRequest {
            lease: Some(lease.clone()),
            streams: Vec::new(),
        }))
        .await
        .map_err(|e| format!("StreamGuestEvents(any): {e}"))?
        .into_inner();
    let mut out = Vec::new();
    while out.len() < limit {
        let Some(event) = stream.next().await else {
            break;
        };
        let event = event.map_err(|e| format!("stream guest event: {e}"))?;
        let printable: String = event
            .payload
            .iter()
            .map(|&b| {
                if (0x20..0x7f).contains(&b) {
                    b as char
                } else {
                    '.'
                }
            })
            .collect();
        out.push(format!(
            "stream={} icount={} vns={} payload_len={} payload={printable}",
            event.stream,
            event.icount,
            event.vns,
            event.payload.len(),
        ));
    }
    if out.is_empty() {
        out.push("no buffered guest events".to_string());
    }
    Ok(out)
}

fn input_log_payload(
    store: &snapstore_client::blocking::SnapstoreClient,
    input_log_id: &[u8],
) -> TestResult<Vec<u8>> {
    let id: [u8; 32] = input_log_id
        .try_into()
        .map_err(|_| "input log id must be 32 bytes".to_string())?;
    let container = store
        .get_input_log(snapstore_types::LogId::from_bytes(id))
        .map_err(|e| format!("get_input_log: {e}"))?;
    let decoded = InputLogContainer::decode(&container)
        .map_err(|e| format!("input log container decode: {e}"))?;
    Ok(decoded.payload().to_vec())
}

#[test]
fn m5_accept_frame_budget_and_at_frame_absolute_across_restore() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }

    dh_vmm::run::install_kick_handler().unwrap();
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let sys = KvmSystem::open().unwrap();
    let mut slot = sys.create_slot_vm(MEM).unwrap();
    dh_vmm::boot::load_and_enter(&slot, nanokernel::fake_frames_elf(), b"").unwrap();
    let counter = setup_counter();

    let cfg = config();
    let cfg_hash = cfg.config_hash().unwrap();
    let boot_bus = frame_bus();
    let boot_entropy = DetEntropy::from_seed([0x5F; 32]);
    let mut chain = StateHashChain::new(&cfg_hash, &[0; 32]);
    let root = take_snapshot(
        &slot,
        SlotState::Paused,
        &boot_bus,
        &boot_entropy,
        &cfg,
        BoundaryState {
            icount: 0,
            vns: 0,
            epoch_index: 0,
            hash_chain: chain.value(),
            agenda_empty: true,
        },
        PageSource::Full,
        &store,
    )
    .expect("root snapshot")
    .snapshot_ref;

    let mut rail = DeviceRail::new(
        boot_bus,
        boot_entropy,
        LogWriter::new(header_for(&root, cfg_hash)),
        VmMem(slot.guest_mem.clone()),
    );
    let first = run_frames(
        &mut slot,
        &counter,
        &mut rail,
        &cfg,
        &mut chain,
        FIRST_FRAMES,
    );
    assert_eq!(
        read_pad_frame_counter(&mut rail.bus),
        FIRST_FRAMES as u32,
        "live pv-pad counter at the first boundary"
    );

    let mid = take_snapshot(
        &slot,
        SlotState::Paused,
        &rail.bus,
        &rail.entropy,
        &cfg,
        BoundaryState {
            icount: first.boundary.icount,
            vns: first.vns,
            epoch_index: first.boundary.icount / cfg.epoch_len,
            hash_chain: first.state_hash,
            agenda_empty: true,
        },
        PageSource::Full,
        &store,
    )
    .expect("mid snapshot")
    .snapshot_ref;
    let first_log = rail.seal(&first, mid.to_bytes()).expect("seal first");
    let first_marks = frame_marks(&first_log);
    assert_strict_frame_table(&first_marks, &[1, 2, 3]);
    let at_frame_icount = resolve_at_frame(&first_marks, AT_FRAME_TARGET)
        .expect("at_frame target must resolve through FRAME_MARK");
    assert_eq!(
        first_marks[(AT_FRAME_TARGET - 1) as usize].0,
        at_frame_icount,
        "at_frame resolves to the icount of the matching absolute frame"
    );
    assert_eq!(
        first_marks.last().unwrap().0,
        first.boundary.icount,
        "FrameBudget stopped on the final frame mark it counted"
    );

    let mut restored_slot = sys.create_slot_vm(MEM).unwrap();
    let mut restored_bus = frame_bus();
    let restored = restore_snapshot(
        &restored_slot,
        SlotState::Paused,
        &mut restored_bus,
        &cfg,
        mid.clone(),
        Some(&counter),
        None,
        &store,
    )
    .expect("restore mid snapshot");
    assert_eq!(
        counter.read().unwrap(),
        0,
        "restore re-zeroes segment counter"
    );
    assert_eq!(
        restored.cumulative_icount, first.boundary.icount,
        "TIME cumulative icount carried through restore"
    );
    assert_eq!(
        read_pad_frame_counter(&mut restored_bus),
        FIRST_FRAMES as u32,
        "PADD restored the absolute frame counter"
    );

    let mut restored_chain = restored.chain;
    let mut restored_rail = DeviceRail::new(
        restored_bus,
        restored.entropy,
        LogWriter::new(header_for(&mid, cfg_hash)),
        VmMem(restored_slot.guest_mem.clone()),
    );
    let second = run_frames(
        &mut restored_slot,
        &counter,
        &mut restored_rail,
        &cfg,
        &mut restored_chain,
        AFTER_RESTORE_FRAMES,
    );
    let second_log = restored_rail
        .seal(&second, [0; 32])
        .expect("seal restored segment");
    let second_marks = frame_marks(&second_log);
    assert_strict_frame_table(&second_marks, &[4, 5]);
    assert_eq!(
        second_marks.last().unwrap().0,
        second.boundary.icount,
        "restored FrameBudget stopped on its Nth frame mark"
    );

    let restored_target = (FIRST_FRAMES + AFTER_RESTORE_FRAMES) as u32;
    assert_eq!(
        resolve_at_frame(&second_marks, restored_target),
        Some(second.boundary.icount),
        "post-restore at_frame uses absolute FRAME_COUNTER values"
    );
    assert!(
        resolve_at_frame(&second_marks, AFTER_RESTORE_FRAMES as u32).is_none(),
        "a segment-relative frame number would be wrong after restore"
    );
}
