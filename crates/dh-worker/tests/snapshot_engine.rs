//! Snapshot-engine joint tests (bead qmp): a live KVM slot, the real
//! device bus, and the REAL snapshot-store (in-process, R12) — the full
//! TakeSnapshot path end to end, both FULL and incremental.
#![cfg(target_arch = "x86_64")]

mod common;

use common::{kvm_available, spawn_store_blocking, test_bus};
use dh_devices::clock::PvClock;
use dh_devices::entropy::{DetEntropy, PvEntropy};
use dh_devices::pad::PvPad;
use dh_devices::MmioBus;
use dh_snapshot::dhsnap::{tag, Container, EntrSectionV2, LapcSection, TimeSection};
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::dirty::{
    enable_dirty_logging, harvest_at_boundary, DirtyPageSet, DirtyRing, PAGE_SIZE,
};
use dh_vmm::kvm::{classify_exit, ExitEvent, KvmSystem, SlotVm};
use dh_vmm::{vcpu_state, SlotState};
use dh_worker::snapshot_engine::{
    capture_bisection_checkpoint_snapshot, take_snapshot, BoundaryState, EngineError, PageSource,
    DEVICE_BLOB_FORMAT_DHSNAP,
};

const MEM: u64 = 2 * 1024 * 1024; // 512 pages

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

