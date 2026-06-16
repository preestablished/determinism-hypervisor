//! Replay-path joint tests (bead 39w): record a real pad-echo run from a
//! real snapshot through the recording rail, then drive a fresh slot
//! from `(snapshot, DHILOG)` — injections at the recorded icounts,
//! every EPOCH_HASH verified against the live chain, end_state_hash
//! checked, and the resealed log BYTE-IDENTICAL to the input. Replay
//! quantizes by record (one quantum per input + the tail), deliberately
//! unlike the recording's fixed 100k quanta — the absolute epoch grid
//! makes the hash sets match anyway.
//!
//! HARDWARE-GATED: kvm-intel lane + lab box; self-skips elsewhere.

#![cfg(target_arch = "x86_64")]

mod common;

use std::sync::atomic::AtomicBool;

use common::{gettid, kvm_available, spawn_store_blocking, VmMem};
use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};
use dh_devices::entropy::{DetEntropy, PvEntropy};
use dh_devices::pad::PvPad;
use dh_devices::MmioBus;
use dh_inputlog::dhilog::{LogWriter, SegmentHeader, DEVICE_ID_DETCHANNEL, EVENT_PIO_ANSWER};
use dh_verify::verify::VerifyProgress;
use dh_vmm::boundary::BoundaryError;
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::hash::StateHashChain;
use dh_vmm::kvm::{KvmSystem, SlotVm};
use dh_vmm::recording::DeviceRail;
use dh_vmm::runctl::{
    run_segment_with_epochs, run_segment_with_scheduled_inputs_frames_and_epochs, Segment,
    SegmentOutcome, StopReason, Until,
};
use dh_vmm::SlotState;
use dh_worker::replay_engine::{replay_segment, ReplayError};
use dh_worker::snapshot_engine::{take_snapshot, BoundaryState, PageSource};
use dh_worker::verify_replay::verify_replay;
use kvm_ioctls::VcpuExit;
use vm_memory::{Bytes, GuestAddress};

const MEM: u64 = 16 << 20;
const QUANTUM: u64 = 100_000;

fn config() -> MachineConfig {
    let mut c = MachineConfig::new(
        MEM,
        [0x77; 32],
        BootSpec::Elf {
            kernel_hash: [0x77; 32],
            cmdline: Vec::new(),
        },
    );
    c.epoch_len = 50_000; // several epochs per quantum; quanta stop on the grid
    c
}

/// The pad-echo recording bus: the pad the guest polls plus the entropy
/// device the snapshot engines REQUIRE on every bus.
fn record_bus() -> MmioBus {
    let mut bus = MmioBus::new();
    bus.register(dh_devices::pad::PV_PAD_BASE, Box::new(PvPad::new()))
        .unwrap();
    bus.register(0xD000_3000, Box::new(PvEntropy::new()))
        .unwrap();
    bus
}

struct Recording {
    snapshot_ref: snapstore_types::SnapshotRef,
    log: Vec<u8>,
    cfg: MachineConfig,
}

