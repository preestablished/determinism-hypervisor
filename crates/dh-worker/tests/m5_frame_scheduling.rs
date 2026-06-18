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

use std::sync::atomic::AtomicBool;

use common::{VmMem, gettid, kvm_available, spawn_store_blocking};
use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};
use dh_devices::ctx::VecGuestMem;
use dh_devices::entropy::{DetEntropy, PvEntropy};
use dh_devices::pad::{PV_PAD_BASE, PvPad, REG_FRAME_COUNTER};
use dh_devices::{DevCtx, MmioBus};
use dh_inputlog::dhilog::{LogWriter, SegmentHeader};
use dh_inputlog::reader::{LogReader, RecordBody};
use dh_vmm::SlotState;
use dh_vmm::boundary::BoundaryError;
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::hash::StateHashChain;
use dh_vmm::kvm::{KvmSystem, SlotVm};
use dh_vmm::recording::DeviceRail;
use dh_vmm::runctl::{Segment, SegmentOutcome, StopReason, Until, run_segment};
use dh_worker::restore_engine::restore_snapshot;
use dh_worker::snapshot_engine::{BoundaryState, PageSource, take_snapshot};

const MEM: u64 = 16 << 20;
const FIRST_FRAMES: u64 = 3;
const AFTER_RESTORE_FRAMES: u64 = 2;
const FRAME_HARD_CAP: u64 = 50_000_000;
const AT_FRAME_TARGET: u32 = 2;

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
