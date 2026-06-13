//! M5 ACCEPT (bead a5e): a seeded, scripted pad sequence spanning 60
//! guest-seconds of vns, recorded on the pad-echo guest from a real
//! snapshot, then replayed from `(snapshot, DHILOG)` — every replay must
//! reproduce `end_state_hash` AND every EPOCH_HASH, 100 times, with zero
//! divergence. The replay engine verifies each epoch link at the link
//! point, checks `end_vns` and `end_state_hash` at END, and reseals the
//! log byte-identically (the strongest equality the layer states).
//!
//! NON-1:1 CLOCK (load-bearing): the segment runs under a 10_000:1
//! vns/icount ratio, so 100k icount = one guest-second and the 6M-icount
//! run ends at exactly 60e9 vns. The RECORD-side `vns == seconds * 1e9`
//! assert is the independent oracle for the clock arithmetic; on the
//! replay side, end_vns finally flows through replay_engine.rs's check
//! with a value distinct from the icount (its note: "masked by 1:1
//! clocks until a5e") — though once the header pins the clock ratio,
//! that check cannot fail independently of end_icount.
//!
//! GRID: quantum == epoch_len == 100k, so every recorded stop boundary
//! IS an epoch point — the hash chain is a pure function of the absolute
//! epoch grid and the recorded inputs, and replay's by-record
//! quantization links the identical chain. One EPOCH_HASH per
//! guest-second: 60 over the acceptance run.
//!
//! PLACEMENT: the a5e bead said tests/determinism, but the ARCH §1
//! normative rule "nothing depends on dh-worker" (pinned by
//! arch_dependency_rule.rs, dev edges included) forbids that package
//! from importing the replay/snapshot engines — so this acceptance
//! lives beside the bead-39w replay tests, like every M4 engine
//! acceptance before it.
//!
//! SHAPE: the x100 acceptance is `#[ignore]`d like the M4 perf gates —
//! minutes of deliberate, quiesced-box work, not ordinary-sweep load
//! (measured: 100 replays ≈ 10 min release on the lab box):
//!
//!   cargo test -p dh-worker --test m5_record_replay --release \
//!     -- --ignored --nocapture
//!
//! The unignored smoke keeps the same rig (same script source, same
//! non-1:1 clock) in every sweep at 6 guest-seconds x1 replay.
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
use dh_inputlog::dhilog::{LogWriter, SegmentHeader};
use dh_inputlog::reader::{LogReader, RecordBody};
use dh_vmm::boundary::BoundaryError;
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::hash::StateHashChain;
use dh_vmm::kvm::{KvmSystem, SlotVm};
use dh_vmm::recording::DeviceRail;
use dh_vmm::runctl::{run_segment_with_epochs, Segment, SegmentOutcome, StopReason, Until};
use dh_vmm::vt::ClockRatio;
use dh_vmm::SlotState;
use dh_worker::replay_engine::replay_segment;
use dh_worker::snapshot_engine::{take_snapshot, BoundaryState, PageSource};
use kvm_ioctls::VcpuExit;
use vm_memory::{Bytes, GuestAddress};

const MEM: u64 = 16 << 20;

/// One run quantum AND the epoch grid (see module docs: keeping them
/// equal makes every stop boundary an epoch point).
const QUANTUM: u64 = 100_000;

/// vns per icount: 100k icount = 1e9 vns = one guest-second.
const CLOCK_NUM: u32 = 10_000;
const VNS_PER_SECOND: u64 = 1_000_000_000;

const ACCEPT_SECONDS: u64 = 60;
const ACCEPT_REPLAYS: usize = 100;
const SMOKE_SECONDS: u64 = 6;

/// The script seed — any fixed value; the sequence is a pure function
/// of it (that is the "seeded, scripted" in the acceptance wording).
const SCRIPT_SEED: u64 = 0x00A5_E060_5EED;