fn run_guest_byte_writes(slot: &mut SlotVm, writes: &[(u16, u8)]) {
    let mut code = Vec::with_capacity(writes.len() * 5 + 1);
    for (addr, value) in writes {
        let [lo, hi] = addr.to_le_bytes();
        code.extend_from_slice(&[0xC6, 0x06, lo, hi, *value]);
    }
    code.push(0xF4); // hlt

    use vm_memory::{Bytes, GuestAddress};
    slot.guest_mem.write_slice(&code, GuestAddress(0)).unwrap();
    let mut sregs = slot.vcpu.get_sregs().unwrap();
    sregs.cs.base = 0;
    sregs.cs.selector = 0;
    slot.vcpu.set_sregs(&sregs).unwrap();
    let mut regs = slot.vcpu.get_regs().unwrap();
    regs.rip = 0;
    regs.rflags = 2;
    slot.vcpu.set_regs(&regs).unwrap();
    match classify_exit(slot.vcpu.run().unwrap()) {
        ExitEvent::Hlt => {}
        other => panic!("unexpected exit: {other:?}"),
    }
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
    // LAPC carries the reset deterministic userspace lAPIC state by default.
    let l = dhsnap.get(tag::LAPC).unwrap();
    assert_eq!(l.sec_version, LapcSection::VERSION);
    assert_eq!(
        LapcSection::decode(l.contents, l.sec_version).unwrap(),
        LapcSection::default()
    );
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
    let mut ring = DirtyRing::map(&slot).expect("ring");
    let mut dirty = DirtyPageSet::new(slot.mem_bytes);
    enable_dirty_logging(&slot).expect("logging on");
    run_guest_byte_writes(&mut slot, &[(0x2000, 0x42), (0x5000, 0x43), (0x9000, 0x44)]);

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
fn bisection_checkpoint_capture_is_full_and_preserves_dirty_tracking() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let sys = KvmSystem::open().unwrap();
    let mut slot = make_slot(&sys);
    let bus = test_bus();
    let entropy = DetEntropy::from_seed([0x47; 32]);
    let config = test_config();

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

    let mut ring = DirtyRing::map(&slot).expect("ring");
    let mut dirty = DirtyPageSet::new(slot.mem_bytes);
    enable_dirty_logging(&slot).expect("logging on");
    run_guest_byte_writes(&mut slot, &[(0x2000, 0x42), (0x5000, 0x43), (0x9000, 0x44)]);

    let entropy_before = entropy.state();
    let mut checkpoint_boundary = boundary();
    checkpoint_boundary.agenda_empty = false;
    let checkpoint = capture_bisection_checkpoint_snapshot(
        &slot,
        SlotState::Paused,
        &bus,
        &entropy,
        &config,
        checkpoint_boundary,
        &store,
    )
    .expect("bisection checkpoint capture");
    assert_eq!(checkpoint.pages_shipped, MEM / PAGE_SIZE);
    assert_eq!(checkpoint.hash_chain, [0xCA; 32]);
    assert_eq!(
        entropy.state(),
        entropy_before,
        "checkpoint capture must not reseed or advance entropy"
    );

    let container = store
        .get_snapshot(checkpoint.snapshot_ref.clone())
        .expect("get checkpoint");
    let manifest = snapstore_manifest::Manifest::decode(&container).expect("manifest");
    assert_eq!(manifest.parent, None);
    assert_eq!(manifest.device_blob.format, DEVICE_BLOB_FORMAT_DHSNAP);
    let resolved = store
        .resolve_pages(checkpoint.snapshot_ref.clone(), None, false)
        .expect("resolve checkpoint pages");
    assert_eq!(resolved.len(), (MEM / PAGE_SIZE) as usize);
    for (page, expected) in [(0x2, 0x42), (0x5, 0x43), (0x9, 0x44)] {
        let payload = resolved
            .iter()
            .find(|(idx, _, _)| *idx == page)
            .and_then(|(_, _, payload)| payload.as_ref())
            .unwrap_or_else(|| panic!("missing resolved page {page:#x}"));
        assert_eq!(payload[0], expected, "page {page:#x}");
    }

    assert!(
        dirty.is_empty(),
        "checkpoint capture must not harvest into the dirty page set"
    );
    let stats = harvest_at_boundary(&mut ring, &slot.vm, &mut dirty).expect("post-capture harvest");
    assert!(stats.harvested >= 3, "stats: {stats:?}");
    for page in [0x2u64, 0x5, 0x9] {
        assert!(dirty.contains(page), "page {page:#x} missing: {dirty:?}");
    }

    let incremental = take_snapshot(
        &slot,
        SlotState::Paused,
        &bus,
        &entropy,
        &config,
        boundary(),
        PageSource::Incremental {
            parent: root.snapshot_ref,
            ring: &mut ring,
            dirty: &mut dirty,
        },
        &store,
    )
    .expect("post-checkpoint incremental snapshot");
    assert!(
        incremental.pages_shipped >= 3,
        "{}",
        incremental.pages_shipped
    );
    assert!(
        incremental.pages_shipped < MEM / PAGE_SIZE,
        "incremental lineage snapshot should remain a delta"
    );
    assert!(
        dirty.is_empty(),
        "ordinary TakeSnapshot still clears dirty tracking after store ack"
    );
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
        assert!(matches!(
            capture_bisection_checkpoint_snapshot(
                &slot,
                state,
                &bus,
                &entropy,
                &config,
                boundary(),
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

/// Mirror of the restore engine's exactly-one guard: ENTR v2 holds ONE
/// device reg blob, so a second pv-entropy device is ambiguous state the
/// capture engine must refuse to launder into a snapshot.
#[test]
fn two_entropy_devices_is_a_loud_codec_error() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let sys = KvmSystem::open().unwrap();
    let slot = make_slot(&sys);
    let mut bus = test_bus();
    bus.register(0xD000_7000, Box::new(PvEntropy::new()))
        .unwrap(); // second 0x0004
    let entropy = DetEntropy::from_seed([0x46; 32]);
    let config = test_config();

    match take_snapshot(
        &slot,
        SlotState::Paused,
        &bus,
        &entropy,
        &config,
        boundary(),
        PageSource::Full,
        &store,
    ) {
        Err(EngineError::Codec(m)) => assert!(m.contains("pv-entropy"), "{m}"),
        Err(e) => panic!("wrong error class: {e:?}"),
        Ok(_) => panic!("two entropy devices must be rejected"),
    }
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
