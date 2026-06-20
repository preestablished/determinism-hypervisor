//! M5 ACCEPT (bead czq): record the polling pv-net loopback guest
//! (TX doorbell -> AUX NET_TX, immediate host loopback -> canonical
//! NET_RX), then replay the sealed segment from `(snapshot, DHILOG)` and
//! require byte-identical reseal plus the replayed guest RAM payload.
//!
//! HARDWARE-GATED: KVM lane only; self-skips elsewhere.

#![cfg(target_arch = "x86_64")]

mod common;

use std::sync::atomic::AtomicBool;

use common::{gettid, kvm_available, spawn_store_blocking, TestResult, VmMem};
use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};
use dh_devices::entropy::{DetEntropy, PvEntropy};
use dh_devices::net::{PvNet, PV_NET_BASE, REG_TX_DOORBELL};
use dh_devices::MmioBus;
use dh_inputlog::dhilog::{LogWriter, SegmentHeader};
use dh_inputlog::reader::{LogReader, RecordBody};
use dh_proto::v1 as proto;
use dh_proto::v1::hypervisor_worker_server::HypervisorWorker;
use dh_snapshot::dhsnap::tag;
use dh_vmm::boundary::BoundaryError;
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::hash::StateHashChain;
use dh_vmm::kvm::{KvmSystem, SlotVm};
use dh_vmm::recording::DeviceRail;
use dh_vmm::runctl::{
    run_segment_with_epoch_options, run_segment_with_epochs, RunOptions, Segment, SegmentOutcome,
    StopReason, Until,
};
use dh_vmm::SlotState;
use dh_worker::replay_engine::{replay_segment, ReplayOutcome};
use dh_worker::runtime::runtime_hash_device_sections;
use dh_worker::snapshot_engine::{take_snapshot, BoundaryState, PageSource};
use kvm_ioctls::VcpuExit;
use snapstore_types::SnapshotRef;
use tonic::Request;
use vm_memory::{Bytes, GuestAddress};

const MEM: u64 = 16 << 20;
const HARD_CAP: u64 = 1_000_000;
const EPOCH_LEN: u64 = 64;
const NET_TX_DOORBELL_GPA: u64 = PV_NET_BASE + REG_TX_DOORBELL;
const LINUX_IO_FRAMES: u32 = 1;
const LINUX_FRAME_HARD_CAP: u64 = 50_000_000;
const META_IO_MAGIC_OFF: u64 = 32;
const META_IO_PROOF_LEN: u64 = 24;
const BLKO_DIRTY_COUNT_OFF: usize = 32;

