//! Snapshot-engine joint tests (bead qmp): a live KVM slot, the real
//! device bus, and the REAL snapshot-store (in-process, R12) — the full
//! TakeSnapshot path end to end, both FULL and incremental.
#![cfg(target_arch = "x86_64")]

use dh_devices::clock::PvClock;
use dh_devices::entropy::{DetEntropy, PvEntropy};
use dh_devices::pad::PvPad;
use dh_devices::serial::DebugSerial;
use dh_devices::MmioBus;
use dh_snapshot::dhsnap::{tag, Container, EntrSectionV2, TimeSection};
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::dirty::{enable_dirty_logging, DirtyPageSet, DirtyRing};
use dh_vmm::kvm::{classify_exit, ExitEvent, KvmSystem, SlotVm};
use dh_vmm::{vcpu_state, SlotState};
use dh_worker::snapshot_engine::{
    take_snapshot, BoundaryState, EngineError, PageSource, DEVICE_BLOB_FORMAT_DHSNAP,
};
use snapstore_client::blocking::SnapstoreClient as BlockingClient;
use snapstore_client::Transport;
use snapstore_server::build_server::{serve_for_tests, ServerHandle};
use snapstore_server::config::ServerConfig;
use tempfile::TempDir;

const MEM: u64 = 2 * 1024 * 1024; // 512 pages

fn kvm_available() -> bool {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/kvm")
        .is_ok()
}

/// Real store on a side runtime; the engine itself stays synchronous and
/// reaches it via the blocking facade (the production shape).
fn spawn_store_blocking() -> (
    tokio::runtime::Runtime,
    ServerHandle,
    BlockingClient,
    TempDir,
) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    let dir = TempDir::new().expect("tempdir");
    let data_root = dir.path().to_path_buf();
    let config = ServerConfig {
        data_root: data_root.clone(),
        grpc_tcp_addr: "127.0.0.1:0".parse().expect("addr"),
        grpc_uds_path: Some(data_root.join("snapstore.sock")),
        page_channel_path: None,
        http_addr: "127.0.0.1:0".parse().expect("addr"),
        pagestore: Default::default(),
        meta: Default::default(),
        page_channel: Default::default(),
    };
    let (handle, uds) = rt
        .block_on(serve_for_tests(config))
        .expect("serve_for_tests");
    // Readiness probe (same shape as tests/determinism/store_joint.rs).
    let mut client = None;
    for _ in 0..50 {
        match BlockingClient::connect(Transport::Uds(uds.clone())) {
            Ok(c) => {
                client = Some(c);
                break;
            }
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }
    (rt, handle, client.expect("store ready"), dir)
}

fn test_bus() -> MmioBus {
    let mut bus = MmioBus::new();
    bus.register(0xD000_1000, Box::new(PvPad::new())).unwrap();
    bus.register(0xD000_2000, Box::new(PvClock::new(1, 1)))
        .unwrap();
    bus.register(0xD000_3000, Box::new(PvEntropy::new()))
        .unwrap();
    bus.register(0xD000_6000, Box::new(DebugSerial::new()))
        .unwrap();
    bus
}

fn test_config() -> MachineConfig {
    MachineConfig::new(
        MEM,
        [0x11; 32],
        BootSpec::Elf {
            kernel_hash: [0x22; 32],
            cmdline: b"console=none".to_vec(),
        },
    )
}

fn boundary() -> BoundaryState {
    BoundaryState {
        icount: 1_000_000,
        vns: 1_000_000,
        epoch_index: 2,
        hash_chain: [0xCA; 32],
        agenda_empty: true,
    }
}

fn make_slot(sys: &KvmSystem) -> SlotVm {
    sys.create_slot_vm(MEM).unwrap()
}