fn hex(b: &[u8; 32]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// SplitMix64 — tiny, dependency-free, and fully determined by the seed.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The scripted pad sequence: one PAD_SET per inter-quantum boundary
/// (`seconds - 1` of them — none after the final quantum; a sealed
/// segment ends on a clean boundary with nothing pending).
///
/// INVARIANT (checked): no value is 0 (the boot latch) and no two
/// adjacent values are equal — `assert_table_eras` compares the
/// guest-observed era sequence EXACTLY, so a seed change that
/// introduced a collapsed era must fail here, loudly, not weaken the
/// era assertion silently.
fn pad_script(seconds: u64) -> Vec<u32> {
    let mut s = SCRIPT_SEED;
    let script: Vec<u32> = (1..seconds).map(|_| splitmix64(&mut s) as u32).collect();
    assert!(
        !script.contains(&0) && script.windows(2).all(|w| w[0] != w[1]),
        "script invariant violated for seed {SCRIPT_SEED:#x}: pick a seed with \
         no zeros and no adjacent-equal values (eras would collapse)"
    );
    script
}

fn config() -> MachineConfig {
    let mut c = MachineConfig::new(
        MEM,
        [0x5A; 32],
        BootSpec::Elf {
            kernel_hash: [0x5A; 32],
            cmdline: Vec::new(),
        },
    );
    c.epoch_len = QUANTUM;
    c.clock = ClockRatio::new(CLOCK_NUM, 1).expect("nonzero ratio");
    c
}

/// The pad-echo bus: the pad the guest polls plus the entropy device the
/// snapshot engines require on every bus. 0xD000_3000 is the joint-test
/// entropy base (replay_engine.rs, common::test_bus) — deliberately NOT
/// `entropy::PV_ENTROPY_BASE` (0xD000_2000), which collides with the
/// canonical clock base in that layout.
fn record_bus() -> MmioBus {
    let mut bus = MmioBus::new();
    bus.register(dh_devices::pad::PV_PAD_BASE, Box::new(PvPad::new()))
        .unwrap();
    bus.register(0xD000_3000, Box::new(PvEntropy::new()))
        .unwrap();
    bus
}

fn header_for(snap: &snapstore_types::SnapshotRef, cfg_hash: [u8; 32]) -> SegmentHeader {
    SegmentHeader {
        base_snapshot_id: snap.to_bytes(),
        entropy_seed: [0; 32], // zero ⇒ replay continues the snapshot PRNG
        machine_config_hash: cfg_hash,
        clock_num: CLOCK_NUM,
        clock_den: 1,
        encoder_fingerprint: 0,
    }
}

struct Recording {
    snapshot_ref: snapstore_types::SnapshotRef,
    log: Vec<u8>,
    cfg: MachineConfig,
    end_state_hash: [u8; 32],
}

/// Boot pad_echo, snapshot at the boot boundary, then record `seconds`
/// one-guest-second quanta with the scripted PAD_SET at every
/// inter-quantum boundary; seal from the final outcome.
fn record(store: &snapstore_client::blocking::SnapstoreClient, seconds: u64) -> Recording {
    let script = pad_script(seconds);

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

    let rail = std::cell::RefCell::new(DeviceRail::new(
        bus,
        entropy,
        LogWriter::new(header_for(&snap, cfg_hash)),
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

    let mut last = None;
    for i in 1..=seconds {
        let out = run_one(&mut slot, &mut chain);
        assert_eq!(
            out.boundary.icount,
            i * QUANTUM,
            "quantum {i} must land exactly on the grid"
        );
        if i < seconds {
            // The script: pad i lands at the end-of-second-i boundary.
            // frame_hint = i is not independently asserted here — it
            // matters because the reseal hammer round-trips it
            // byte-identically through record and replay.
            rail.borrow_mut()
                .apply_pad_set(
                    out.boundary.icount,
                    out.boundary.rip,
                    0,
                    script[(i - 1) as usize],
                    i as u32,
                )
                .unwrap();
        }
        last = Some(out);
    }
    let last = last.expect("at least one quantum");

    // The 60-guest-second identity: vns is exact, not approximate.
    assert_eq!(
        last.vns,
        seconds * VNS_PER_SECOND,
        "the run must end at exactly {seconds} guest-seconds of vns"
    );
    assert!(rail.borrow().irqs.is_empty(), "pad vector disabled");
    let end_state_hash = last.state_hash;
    let log = rail.into_inner().seal(&last, [0; 32]).expect("seal");

    // Sanity-parse the sealed log: the full epoch grid and the full
    // script landed.
    let r = LogReader::parse(&log).expect("parse own log");
    assert!(r.header().has_epoch_hashes());
    let epochs = r
        .aux()
        .filter(|rec| matches!(rec.body(), RecordBody::EpochHash { .. }))
        .count() as u64;
    assert_eq!(epochs, seconds, "one EPOCH_HASH per guest-second");
    let pads: Vec<u32> = r
        .canonical()
        .filter_map(|rec| match rec.body() {
            RecordBody::PadSet { buttons, .. } => Some(buttons),
            _ => None,
        })
        .collect();
    assert_eq!(pads, script, "the recorded sequence IS the script");
    let (_, log_end_hash) = r.end();
    assert_eq!(log_end_hash, end_state_hash);

    Recording {
        snapshot_ref: snap,
        log,
        cfg,
        end_state_hash,
    }
}

/// One replay leg on a fresh slot: full verification happens inside
/// `replay_segment` (epoch links at the link point, end_vns,
/// end_state_hash, reseal); the assertions here pin the counts and the
/// byte-identity to THIS recording. Returns the slot SO THE CALLER CAN
/// READ THE GUEST OBSERVATION TABLE (`assert_table_eras`) — do not
/// simplify the return to `()`.
fn replay_once(
    sys: &KvmSystem,
    counter: &InstRetired,
    store: &snapstore_client::blocking::SnapstoreClient,
    rec: &Recording,
    seconds: u64,
) -> SlotVm {
    let mut slot = sys.create_slot_vm(MEM).unwrap();
    let rail = DeviceRail::new(
        record_bus(),
        DetEntropy::from_seed([0; 32]), // replaced by the restored PRNG
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
        counter,
        store,
        &rec.log,
    )
    .expect("replay must not diverge");

    assert_eq!(outcome.records_applied, seconds - 1, "every scripted pad");
    assert_eq!(
        outcome.epoch_hashes_verified, seconds,
        "every EPOCH_HASH verified"
    );
    assert_eq!(outcome.end_icount, seconds * QUANTUM);
    assert_eq!(outcome.end_state_hash, rec.end_state_hash);
    // DELIBERATE wire-format lock: replay_segment already enforces the
    // reseal internally, so this adds no divergence-detection power —
    // but the M5 gate also pins DHILOG v1.0 bytes (the golden-fixtures
    // milestone sibling): a log-format change SHOULD redden this
    // acceptance and force a re-bless.
    assert_eq!(outcome.resealed, rec.log, "the reseal hammer");
    slot
}

/// The guest-side proof: pad-echo's RAM table observed the latch eras in
/// script order — consecutive-dedup of the per-frame pad0 column must be
/// exactly `[0 (boot latch), script...]` (exact because of pad_script's
/// checked invariant). The strong "every PAD_SET landed with the right
/// bytes" proof is the host-side `pads == script` compare in `record`;
/// this one proves the values reached the GUEST.
fn assert_table_eras(slot: &SlotVm, seconds: u64) {
    let mut head = [0u8; 8];
    slot.guest_mem
        .read_slice(&mut head, GuestAddress(nanokernel::PAD_ECHO_TABLE_GPA))
        .unwrap();
    let count = u64::from_le_bytes(head);
    assert!(count > 0, "guest recorded no frames");
    assert!(
        count < nanokernel::PAD_ECHO_TABLE_CAPACITY,
        "ring wrapped ({count} frames) — era reconstruction would be lossy"
    );
    let mut eras: Vec<u32> = Vec::new();
    for i in 0..count {
        let mut e = [0u8; 8];
        slot.guest_mem
            .read_slice(
                &mut e,
                GuestAddress(
                    nanokernel::PAD_ECHO_TABLE_GPA
                        + 8
                        + (i & (nanokernel::PAD_ECHO_TABLE_CAPACITY - 1))
                            * nanokernel::PAD_ECHO_ENTRY_BYTES,
                ),
            )
            .unwrap();
        let pad0 = u32::from_le_bytes(e[4..8].try_into().unwrap());
        if eras.last() != Some(&pad0) {
            eras.push(pad0);
        }
    }
    // Exact compare, no dedup on the expected side: pad_script's checked
    // invariant (no zeros, no adjacent-equal values) guarantees every
    // scripted value opens a distinct era.
    let mut expected = vec![0u32];
    expected.extend(pad_script(seconds));
    assert_eq!(eras, expected, "guest-observed latch eras == the script");
}

/// The every-sweep smoke: the identical rig at 6 guest-seconds x1 replay
/// keeps the non-1:1 clock path (end_vns) continuously covered.
#[test]
fn m5_smoke_record_replay_6s_vns_nonunit_clock() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let rec = record(&store, SMOKE_SECONDS);

    let sys = KvmSystem::open().unwrap();
    let counter = InstRetired::open_for_current_thread().unwrap();
    counter
        .route_overflow_to_thread(gettid(), dh_vmm::run::kick_signal())
        .unwrap();
    counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
    counter.reset().unwrap();
    counter.enable().unwrap();

    let slot = replay_once(&sys, &counter, &store, &rec, SMOKE_SECONDS);
    assert_table_eras(&slot, SMOKE_SECONDS);
}

/// THE M5 ACCEPTANCE (bead a5e): 60 guest-seconds of scripted pads,
/// replayed 100x from `(snapshot, DHILOG)` with zero divergence — every
/// replay reproduces end_state_hash, every EPOCH_HASH, end_vns, and the
/// exact log bytes. See the module docs for the invocation.
#[test]
#[ignore = "M5 acceptance gate: minutes of deliberate work — \
            cargo test -p dh-worker --test m5_record_replay --release -- --ignored --nocapture"]
fn m5_accept_record_replay_60s_vns_pad_sequence_x100() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let rec = record(&store, ACCEPT_SECONDS);
    eprintln!(
        "recorded: {} bytes of DHILOG, end_state_hash {}",
        rec.log.len(),
        hex(&rec.end_state_hash)
    );

    let sys = KvmSystem::open().unwrap();
    let counter = InstRetired::open_for_current_thread().unwrap();
    counter
        .route_overflow_to_thread(gettid(), dh_vmm::run::kick_signal())
        .unwrap();
    counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
    counter.reset().unwrap();
    counter.enable().unwrap();

    for i in 1..=ACCEPT_REPLAYS {
        // A fresh slot per replay (the restore engine's precondition);
        // the counter is reset by each restore. The slot drops at scope
        // end — 100 concurrent 16 MiB guests would be pointless load.
        let slot = replay_once(&sys, &counter, &store, &rec, ACCEPT_SECONDS);
        if i == 1 || i == ACCEPT_REPLAYS {
            // The guest-visible script check: dispositive on the first
            // and last legs; the replay engine's reseal byte-identity
            // covers every leg in between.
            assert_table_eras(&slot, ACCEPT_SECONDS);
        }
        if i % 10 == 0 {
            eprintln!("replay {i}/{ACCEPT_REPLAYS}: zero divergence");
        }
    }
}