#[test]
#[ignore = "M9 Linux acceptance: requires KVM dirty-ring support and staged DH_M9_* artifacts"]
fn linux_pvblk_io_loopback_records_and_replays() -> TestResult<()> {
    let Some(ready) =
        common::m9_linux_ready_snapshot("m5_net_loopback::linux_pvblk_io_loopback", 2)?
    else {
        return Ok(());
    };
    let ready_blko = common::snapshot_section(&ready.store, &ready.ready_snapshot_ref, tag::BLKO)?;
    assert_eq!(
        blko_dirty_clusters(&ready_blko)?,
        0,
        "READY snapshot should not have pv-blk overlay writes before Start"
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("test runtime: {e}"))?;
    let (run, io_snapshot, meta_proof, verify_done) = rt.block_on(async {
        let run = ready
            .svc
            .run(Request::new(proto::RunRequest {
                lease: Some(ready.lease.clone()),
                until: Some(proto::run_request::Until::FrameBudget(LINUX_IO_FRAMES)),
                hard_icount_cap: LINUX_FRAME_HARD_CAP,
                capture: None,
            }))
            .await
            .map_err(|e| format!("Run Linux pv-blk IO frame: {e}"))?
            .into_inner();
        if run.reason != i32::from(proto::StopReason::BudgetReached) {
            return Err(format!(
                "Linux pv-blk IO run stopped with {}, expected BudgetReached",
                run.reason
            ));
        }
        if run.frames_elapsed != u64::from(LINUX_IO_FRAMES) {
            return Err(format!(
                "Linux pv-blk IO run frames_elapsed {}, expected {LINUX_IO_FRAMES}",
                run.frames_elapsed
            ));
        }

        let meta = ready
            .svc
            .read_guest_memory(Request::new(proto::ReadGuestMemoryRequest {
                lease: Some(ready.lease.clone()),
                ranges: Vec::new(),
                region_ranges: vec![proto::RegionRange {
                    region: "meta".into(),
                    layout_version: 1,
                    offset: META_IO_MAGIC_OFF,
                    len: META_IO_PROOF_LEN,
                }],
            }))
            .await
            .map_err(|e| format!("ReadGuestMemory meta IO proof: {e}"))?
            .into_inner();
        let meta_proof = meta
            .chunks
            .first()
            .cloned()
            .ok_or_else(|| "ReadGuestMemory returned no meta IO proof chunk".to_string())?;

        let io_snapshot = ready
            .svc
            .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                lease: Some(ready.lease.clone()),
                seal_input_log: Some(true),
                capture: None,
            }))
            .await
            .map_err(|e| format!("TakeSnapshot Linux pv-blk IO segment: {e}"))?
            .into_inner();
        let verify_done = common::verify_replay_done(
            &ready.svc,
            ready.ready_snapshot_ref.clone(),
            io_snapshot.input_log_id.clone(),
        )
        .await?;

        let _ = ready
            .svc
            .destroy_vm(Request::new(proto::DestroyVmRequest {
                lease: Some(ready.lease.clone()),
            }))
            .await;

        Ok::<_, String>((run, io_snapshot, meta_proof, verify_done))
    })?;

    let checksum = assert_meta_io_proof(&meta_proof)?;
    let io_ref = io_snapshot
        .snapshot
        .as_ref()
        .ok_or_else(|| "Linux pv-blk IO snapshot returned no snapshot ref".to_string())?;
    let io_blko = common::snapshot_section(&ready.store, io_ref, tag::BLKO)?;
    assert_ne!(
        ready_blko, io_blko,
        "guest-driven pv-blk write must change the BLKO snapshot section"
    );
    assert!(
        blko_dirty_clusters(&io_blko)? > 0,
        "Linux pv-blk IO snapshot must contain dirty overlay clusters"
    );
    let live_end_hash = io_snapshot
        .state_hash
        .as_ref()
        .ok_or_else(|| "Linux pv-blk IO snapshot returned no state hash".to_string())?;
    assert_eq!(
        verify_done
            .end_state_hash
            .ok_or_else(|| "VerifyReplay returned no end state hash".to_string())?
            .hash,
        live_end_hash.hash,
        "VerifyReplay must reproduce the guest-driven pv-blk IO segment end hash"
    );

    eprintln!(
        "linux-pvblk-io run_icount={} frame_counter={} checksum={checksum:#x} blko_dirty_clusters={}",
        run.icount,
        io_snapshot.frame_counter,
        blko_dirty_clusters(&io_blko)?
    );

    Ok(())
}

fn config() -> MachineConfig {
    let mut c = MachineConfig::new(
        MEM,
        [0xC7; 32],
        BootSpec::Elf {
            kernel_hash: [0xC7; 32],
            cmdline: Vec::new(),
        },
    );
    c.epoch_len = EPOCH_LEN;
    // Keep the short loopback guest's epoch grid dense enough to record
    // EPOCH_HASH records, while keeping the margin above measured skid.
    c.skid_margin = 128;
    c.resync_slack = 128;
    c
}

fn net_bus() -> MmioBus {
    let mut bus = MmioBus::new();
    bus.register(PV_NET_BASE, Box::new(PvNet::new())).unwrap();
    bus.register(0xD000_3000, Box::new(PvEntropy::new()))
        .unwrap();
    bus
}

fn blko_dirty_clusters(section: &[u8]) -> TestResult<u32> {
    let bytes = section
        .get(BLKO_DIRTY_COUNT_OFF..BLKO_DIRTY_COUNT_OFF + 4)
        .ok_or_else(|| format!("BLKO section too short: {} bytes", section.len()))?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}