#[test]
fn full_snapshot_round_trips_through_the_real_store() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let sys = KvmSystem::open().unwrap();
    let slot = make_slot(&sys);
    let bus = test_bus();
    let entropy = DetEntropy::from_seed([0x42; 32]);
    let config = test_config();

    // Recognizable RAM content.
    use vm_memory::{Bytes, GuestAddress};
    slot.guest_mem
        .write_slice(&[0xAB; 8], GuestAddress(0x4000))
        .unwrap();

    let outcome = take_snapshot(
        &slot,
        SlotState::Paused,
        &bus,
        &entropy,
        &config,
        boundary(),
        PageSource::Full,
        &store,
    )
    .expect("take_snapshot");
    assert_eq!(outcome.pages_shipped, MEM / 4096);
    assert_eq!(outcome.hash_chain, [0xCA; 32]);

    // The ref is a durability receipt: pull the container back and verify
    // the DHSNAP inside it section by section.
    let container = store
        .get_snapshot(outcome.snapshot_ref)
        .expect("get_snapshot");
    let manifest = snapstore_manifest::Manifest::decode(&container).expect("manifest");
    assert_eq!(manifest.device_blob.format, DEVICE_BLOB_FORMAT_DHSNAP);

    let dhsnap = Container::parse(&manifest.device_blob.bytes).expect("DHSNAP parses");
    // Canonical §4 order, the engine's fixed layout for this bus.
    let tags: Vec<[u8; 4]> = dhsnap.sections().map(|s| s.tag).collect();
    assert_eq!(
        tags,
        vec![
            tag::MCFG,
            tag::VCPU,
            tag::LAPC,
            tag::TIME,
            tag::ENTR,
            tag::CLKD,
            tag::PADD,
            tag::SERL
        ]
    );

    // MCFG is the canonical config encoding.
    assert_eq!(
        dhsnap.get(tag::MCFG).unwrap().contents,
        config.canonical_encode().unwrap().as_slice()
    );
    // TIME carries the boundary.
    let t = dhsnap.get(tag::TIME).unwrap();
    let time = TimeSection::decode(t.contents, t.sec_version).unwrap();
    assert_eq!(time.cumulative_icount, 1_000_000);
    assert_eq!(time.hash_chain, [0xCA; 32]);
    // ENTR v2 carries the live PRNG state + the device regs.
    let e = dhsnap.get(tag::ENTR).unwrap();
    let v2 = EntrSectionV2::decode(e.contents, e.sec_version).unwrap();
    assert_eq!(v2.seed, [0x42; 32]);
    // VCPU decodes back to exactly the captured state.
    let v = dhsnap.get(tag::VCPU).unwrap();
    let decoded = vcpu_state::decode_section(v.contents, v.sec_version).unwrap();
    let fresh = vcpu_state::capture(&slot).unwrap();
    assert_eq!(decoded, fresh, "VCPU section is the live capture");
    // LAPC present and empty (v1 placeholder).
    assert_eq!(dhsnap.get(tag::LAPC).unwrap().contents, &[] as &[u8]);
}

#[test]
fn incremental_snapshot_ships_exactly_the_dirty_pages_and_clears() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let sys = KvmSystem::open().unwrap();
    let mut slot = make_slot(&sys);
    let bus = test_bus();
    let entropy = DetEntropy::from_seed([0x43; 32]);
    let config = test_config();

    // Root first (the parent).
    let root = take_snapshot(
        &slot,
        SlotState::Paused,
        &bus,
        &entropy,
        &config,
        boundary(),
        PageSource::Full,
        &store,
    )
    .expect("root snapshot");

    // Now dirty exactly three pages from INSIDE the guest (the ring only
    // sees guest writes).
    let mut ring = DirtyRing::map(&slot.vcpu).expect("ring");
    let mut dirty = DirtyPageSet::new(slot.mem_bytes);
    enable_dirty_logging(&slot).expect("logging on");
    use vm_memory::{Bytes, GuestAddress};
    slot.guest_mem
        .write_slice(
            &[
                0xC6, 0x06, 0x00, 0x20, 0x42, // mov byte [0x2000], 0x42
                0xC6, 0x06, 0x00, 0x50, 0x43, // mov byte [0x5000], 0x43
                0xC6, 0x06, 0x00, 0x90, 0x44, // mov byte [0x9000], 0x44
                0xF4, // hlt
            ],
            GuestAddress(0),
        )
        .unwrap();
    let mut sregs = slot.vcpu.get_sregs().unwrap();
    sregs.cs.base = 0;
    sregs.cs.selector = 0;
    slot.vcpu.set_sregs(&sregs).unwrap();
    let mut regs = slot.vcpu.get_regs().unwrap();
    regs.rip = 0;
    regs.rflags = 2;
    slot.vcpu.set_regs(&regs).unwrap();
    loop {
        match classify_exit(slot.vcpu.run().unwrap()) {
            ExitEvent::Hlt => break,
            ExitEvent::DirtyRingFull => {
                dh_vmm::dirty::harvest_at_boundary(&mut ring, &slot.vm, &mut dirty).unwrap();
            }
            other => panic!("unexpected exit: {other:?}"),
        }
    }

    let outcome = take_snapshot(
        &slot,
        SlotState::Paused,
        &bus,
        &entropy,
        &config,
        boundary(),
        PageSource::Incremental {
            parent: root.snapshot_ref.clone(),
            ring: &mut ring,
            dirty: &mut dirty,
        },
        &store,
    )
    .expect("incremental snapshot");

    // The delta is small (the guest-written pages, possibly plus a page or
    // two of KVM-internal dirtying) and the set was cleared post-ack.
    assert!(outcome.pages_shipped >= 3, "{}", outcome.pages_shipped);
    assert!(
        outcome.pages_shipped < 32,
        "delta should be pages, not the image: {}",
        outcome.pages_shipped
    );
    assert!(
        dirty.is_empty(),
        "dirty set cleared only after the store ack"
    );
    assert_ne!(outcome.snapshot_ref, root.snapshot_ref);

    // The incremental container exists and is a DELTA (carries the parent).
    let container = store.get_snapshot(outcome.snapshot_ref).expect("get");
    let manifest = snapstore_manifest::Manifest::decode(&container).expect("manifest");
    assert_eq!(manifest.parent, Some(root.snapshot_ref));
}