/// Boot pad_echo, snapshot at the boot boundary, then record three
/// quanta with PAD_SETs at the two inter-quantum boundaries — sealed
/// from the final outcome. `poison_ram` mutates guest RAM AFTER the
/// snapshot (host-side) so the recording's hashes belong to a machine
/// the snapshot does not describe — the divergence negative. (A
/// poisoned chain SEED would travel through TIME and stay
/// self-consistent; RAM divergence is the honest mismatch.)
fn record(store: &snapstore_client::blocking::SnapstoreClient, poison_ram: bool) -> Recording {
    dh_vmm::run::install_kick_handler().unwrap();
    let sys = KvmSystem::open().unwrap();
    let mut slot = sys.create_slot_vm(MEM).unwrap();
    dh_vmm::boot::load_and_enter(&slot, nanokernel::pad_echo_elf(), b"").unwrap();
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

    // The base snapshot at the boot boundary (counter at 0).
    let bus = record_bus();
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

    if poison_ram {
        slot.guest_mem
            .write_slice(&[0xDD; 64], GuestAddress(0x60_0000))
            .unwrap();
    }

    let rail = std::cell::RefCell::new(DeviceRail::new(
        bus,
        entropy,
        LogWriter::new(SegmentHeader {
            base_snapshot_id: snap.to_bytes(),
            entropy_seed: [0; 32],
            machine_config_hash: cfg_hash,
            clock_num: 1,
            clock_den: 1,
            encoder_fingerprint: 0,
        }),
        VmMem(slot.guest_mem.clone()),
    ));
    let pause = AtomicBool::new(false);

    let run_one = |slot: &mut SlotVm, chain: &mut StateHashChain| -> SegmentOutcome {
        let start = counter.read().unwrap();
        let out = {
            let mut seg = Segment {
                slot,
                counter: &counter,
                chain,
                config: &cfg,
                start_icount: start,
                injections: &[],
                timer: None,
                pause: &pause,
                sdk_events: None,
            };
            let counter_ref = &counter;
            run_segment_with_epochs(
                &mut seg,
                Until::IcountBudget(QUANTUM),
                &mut || false,
                &mut |exit: VcpuExit| {
                    let icount = counter_ref
                        .read()
                        .map_err(|e| BoundaryError::Exit(format!("{e:?}")))?;
                    rail.borrow_mut().service_exit(icount, exit)
                },
                &mut |idx, icount, value| {
                    rail.borrow_mut()
                        .log_epoch_hash(idx, icount, value)
                        .map_err(|e| BoundaryError::Exit(format!("epoch log: {e:?}")))
                },
            )
            .unwrap()
        };
        assert_eq!(out.reason, StopReason::BudgetReached);
        out
    };

    let o1 = run_one(&mut slot, &mut chain);
    rail.borrow_mut()
        .apply_pad_set(o1.boundary.icount, o1.boundary.rip, 0, 0xA1B2, 0)
        .unwrap();
    let mut pio = [0u8; 8];
    pio[..2].copy_from_slice(&0xD370u16.to_le_bytes());
    pio[4..].copy_from_slice(&0x1234u32.to_le_bytes());
    rail.borrow_mut()
        .apply_dev_event(
            o1.boundary.icount,
            o1.boundary.rip,
            DEVICE_ID_DETCHANNEL,
            EVENT_PIO_ANSWER,
            &pio,
        )
        .unwrap();
    let o2 = run_one(&mut slot, &mut chain);
    rail.borrow_mut()
        .apply_pad_set(o2.boundary.icount, o2.boundary.rip, 0, 0xC3D4, 1)
        .unwrap();
    let o3 = run_one(&mut slot, &mut chain);

    let log = rail.into_inner().seal(&o3, [0; 32]).expect("seal");
    Recording {
        snapshot_ref: snap,
        log,
        cfg,
    }
}

#[test]
fn replay_reproduces_the_recording_bit_identically() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let rec = record(&store, false);

    // Fresh machine for the replay leg (same thread, counter reset by
    // the restore inside replay_segment).
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
        record_bus(),
        DetEntropy::from_seed([0; 32]), // replaced by the restored PRNG
        LogWriter::new(SegmentHeader {
            base_snapshot_id: rec.snapshot_ref.to_bytes(),
            entropy_seed: [0; 32],
            machine_config_hash: rec.cfg.config_hash().unwrap(),
            clock_num: 1,
            clock_den: 1,
            encoder_fingerprint: 0,
        }),
        VmMem(slot.guest_mem.clone()),
    );

    let outcome = replay_segment(
        &mut slot,
        rail,
        &rec.cfg,
        rec.snapshot_ref.clone(),
        &counter,
        &store,
        &rec.log,
    )
    .expect("replay");

    assert_eq!(
        outcome.records_applied, 3,
        "both PAD_SETs and the DEV_EVENT replayed"
    );
    // 3 quanta x (100k/50k grid) = epochs 1..=6.
    assert_eq!(outcome.epoch_hashes_verified, 6);
    assert_eq!(outcome.end_icount, 3 * QUANTUM);
    assert_eq!(outcome.resealed, rec.log, "the reseal hammer");

    // The replayed guest observed the same pads (spot check the table).
    let mut head = [0u8; 8];
    slot.guest_mem
        .read_slice(&mut head, GuestAddress(nanokernel::PAD_ECHO_TABLE_GPA))
        .unwrap();
    assert!(u64::from_le_bytes(head) > 0);
}