fn assert_meta_io_proof(bytes: &[u8]) -> TestResult<u64> {
    if bytes.len() != META_IO_PROOF_LEN as usize {
        return Err(format!(
            "meta IO proof length {}, expected {META_IO_PROOF_LEN}",
            bytes.len()
        ));
    }
    if &bytes[..8] != b"PVBLKIO1" {
        return Err(format!("meta IO proof missing magic: {:?}", &bytes[..8]));
    }
    let frame = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    if frame != 0 {
        return Err(format!("meta IO proof frame {frame}, expected 0"));
    }
    let checksum = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
    if checksum == 0 {
        return Err("meta IO proof checksum must be nonzero".into());
    }
    Ok(checksum)
}

fn header_for(snap: &SnapshotRef, cfg_hash: [u8; 32]) -> SegmentHeader {
    SegmentHeader {
        base_snapshot_id: snap.to_bytes(),
        entropy_seed: [0; 32],
        machine_config_hash: cfg_hash,
        clock_num: 1,
        clock_den: 1,
        encoder_fingerprint: 0,
    }
}

struct Recording {
    snapshot_ref: SnapshotRef,
    log: Vec<u8>,
    cfg: MachineConfig,
    end_state_hash: [u8; 32],
    end_icount: u64,
    epoch_hashes: u64,
}