#[test]
fn preconditions_fail_loudly_without_touching_the_store() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let sys = KvmSystem::open().unwrap();
    let slot = make_slot(&sys);
    let bus = test_bus();
    let entropy = DetEntropy::from_seed([0x44; 32]);
    let config = test_config();

    let mut b = boundary();
    b.agenda_empty = false;
    assert!(matches!(
        take_snapshot(
            &slot,
            SlotState::Paused,
            &bus,
            &entropy,
            &config,
            b,
            PageSource::Full,
            &store
        ),
        Err(EngineError::AgendaNotEmpty)
    ));

    for state in [SlotState::Running, SlotState::Frozen, SlotState::Empty] {
        assert!(matches!(
            take_snapshot(
                &slot,
                state,
                &bus,
                &entropy,
                &config,
                boundary(),
                PageSource::Full,
                &store
            ),
            Err(EngineError::NotPaused { .. })
        ));
    }
}

#[test]
fn missing_entropy_device_is_a_loud_codec_error() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let sys = KvmSystem::open().unwrap();
    let slot = make_slot(&sys);
    let mut bus = MmioBus::new();
    bus.register(0xD000_1000, Box::new(PvPad::new())).unwrap(); // no 0x0004
    let entropy = DetEntropy::from_seed([0x45; 32]);
    let config = test_config();

    assert!(matches!(
        take_snapshot(
            &slot,
            SlotState::Paused,
            &bus,
            &entropy,
            &config,
            boundary(),
            PageSource::Full,
            &store
        ),
        Err(EngineError::Codec(_))
    ));
}

/// Byte determinism — the property the fork/dedup foundation rests on:
/// identical state through the WHOLE engine yields the identical ref,
/// even across two independently constructed slots and buses.
#[test]
fn identical_state_yields_identical_refs_across_vms() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let sys = KvmSystem::open().unwrap();
    let config = test_config();

    let mut refs = Vec::new();
    for _ in 0..2 {
        let slot = make_slot(&sys);
        let bus = test_bus();
        let entropy = DetEntropy::from_seed([0x77; 32]);
        let outcome = take_snapshot(
            &slot,
            SlotState::Paused,
            &bus,
            &entropy,
            &config,
            boundary(),
            PageSource::Full,
            &store,
        )
        .expect("take_snapshot");
        refs.push(outcome.snapshot_ref);
    }
    assert_eq!(
        refs[0], refs[1],
        "cross-VM identical state must dedup to one ref"
    );
}

/// The engine's canonical ordering claim, made non-vacuous: a bus whose
/// registration order DISAGREES with §4 table order (blk at a low base)
/// still produces KNOWN_TAGS-ordered sections, and PvBlk's contents ride
/// along intact.
#[test]
fn section_order_is_engine_fixed_not_bus_order() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    use dh_devices::blk::{BaseIoError, BlockBase, PvBlk};
    struct ZeroBase;
    impl BlockBase for ZeroBase {
        fn len_bytes(&self) -> u64 {
            4096
        }
        fn read_at(&self, _offset: u64, buf: &mut [u8]) -> Result<(), BaseIoError> {
            buf.fill(0);
            Ok(())
        }
    }

    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let sys = KvmSystem::open().unwrap();
    let slot = make_slot(&sys);
    // blk FIRST by base address (aligned window below the others) — §4
    // order puts BLKO near the end.
    let mut bus = MmioBus::new();
    bus.register(0xD000_0000, Box::new(PvBlk::new(Box::new(ZeroBase))))
        .unwrap();
    bus.register(0xD000_1000, Box::new(PvPad::new())).unwrap();
    bus.register(0xD000_2000, Box::new(PvClock::new(1, 1)))
        .unwrap();
    bus.register(0xD000_3000, Box::new(PvEntropy::new()))
        .unwrap();
    let entropy = DetEntropy::from_seed([0x78; 32]);
    let config = test_config();

    let outcome = take_snapshot(
        &slot,
        SlotState::Paused,
        &bus,
        &entropy,
        &config,
        boundary(),
        PageSource::Full,
        &store,
    )
    .expect("take_snapshot");
    let container = store.get_snapshot(outcome.snapshot_ref).expect("get");
    let manifest = snapstore_manifest::Manifest::decode(&container).expect("manifest");
    let dhsnap = Container::parse(&manifest.device_blob.bytes).expect("parse");
    let tags: Vec<[u8; 4]> = dhsnap.sections().map(|s| s.tag).collect();
    assert_eq!(
        tags,
        vec![
            tag::MCFG,
            tag::VCPU,
            tag::LAPC,
            tag::TIME,
            tag::ENTR,
            tag::CLKD,
            tag::PADD,
            tag::BLKO
        ]
    );
    assert!(!dhsnap.get(tag::BLKO).unwrap().contents.is_empty());
}