#[test]
fn replay_does_not_hash_intermediate_canonical_record_landings() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();

    dh_vmm::run::install_kick_handler().unwrap();
    let sys = KvmSystem::open().unwrap();
    let mut slot = sys.create_slot_vm(MEM).unwrap();
    dh_vmm::boot::load_and_enter(&slot, nanokernel::pad_echo_elf(), b"").unwrap();
    let counter = InstRetired::open_for_current_thread().unwrap();
    counter
        .route_overflow_to_thread(gettid(), dh_vmm::run::kick_signal())
        .unwrap();
    counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
    counter.reset().unwrap();
    counter.enable().unwrap();

    let mut cfg = config();
    cfg.epoch_len = 60_000;
    let cfg_hash = cfg.config_hash().unwrap();
    let mut chain = StateHashChain::new(&cfg_hash, &[0; 32]);
    let bus = record_bus();
    let entropy = DetEntropy::from_seed([0x43; 32]);
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
        &store,
    )
    .expect("base snapshot")
    .snapshot_ref;

    let rail = std::cell::RefCell::new(DeviceRail::new(
        bus,
        entropy,
        LogWriter::new(SegmentHeader {
            base_snapshot_id: snap.to_bytes(),
            entropy_seed: [0; 32],
            machine_config_hash: cfg_hash,
            clock_num: 1,
            clock_den: 1,
            encoder_fingerprint: 0,
        }),
        VmMem(slot.guest_mem.clone()),
    ));
    let pause = AtomicBool::new(false);
    let scheduled_inputs = [40_000, cfg.epoch_len, QUANTUM];
    let out = {
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
        };
        let counter_ref = &counter;
        run_segment_with_scheduled_inputs_frames_and_epochs(
            &mut seg,
            Until::IcountBudget(QUANTUM),
            &scheduled_inputs,
            &[],
            0,
            &mut || false,
            &mut |exit: VcpuExit| {
                let icount = counter_ref
                    .read()
                    .map_err(|e| BoundaryError::Exit(format!("{e:?}")))?;
                rail.borrow_mut().service_exit(icount, exit)
            },
            &mut |idx, boundary| {
                rail.borrow_mut()
                    .apply_pad_set(boundary.icount, boundary.rip, 0, 0xFACE ^ (idx as u32), 0)
                    .map(|vector| vector.into_iter().collect())
                    .map_err(|e| BoundaryError::Exit(format!("input: {e:?}")))
            },
            &mut |idx, icount, value| {
                rail.borrow_mut()
                    .log_epoch_hash(idx, icount, value)
                    .map_err(|e| BoundaryError::Exit(format!("epoch log: {e:?}")))
            },
        )
        .unwrap()
    };
    assert_eq!(out.reason, StopReason::BudgetReached);

    let log = rail.into_inner().seal(&out, [0; 32]).expect("seal");
    let reader = dh_inputlog::reader::LogReader::parse(&log).unwrap();
    assert_eq!(
        reader.canonical().count(),
        scheduled_inputs.len(),
        "the recording has the expected canonical split points"
    );
    assert_eq!(
        reader
            .aux()
            .filter(|rec| matches!(
                rec.body(),
                dh_inputlog::reader::RecordBody::EpochHash { .. }
            ))
            .count(),
        1,
        "only the real epoch boundary should add an epoch/hash record"
    );

    let mut replay_slot = sys.create_slot_vm(MEM).unwrap();
    counter.reset().unwrap();
    let replay_rail = DeviceRail::new(
        record_bus(),
        DetEntropy::from_seed([0; 32]),
        LogWriter::new(SegmentHeader {
            base_snapshot_id: snap.to_bytes(),
            entropy_seed: [0; 32],
            machine_config_hash: cfg_hash,
            clock_num: 1,
            clock_den: 1,
            encoder_fingerprint: 0,
        }),
        VmMem(replay_slot.guest_mem.clone()),
    );
    let outcome = replay_segment(
        &mut replay_slot,
        replay_rail,
        &cfg,
        snap,
        &counter,
        &store,
        &log,
    )
    .expect("replay with non-epoch canonical split");
    assert_eq!(outcome.records_applied, scheduled_inputs.len() as u64);
    assert_eq!(outcome.epoch_hashes_verified, 1);
    assert_eq!(outcome.resealed, log);
}