fn record(store: &snapstore_client::blocking::SnapstoreClient) -> Recording {
    dh_vmm::run::install_kick_handler().unwrap();
    let sys = KvmSystem::open().unwrap();
    let mut slot = sys.create_slot_vm(MEM).unwrap();
    dh_vmm::boot::load_and_enter(&slot, nanokernel::net_loopback_elf(), b"").unwrap();

    let counter = InstRetired::open_for_current_thread().unwrap();
    counter
        .route_overflow_to_thread(gettid(), dh_vmm::run::kick_signal())
        .unwrap();
    counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
    counter.reset().unwrap();
    counter.enable().unwrap();

    let cfg = config();
    let cfg_hash = cfg.config_hash().unwrap();
    let mut chain = StateHashChain::new(&cfg_hash, &[0; 32]);
    let bus = net_bus();
    let entropy = DetEntropy::from_seed([0x42; 32]);

    let snap = take_snapshot(
        &slot,
        SlotState::Paused,
        &bus,
        &entropy,
        &cfg,
        BoundaryState {
            icount: 0,
            vns: 0,
            epoch_index: 0,
            hash_chain: chain.value(),
            agenda_empty: true,
        },
        PageSource::Full,
        store,
    )
    .expect("base snapshot")
    .snapshot_ref;

    let expected_frame = nanokernel::net_loopback_frame();
    let rail = std::cell::RefCell::new(DeviceRail::new(
        bus,
        entropy,
        LogWriter::new(header_for(&snap, cfg_hash)),
        VmMem(slot.guest_mem.clone()),
    ));
    let pause = AtomicBool::new(false);
    let serial = std::cell::RefCell::new(Vec::new());
    let pending_rx = std::cell::RefCell::new(None::<(u64, Vec<u8>)>);

    let rx_boundary: SegmentOutcome = {
        let mut seg = Segment {
            slot: &mut slot,
            counter: &counter,
            chain: &mut chain,
            config: &cfg,
            start_icount: 0,
            injections: &[],
            timer: None,
            pause: &pause,
            sdk_events: None,
            hash_device_sections: Some(&|| {
                let rail_ref = rail.borrow();
                runtime_hash_device_sections(&rail_ref.bus, &rail_ref.lapic)
            }),
        };
        let counter_ref = &counter;
        run_segment_with_epoch_options(
            &mut seg,
            Until::Goal {
                poll_period: 1,
                hard_cap: HARD_CAP,
            },
            // This quantum stops at the next canonical NET_RX landing, not
            // a segment END; replay must not inherit a hidden final-stop link.
            RunOptions {
                hash_final_stop: false,
                ..RunOptions::default()
            },
            &mut || pending_rx.borrow().is_some(),
            &mut |exit: VcpuExit| {
                let loopback =
                    matches!(&exit, VcpuExit::MmioWrite(gpa, _) if *gpa == NET_TX_DOORBELL_GPA);
                let icount = counter_ref
                    .read()
                    .map_err(|e| BoundaryError::Exit(format!("counter: {e:?}")))?;
                let mut rail = rail.borrow_mut();
                rail.service_exit(icount, exit)?;
                serial.borrow_mut().extend(rail.serial.take_output());
                if loopback {
                    let frame = rail
                        .drain_net_tx()
                        .map_err(|e| BoundaryError::Exit(format!("drain NET_TX: {e:?}")))?
                        .ok_or_else(|| {
                            BoundaryError::Exit("NET_TX doorbell drained no frame".into())
                        })?;
                    if frame != expected_frame {
                        return Err(BoundaryError::Exit("NET_TX frame bytes drifted".into()));
                    }
                    let rx_icount = icount
                        .checked_add(1)
                        .ok_or_else(|| BoundaryError::Exit("NET_RX icount overflow".into()))?;
                    if pending_rx
                        .borrow_mut()
                        .replace((rx_icount, frame))
                        .is_some()
                    {
                        return Err(BoundaryError::Exit(
                            "net_loopback guest transmitted more than once".into(),
                        ));
                    }
                }
                Ok(())
            },
            &mut |idx, boundary, value, _slot| {
                rail.borrow_mut()
                    .log_epoch_hash(idx, boundary.icount, value)
                    .map_err(|e| BoundaryError::Exit(format!("epoch log: {e:?}")))
            },
        )
        .unwrap()
    };
    assert_eq!(
        rx_boundary.reason,
        StopReason::GoalSatisfied,
        "first quantum stops at the deferred NET_RX landing boundary"
    );
    let (rx_icount, frame) = pending_rx.borrow_mut().take().expect("pending NET_RX");
    assert_eq!(
        rx_icount, rx_boundary.boundary.icount,
        "NET_RX must land after the hash-chain boundary at that icount"
    );
    let vector = rail
        .borrow_mut()
        .apply_net_rx(rx_icount, rx_boundary.boundary.rip, &frame)
        .expect("apply NET_RX");
    assert!(
        vector.is_none(),
        "net_loopback guest must keep RX_VECTOR disabled"
    );

    let outcome: SegmentOutcome = {
        let start = counter.read().unwrap();
        let mut seg = Segment {
            slot: &mut slot,
            counter: &counter,
            chain: &mut chain,
            config: &cfg,
            start_icount: start,
            injections: &[],
            timer: None,
            pause: &pause,
            sdk_events: None,
            hash_device_sections: Some(&|| {
                let rail_ref = rail.borrow();
                runtime_hash_device_sections(&rail_ref.bus, &rail_ref.lapic)
            }),
        };
        let counter_ref = &counter;
        run_segment_with_epochs(
            &mut seg,
            Until::Goal {
                poll_period: 1,
                hard_cap: HARD_CAP,
            },
            &mut || serial.borrow().as_slice() == nanokernel::NET_LOOPBACK_OK_SEQUENCE,
            &mut |exit: VcpuExit| {
                if matches!(&exit, VcpuExit::MmioWrite(gpa, _) if *gpa == NET_TX_DOORBELL_GPA) {
                    return Err(BoundaryError::Exit(
                        "unexpected second NET_TX doorbell".into(),
                    ));
                }
                let icount = counter_ref
                    .read()
                    .map_err(|e| BoundaryError::Exit(format!("counter: {e:?}")))?;
                let mut rail = rail.borrow_mut();
                rail.service_exit(icount, exit)?;
                serial.borrow_mut().extend(rail.serial.take_output());
                Ok(())
            },
            &mut |idx, boundary, value, _slot| {
                rail.borrow_mut()
                    .log_epoch_hash(idx, boundary.icount, value)
                    .map_err(|e| BoundaryError::Exit(format!("epoch log: {e:?}")))
            },
        )
        .unwrap()
    };
    assert_eq!(
        outcome.reason,
        StopReason::GoalSatisfied,
        "net_loopback should stop after the success serial byte, before HLT"
    );
    let serial = serial.into_inner();
    assert_eq!(
        serial,
        nanokernel::NET_LOOPBACK_OK_SEQUENCE,
        "serial progress (lowercase identifies the failing stage)"
    );
    assert!(
        rail.borrow().irqs.is_empty(),
        "polling guest queues no IRQs"
    );

    let end_state_hash = outcome.state_hash;
    let end_icount = outcome.boundary.icount;
    let log = rail.into_inner().seal(&outcome, [0; 32]).expect("seal");
    let reader = LogReader::parse(&log).expect("parse recording");
    assert!(reader.header().has_epoch_hashes());
    assert_eq!(reader.end().0, 2, "END reason is GoalSatisfied");
    assert_eq!(reader.end().1, end_state_hash);

    let net_rx: Vec<_> = reader
        .canonical()
        .filter_map(|rec| match rec.body() {
            RecordBody::NetRx { frame } => Some((rec.icount(), rec.boundary_rip(), frame)),
            _ => None,
        })
        .collect();
    assert_eq!(net_rx.len(), 1, "exactly one canonical NET_RX");
    assert_eq!(net_rx[0].1, rx_boundary.boundary.rip);
    assert_eq!(net_rx[0].2, expected_frame.as_slice());

    let net_tx: Vec<_> = reader
        .aux()
        .filter_map(|rec| match rec.body() {
            RecordBody::NetTx { len, digest8 } => Some((rec.icount(), len, digest8)),
            _ => None,
        })
        .collect();
    assert_eq!(net_tx.len(), 1, "exactly one AUX NET_TX");
    assert_eq!(
        net_tx[0].0 + 1,
        net_rx[0].0,
        "NET_RX lands at the first guest-visible boundary after TX"
    );
    assert_eq!(net_tx[0].1, nanokernel::NET_LOOPBACK_FRAME_LEN);
    assert_eq!(net_tx[0].2, LogWriter::digest8(&expected_frame));

    let epoch_hashes = reader
        .aux()
        .filter(|rec| matches!(rec.body(), RecordBody::EpochHash { .. }))
        .count() as u64;
    assert!(epoch_hashes > 0, "short run must still verify epoch hashes");

    Recording {
        snapshot_ref: snap,
        log,
        cfg,
        end_state_hash,
        end_icount,
        epoch_hashes,
    }
}