#[test]
fn replay_refuses_foreign_headers_and_reports_divergence() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();

    // A recording whose guest RAM was mutated AFTER the snapshot: every
    // EPOCH_HASH it carries belongs to a machine the snapshot does not
    // describe. Parse-valid, semantically divergent from the restore.
    let poisoned = record(&store, true);

    let sys = KvmSystem::open().unwrap();
    let counter = InstRetired::open_for_current_thread().unwrap();
    counter
        .route_overflow_to_thread(gettid(), dh_vmm::run::kick_signal())
        .unwrap();
    counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
    counter.reset().unwrap();
    counter.enable().unwrap();

    let mut slot = sys.create_slot_vm(MEM).unwrap();
    let rail = DeviceRail::new(
        record_bus(),
        DetEntropy::from_seed([0; 32]),
        LogWriter::new(SegmentHeader {
            base_snapshot_id: poisoned.snapshot_ref.to_bytes(),
            entropy_seed: [0; 32],
            machine_config_hash: poisoned.cfg.config_hash().unwrap(),
            clock_num: 1,
            clock_den: 1,
            encoder_fingerprint: 0,
        }),
        VmMem(slot.guest_mem.clone()),
    );
    match replay_segment(
        &mut slot,
        rail,
        &poisoned.cfg,
        poisoned.snapshot_ref.clone(),
        &counter,
        &store,
        &poisoned.log,
    ) {
        Err(ReplayError::Divergence { what, .. }) => {
            assert!(what.contains("EPOCH_HASH"), "{what}");
        }
        Err(e) => panic!("wrong error class: {e:?}"),
        Ok(_) => panic!("poisoned recording must diverge"),
    }

    // Wrong machine config → refused at the header, before any restore.
    let mut wrong_cfg = poisoned.cfg.clone();
    wrong_cfg.epoch_len = 60_000;
    let mut slot2 = sys.create_slot_vm(MEM).unwrap();
    let rail2 = DeviceRail::new(
        record_bus(),
        DetEntropy::from_seed([0; 32]),
        LogWriter::new(SegmentHeader {
            base_snapshot_id: poisoned.snapshot_ref.to_bytes(),
            entropy_seed: [0; 32],
            machine_config_hash: [0; 32],
            clock_num: 1,
            clock_den: 1,
            encoder_fingerprint: 0,
        }),
        VmMem(slot2.guest_mem.clone()),
    );
    match replay_segment(
        &mut slot2,
        rail2,
        &wrong_cfg,
        poisoned.snapshot_ref,
        &counter,
        &store,
        &poisoned.log,
    ) {
        Err(ReplayError::HeaderMismatch(what)) => assert_eq!(what, "machine_config_hash"),
        Err(e) => panic!("wrong error class: {e:?}"),
        Ok(_) => panic!("foreign config must be refused"),
    }
}

/// The 1py library harness: a good recording verifies end-to-end with
/// one EpochOk per recorded epoch and a Done carrying the END identity;
/// a machine-mismatched recording yields a Divergence verdict (an Ok
/// report, NOT an infrastructure error) naming the first bad epoch.
#[test]
fn verify_replay_reports_done_and_divergence() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();

    // Good recording → verified.
    let rec = record(&store, false);
    let sys = KvmSystem::open().unwrap();
    let counter = InstRetired::open_for_current_thread().unwrap();
    counter
        .route_overflow_to_thread(gettid(), dh_vmm::run::kick_signal())
        .unwrap();
    counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
    counter.reset().unwrap();
    counter.enable().unwrap();
    let mut slot = sys.create_slot_vm(MEM).unwrap();
    let rail = DeviceRail::new(
        record_bus(),
        DetEntropy::from_seed([0; 32]),
        LogWriter::new(SegmentHeader {
            base_snapshot_id: rec.snapshot_ref.to_bytes(),
            entropy_seed: [0; 32],
            machine_config_hash: rec.cfg.config_hash().unwrap(),
            clock_num: 1,
            clock_den: 1,
            encoder_fingerprint: 0,
        }),
        VmMem(slot.guest_mem.clone()),
    );
    let report = verify_replay(
        &mut slot,
        rail,
        &rec.cfg,
        rec.snapshot_ref.clone(),
        &counter,
        &store,
        &rec.log,
    )
    .expect("verification ran");
    assert!(report.verified());
    assert_eq!(report.epochs_ok(), 6);
    let (total, end_hash) = report.done().unwrap();
    assert_eq!(total, 3 * QUANTUM);
    assert_ne!(end_hash, [0u8; 32]);

    // Poisoned recording → a DIVERGENCE VERDICT, not an error.
    let poisoned = record(&store, true);
    let mut slot2 = sys.create_slot_vm(MEM).unwrap();
    let rail2 = DeviceRail::new(
        record_bus(),
        DetEntropy::from_seed([0; 32]),
        LogWriter::new(SegmentHeader {
            base_snapshot_id: poisoned.snapshot_ref.to_bytes(),
            entropy_seed: [0; 32],
            machine_config_hash: poisoned.cfg.config_hash().unwrap(),
            clock_num: 1,
            clock_den: 1,
            encoder_fingerprint: 0,
        }),
        VmMem(slot2.guest_mem.clone()),
    );
    let report2 = verify_replay(
        &mut slot2,
        rail2,
        &poisoned.cfg,
        poisoned.snapshot_ref.clone(),
        &counter,
        &store,
        &poisoned.log,
    )
    .expect("verification ran");
    assert!(!report2.verified());
    match report2.divergence().unwrap() {
        VerifyProgress::Divergence {
            first_bad_epoch,
            what,
            expected,
            got,
            ..
        } => {
            assert_eq!(*first_bad_epoch, Some(1), "the very first epoch diverges");
            assert!(what.contains("EPOCH_HASH"));
            assert_ne!(expected, got);
        }
        other => panic!("wrong event: {other:?}"),
    }
}