fn replay_once(
    store: &snapstore_client::blocking::SnapstoreClient,
    rec: &Recording,
) -> (SlotVm, ReplayOutcome) {
    dh_vmm::run::install_kick_handler().unwrap();
    let sys = KvmSystem::open().unwrap();
    let mut slot = sys.create_slot_vm(MEM).unwrap();
    let counter = InstRetired::open_for_current_thread().unwrap();
    counter
        .route_overflow_to_thread(gettid(), dh_vmm::run::kick_signal())
        .unwrap();
    counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
    counter.reset().unwrap();
    counter.enable().unwrap();

    let rail = DeviceRail::new(
        net_bus(),
        DetEntropy::from_seed([0; 32]),
        LogWriter::new(header_for(
            &rec.snapshot_ref,
            rec.cfg.config_hash().unwrap(),
        )),
        VmMem(slot.guest_mem.clone()),
    );
    let outcome = replay_segment(
        &mut slot,
        rail,
        &rec.cfg,
        rec.snapshot_ref.clone(),
        &counter,
        store,
        &rec.log,
    )
    .expect("replay must not diverge");
    (slot, outcome)
}

fn assert_replayed_guest_ram(slot: &SlotVm) {
    let expected = nanokernel::net_loopback_frame();
    let mut tx = vec![0u8; expected.len()];
    slot.guest_mem
        .read_slice(&mut tx, GuestAddress(nanokernel::NET_LOOPBACK_TX_GPA))
        .unwrap();
    assert_eq!(tx, expected, "guest TX frame changed");

    let mut rx = vec![0u8; expected.len()];
    slot.guest_mem
        .read_slice(&mut rx, GuestAddress(nanokernel::NET_LOOPBACK_RX_GPA))
        .unwrap();
    assert_eq!(rx, expected, "replayed NET_RX payload reached guest RAM");
}

#[test]
fn m5_net_rx_loopback_records_and_replays_bit_identically() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }

    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let rec = record(&store);
    let (slot, replay) = replay_once(&store, &rec);

    assert_eq!(replay.records_applied, 1, "the NET_RX record applied");
    assert_eq!(
        replay.epoch_hashes_verified, rec.epoch_hashes,
        "all recorded epoch hashes verified"
    );
    assert_eq!(replay.end_icount, rec.end_icount);
    assert_eq!(replay.end_state_hash, rec.end_state_hash);
    assert_eq!(replay.resealed, rec.log, "the reseal hammer");
    assert_replayed_guest_ram(&slot);
}
