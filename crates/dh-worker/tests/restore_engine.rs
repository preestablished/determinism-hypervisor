//! Restore-engine joint tests (bead 9wa): a live KVM slot, the real device
//! bus, and the REAL snapshot-store — the full §8.3 tier-B path. The
//! headline acceptance is transparency: take → restore into a FRESH slot →
//! take again yields the IDENTICAL snapshot ref (byte-identical container,
//! same page set), for both FULL roots and DELTA chains.
#![cfg(target_arch = "x86_64")]

mod common;

use common::{CLOCK_BASE, VmMem, kvm_available, spawn_store_blocking, test_bus};
use detguest_host::LogFaultPlan;
use detguest_wire::header::{
    CHANNEL_SIZE, CHANNEL_SIZE_PAGES, ChannelHeader, OFF_MANIFEST, OFF_RESERVED,
};
use detguest_wire::manifest::{MANIFEST_TOTAL_SIZE, init_manifest};
use detguest_wire::ports::{PORT_INIT_GO, PORT_INIT_HI, PORT_INIT_LO};
use dh_devices::clock::{REG_TIMER_DEADLINE, REG_VNS};
use dh_devices::ctx::VecGuestMem;
use dh_devices::detchannel::DetChannelDevice;
use dh_devices::entropy::{DetEntropy, PvEntropy};
use dh_devices::{DevCtx, EntropySource, MmioBus};
use dh_inputlog::dhilog::{LogWriter, SegmentHeader};
use dh_snapshot::dhsnap::{Container, ContainerWriter, tag};
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::dirty::{DirtyPageSet, DirtyRing, PAGE_SIZE, enable_dirty_logging};
use dh_vmm::kvm::{ExitEvent, KvmSystem, classify_exit};
use dh_vmm::{SlotState, vcpu_state};
use dh_worker::restore_engine::{RestoreError, recover_machine_config, restore_snapshot};
use dh_worker::snapshot_engine::{
    BoundaryState, DEVICE_BLOB_FORMAT_DHSNAP, PageSource, take_snapshot,
};
use snapstore_manifest::DeviceBlob;
use snapstore_types::SnapshotRef;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const MEM: u64 = 2 * 1024 * 1024; // 512 pages
const DETCHANNEL_MMIO_BASE: u64 = 0xD000_5000;
const DETCHANNEL_GPA: u64 = 0;

type TestDetChannel = DetChannelDevice<VmMem, LogFaultPlan, fn() -> LogFaultPlan>;

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

/// Drive one MMIO access against a bus outside a live VM (test-only DevCtx
/// plumbing; the bus only cares about a correct icount).
fn with_ctx<R>(icount: u64, f: impl FnOnce(&mut DevCtx) -> R) -> R {
    let mut log = LogWriter::new(SegmentHeader {
        base_snapshot_id: [0; 32],
        entropy_seed: [0; 32],
        machine_config_hash: [0; 32],
        clock_num: 1,
        clock_den: 1,
        encoder_fingerprint: 0,
    });
    let mut mem = VecGuestMem(vec![0u8; 16]);
    let mut entropy = DetEntropy::from_seed([0; 32]);
    let mut irqs = Vec::new();
    let mut ctx = DevCtx::new(icount, 0, &mut log, &mut mem, &mut entropy, &mut irqs);
    f(&mut ctx)
}

/// All device snapshot blobs of a bus, in base order — bus state equality.
fn bus_state(bus: &MmioBus) -> Vec<(u16, Vec<u8>)> {
    bus.devices()
        .map(|(_b, d)| {
            let mut s = Vec::new();
            d.snapshot(&mut s);
            (d.device_id(), s)
        })
        .collect()
}

fn detchannel_for_slot(slot: &dh_vmm::kvm::SlotVm) -> TestDetChannel {
    DetChannelDevice::new(
        VmMem(slot.guest_mem.clone()),
        LogFaultPlan::default(),
        LogFaultPlan::default,
    )
}

fn add_detchannel(bus: &mut MmioBus, slot: &dh_vmm::kvm::SlotVm) {
    bus.register(DETCHANNEL_MMIO_BASE, Box::new(detchannel_for_slot(slot)))
        .unwrap();
}

fn detchannel_mut(bus: &mut MmioBus) -> &mut TestDetChannel {
    bus.devices_mut()
        .find_map(|(_base, dev)| dev.as_any_mut()?.downcast_mut::<TestDetChannel>())
        .expect("DetChannelDevice on bus")
}

fn write_channel_page(mem: &GuestMemoryMmap<()>) {
    assert!(CHANNEL_SIZE as u64 <= MEM);
    let mut header = [0u8; OFF_RESERVED];
    ChannelHeader::canonical().write_to(&mut header).unwrap();
    mem.write_slice(&header, GuestAddress(DETCHANNEL_GPA))
        .unwrap();

    let mut manifest = vec![0u8; MANIFEST_TOTAL_SIZE];
    init_manifest(&mut manifest).unwrap();
    mem.write_slice(
        &manifest,
        GuestAddress(DETCHANNEL_GPA + OFF_MANIFEST as u64),
    )
    .unwrap();
}

#[test]
fn full_restore_is_transparent_and_reseeds_the_segment_clocks() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let sys = KvmSystem::open().unwrap();
    let config = test_config();

    // ── Source slot A: distinctive RAM, vCPU regs, clock and PRNG state ──
    let slot_a = sys.create_slot_vm(MEM).unwrap();
    let mut bus_a = test_bus();
    slot_a
        .guest_mem
        .write_slice(&[0xAB; 64], GuestAddress(0x4000))
        .unwrap();
    let mut regs = slot_a.vcpu.get_regs().unwrap();
    regs.rbx = 0xDEAD_BEEF_0BAD_F00D;
    regs.rip = 0x4321;
    slot_a.vcpu.set_regs(&regs).unwrap();
    with_ctx(0, |ctx| {
        bus_a
            .write(
                CLOCK_BASE + REG_TIMER_DEADLINE,
                &7_777_777u64.to_le_bytes(),
                ctx,
            )
            .unwrap();
    });
    let mut entropy_a = DetEntropy::from_seed([0x42; 32]);
    let mut burn = [0u8; 100];
    entropy_a.fill(&mut burn); // advance word_pos past the seed origin

    let snap = take_snapshot(
        &slot_a,
        SlotState::Paused,
        &bus_a,
        &entropy_a,
        &config,
        boundary(),
        PageSource::Full,
        &store,
    )
    .expect("take_snapshot A");

    // ── Fresh slot B, default-state bus, restore ──────────────────────────
    let slot_b = sys.create_slot_vm(MEM).unwrap();
    let mut bus_b = test_bus();
    let mut dirty_b = DirtyPageSet::new(MEM);
    dirty_b.insert(3).unwrap(); // must come out cleared

    let outcome = restore_snapshot(
        &slot_b,
        SlotState::Paused,
        &mut bus_b,
        &config,
        snap.snapshot_ref.clone(),
        None,
        Some(&mut dirty_b),
        &store,
    )
    .expect("restore_snapshot");

    // Boundary position round-tripped through TIME.
    assert_eq!(outcome.pages_loaded, MEM / PAGE_SIZE);
    assert_eq!(outcome.cumulative_icount, 1_000_000);
    assert_eq!(outcome.vns, 1_000_000);
    assert_eq!(outcome.epoch_index, 2);
    assert_eq!(outcome.chain.value(), [0xCA; 32]);
    assert!(dirty_b.is_empty(), "dirty set cleared on restore");

    // RAM, vCPU, and device state are A's exactly.
    let mut ram = [0u8; 64];
    slot_b
        .guest_mem
        .read_slice(&mut ram, GuestAddress(0x4000))
        .unwrap();
    assert_eq!(ram, [0xAB; 64]);
    assert_eq!(
        vcpu_state::capture(&slot_b).unwrap(),
        vcpu_state::capture(&slot_a).unwrap()
    );
    assert_eq!(bus_state(&bus_b), bus_state(&bus_a));

    // PRNG continuation: B's stream picks up exactly where A's left off.
    assert_eq!(outcome.entropy.state(), entropy_a.state());
    let mut next_a = [0u8; 64];
    let mut next_b = [0u8; 64];
    entropy_a.fill(&mut next_a);
    let mut entropy_b = outcome.entropy;
    entropy_b.fill(&mut next_b);
    assert_eq!(next_a, next_b);

    // vns_base ← TIME.vns: at segment-relative icount 0 the guest reads the
    // boundary's absolute vns — time never jumps back across a restore.
    let vns_read = with_ctx(0, |ctx| {
        let mut buf = [0u8; 8];
        bus_b.read(CLOCK_BASE + REG_VNS, &mut buf, ctx).unwrap();
        u64::from_le_bytes(buf)
    });
    assert_eq!(vns_read, 1_000_000);
}

#[test]
fn restore_device_loop_reattaches_detchannel_evtc_after_ram_load() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let sys = KvmSystem::open().unwrap();
    let config = test_config();

    let slot_a = sys.create_slot_vm(MEM).unwrap();
    write_channel_page(&slot_a.guest_mem);
    let mut bus_a = test_bus();
    add_detchannel(&mut bus_a, &slot_a);
    with_ctx(0, |ctx| {
        let host = detchannel_mut(&mut bus_a).host_mut();
        host.pio_out(PORT_INIT_LO, DETCHANNEL_GPA as u32, ctx);
        host.pio_out(PORT_INIT_HI, (DETCHANNEL_GPA >> 32) as u32, ctx);
        host.pio_out(PORT_INIT_GO, CHANNEL_SIZE_PAGES, ctx);
        assert_eq!(host.channel_gpa(), Some(DETCHANNEL_GPA));
        assert!(host.manifest().is_some());
    });

    let entropy_a = DetEntropy::from_seed([0x42; 32]);
    let snap = take_snapshot(
        &slot_a,
        SlotState::Paused,
        &bus_a,
        &entropy_a,
        &config,
        boundary(),
        PageSource::Full,
        &store,
    )
    .expect("take_snapshot A");

    let slot_b = sys.create_slot_vm(MEM).unwrap();
    let mut bus_b = test_bus();
    add_detchannel(&mut bus_b, &slot_b);
    let outcome = restore_snapshot(
        &slot_b,
        SlotState::Paused,
        &mut bus_b,
        &config,
        snap.snapshot_ref.clone(),
        None,
        None,
        &store,
    )
    .expect("restore_snapshot");

    let restored = detchannel_mut(&mut bus_b).host();
    assert_eq!(restored.channel_gpa(), Some(DETCHANNEL_GPA));
    assert!(restored.manifest().is_some());
    assert_eq!(restored.metrics.manifest_read_failures, 0);
    assert_eq!(outcome.pages_loaded, MEM / PAGE_SIZE);
    assert_eq!(bus_state(&bus_b), bus_state(&bus_a));
}

/// The headline M4 property, isolated: the restored entropy stays at its
/// snapshot position so the re-snapshot is byte-for-byte the original.
#[test]
fn full_restore_resnapshot_yields_the_identical_ref() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let sys = KvmSystem::open().unwrap();
    let config = test_config();

    let slot_a = sys.create_slot_vm(MEM).unwrap();
    let bus_a = test_bus();
    slot_a
        .guest_mem
        .write_slice(&[0x5A; 32], GuestAddress(0x7000))
        .unwrap();
    let mut entropy_a = DetEntropy::from_seed([0x42; 32]);
    let mut burn = [0u8; 100];
    entropy_a.fill(&mut burn);

    let snap = take_snapshot(
        &slot_a,
        SlotState::Paused,
        &bus_a,
        &entropy_a,
        &config,
        boundary(),
        PageSource::Full,
        &store,
    )
    .expect("take_snapshot A");

    let slot_b = sys.create_slot_vm(MEM).unwrap();
    let mut bus_b = test_bus();
    let outcome = restore_snapshot(
        &slot_b,
        SlotState::Paused,
        &mut bus_b,
        &config,
        snap.snapshot_ref.clone(),
        None,
        None,
        &store,
    )
    .expect("restore_snapshot");

    let resnap = take_snapshot(
        &slot_b,
        SlotState::Paused,
        &bus_b,
        &outcome.entropy,
        &config,
        BoundaryState {
            icount: outcome.cumulative_icount,
            vns: outcome.vns,
            epoch_index: outcome.epoch_index,
            hash_chain: outcome.chain.value(),
            agenda_empty: true,
        },
        PageSource::Full,
        &store,
    )
    .expect("re-snapshot B");
    assert_eq!(
        resnap.snapshot_ref, snap.snapshot_ref,
        "take → restore → take must be a fixed point (snapshot transparency)"
    );
}

#[test]
fn delta_chain_restore_materializes_the_full_state() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let sys = KvmSystem::open().unwrap();
    let config = test_config();

    let mut slot_a = sys.create_slot_vm(MEM).unwrap();
    let bus_a = test_bus();
    let entropy_a = DetEntropy::from_seed([0x43; 32]);

    let root = take_snapshot(
        &slot_a,
        SlotState::Paused,
        &bus_a,
        &entropy_a,
        &config,
        boundary(),
        PageSource::Full,
        &store,
    )
    .expect("root");

    // Counter leg (§3.1 icount latch): the counter counts GUEST
    // instructions only (exclude_host), so the guest run below is what
    // inflates it; restore must IOC_RESET it back to zero. Perf may be
    // unavailable in CI sandboxes — skip the leg silently then.
    let counter = dh_detclock::counter::InstRetired::open_for_current_thread().ok();
    if let Some(c) = counter.as_ref() {
        c.enable().unwrap();
    }

    // Dirty three pages from inside the guest, then snapshot the delta.
    let mut ring = DirtyRing::map(&slot_a).expect("ring");
    let mut dirty = DirtyPageSet::new(slot_a.mem_bytes);
    enable_dirty_logging(&slot_a).expect("logging on");
    slot_a
        .guest_mem
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
    let mut sregs = slot_a.vcpu.get_sregs().unwrap();
    sregs.cs.base = 0;
    sregs.cs.selector = 0;
    slot_a.vcpu.set_sregs(&sregs).unwrap();
    let mut regs = slot_a.vcpu.get_regs().unwrap();
    regs.rip = 0;
    regs.rflags = 2;
    slot_a.vcpu.set_regs(&regs).unwrap();
    loop {
        match classify_exit(slot_a.vcpu.run().unwrap()) {
            ExitEvent::Hlt => break,
            ExitEvent::DirtyRingFull => {
                dh_vmm::dirty::harvest_at_boundary(&mut ring, &slot_a.vm, &mut dirty).unwrap();
            }
            other => panic!("unexpected exit: {other:?}"),
        }
    }

    let delta = take_snapshot(
        &slot_a,
        SlotState::Paused,
        &bus_a,
        &entropy_a,
        &config,
        boundary(),
        PageSource::Incremental {
            parent: root.snapshot_ref.clone(),
            ring: &mut ring,
            dirty: &mut dirty,
        },
        &store,
    )
    .expect("delta");

    // A's full image at the same boundary — the reference point.
    let full_a = take_snapshot(
        &slot_a,
        SlotState::Paused,
        &bus_a,
        &entropy_a,
        &config,
        boundary(),
        PageSource::Full,
        &store,
    )
    .expect("full A");

    // Restore the DELTA ref into a fresh slot: the engine gets the
    // server-flattened chain, never walks parents itself.
    let slot_b = sys.create_slot_vm(MEM).unwrap();
    let mut bus_b = test_bus();
    let before_reset = counter.as_ref().map(|c| c.read().unwrap());
    let outcome = restore_snapshot(
        &slot_b,
        SlotState::Paused,
        &mut bus_b,
        &config,
        delta.snapshot_ref.clone(),
        counter.as_ref(),
        None,
        &store,
    )
    .expect("restore delta");

    // The guest executed real instructions above; the new segment counts
    // from zero (no guest entry has happened since the reset).
    if let (Some(c), Some(before)) = (counter.as_ref(), before_reset) {
        assert!(before > 0, "guest run did not move the counter");
        assert_eq!(c.read().unwrap(), 0, "counter not re-zeroed by restore");
    }

    for (gpa, want) in [(0x2000u64, 0x42u8), (0x5000, 0x43), (0x9000, 0x44)] {
        let mut b = [0u8; 1];
        slot_b
            .guest_mem
            .read_slice(&mut b, GuestAddress(gpa))
            .unwrap();
        assert_eq!(b[0], want, "delta page at {gpa:#x}");
    }
    assert_eq!(
        vcpu_state::capture(&slot_b).unwrap(),
        vcpu_state::capture(&slot_a).unwrap()
    );

    // Transparency across the chain: full(B) == full(A).
    let full_b = take_snapshot(
        &slot_b,
        SlotState::Paused,
        &bus_b,
        &outcome.entropy,
        &config,
        BoundaryState {
            icount: outcome.cumulative_icount,
            vns: outcome.vns,
            epoch_index: outcome.epoch_index,
            hash_chain: outcome.chain.value(),
            agenda_empty: true,
        },
        PageSource::Full,
        &store,
    )
    .expect("full B");
    assert_eq!(full_b.snapshot_ref, full_a.snapshot_ref);
}

#[test]
fn restore_preconditions_and_mismatches_fail_loudly() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let sys = KvmSystem::open().unwrap();
    let config = test_config();

    // State gate fires before any store traffic — a fake ref suffices.
    let slot = sys.create_slot_vm(MEM).unwrap();
    for state in [SlotState::Running, SlotState::Frozen, SlotState::Empty] {
        let mut bus = test_bus();
        assert!(matches!(
            restore_snapshot(
                &slot,
                state,
                &mut bus,
                &config,
                SnapshotRef::from_bytes([0xEE; 32]),
                None,
                None,
                &store,
            ),
            Err(RestoreError::NotPaused { .. })
        ));
    }

    // Unknown ref → Store error.
    let mut bus = test_bus();
    assert!(matches!(
        restore_snapshot(
            &slot,
            SlotState::Paused,
            &mut bus,
            &config,
            SnapshotRef::from_bytes([0xEE; 32]),
            None,
            None,
            &store,
        ),
        Err(RestoreError::Store(_))
    ));

    // A real snapshot to mismatch against.
    let bus_a = test_bus();
    let entropy = DetEntropy::from_seed([0x44; 32]);
    let snap = take_snapshot(
        &slot,
        SlotState::Paused,
        &bus_a,
        &entropy,
        &config,
        boundary(),
        PageSource::Full,
        &store,
    )
    .expect("snapshot");

    // Different machine config (kernel hash differs) → ConfigMismatch.
    let other_config = MachineConfig::new(
        MEM,
        [0x11; 32],
        BootSpec::Elf {
            kernel_hash: [0x33; 32],
            cmdline: b"console=none".to_vec(),
        },
    );
    let mut bus = test_bus();
    assert!(matches!(
        restore_snapshot(
            &slot,
            SlotState::Paused,
            &mut bus,
            &other_config,
            snap.snapshot_ref.clone(),
            None,
            None,
            &store,
        ),
        Err(RestoreError::ConfigMismatch(_))
    ));

    // Different guest RAM size → ConfigMismatch before any page lands.
    let big_slot = sys.create_slot_vm(2 * MEM).unwrap();
    let mut bus = test_bus();
    assert!(matches!(
        restore_snapshot(
            &big_slot,
            SlotState::Paused,
            &mut bus,
            &config,
            snap.snapshot_ref.clone(),
            None,
            None,
            &store,
        ),
        Err(RestoreError::ConfigMismatch(_))
    ));

    // Two pv-entropy devices: ENTR v2 holds ONE reg blob — ambiguous, and
    // the section-count arithmetic alone would not catch it.
    let mut two_entropy = test_bus();
    two_entropy
        .register(0xD000_7000, Box::new(PvEntropy::new()))
        .unwrap();
    match restore_snapshot(
        &slot,
        SlotState::Paused,
        &mut two_entropy,
        &config,
        snap.snapshot_ref.clone(),
        None,
        None,
        &store,
    ) {
        Err(RestoreError::Codec(m)) => assert!(m.contains("pv-entropy"), "{m}"),
        Err(e) => panic!("wrong error class: {e:?}"),
        Ok(_) => panic!("two entropy devices must be rejected"),
    }
}

/// Rebuild a parsed DHSNAP with one mutation applied to its section list.
fn rebuild(orig: &[u8], mutate: impl FnOnce(&mut Vec<([u8; 4], u16, Vec<u8>)>)) -> Vec<u8> {
    let c = Container::parse(orig).expect("orig parses");
    let mut sections: Vec<([u8; 4], u16, Vec<u8>)> = c
        .sections()
        .map(|s| (s.tag, s.sec_version, s.contents.to_vec()))
        .collect();
    mutate(&mut sections);
    let mut w = ContainerWriter::new();
    for (t, v, contents) in &sections {
        w.push_section(*t, *v, contents).expect("rebuild section");
    }
    w.finish()
}

#[test]
fn recovers_machine_config_from_snapshot_mcfg() {
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let config = test_config();
    let config_bytes = config.canonical_encode().unwrap();
    let zero_pages: Vec<(u64, Vec<u8>)> = (0..MEM / PAGE_SIZE)
        .map(|i| (i, vec![0u8; PAGE_SIZE as usize]))
        .collect();
    let put_mcfg = |sec_version: u16, contents: &[u8]| {
        let mut w = ContainerWriter::new();
        w.push_section(tag::MCFG, sec_version, contents)
            .expect("MCFG section");
        let bytes = w.finish();
        store
            .put_snapshot_from_parts(
                None,
                MEM,
                zero_pages.clone(),
                DeviceBlob {
                    format: DEVICE_BLOB_FORMAT_DHSNAP,
                    zstd: false,
                    raw_len: bytes.len() as u64,
                    bytes,
                },
            )
            .expect("put MCFG snapshot")
    };

    let recovered = recover_machine_config(put_mcfg(1, &config_bytes), &store).unwrap();
    assert_eq!(recovered.canonical_encode().unwrap(), config_bytes);
    assert_eq!(recovered.boot, config.boot);

    let err = recover_machine_config(put_mcfg(2, &config_bytes), &store).unwrap_err();
    assert!(
        matches!(&err, RestoreError::Codec(m) if m.contains("MCFG")),
        "{err:?}"
    );
}

#[test]
fn mis_shaped_containers_are_rejected_loudly() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let sys = KvmSystem::open().unwrap();
    let config = test_config();

    // A real snapshot supplies the valid DHSNAP to corrupt.
    let slot = sys.create_slot_vm(MEM).unwrap();
    let bus_a = test_bus();
    let entropy = DetEntropy::from_seed([0x45; 32]);
    let snap = take_snapshot(
        &slot,
        SlotState::Paused,
        &bus_a,
        &entropy,
        &config,
        boundary(),
        PageSource::Full,
        &store,
    )
    .expect("snapshot");
    let manifest = snapstore_manifest::Manifest::decode(
        &store.get_snapshot(snap.snapshot_ref.clone()).unwrap(),
    )
    .unwrap();
    let valid_dhsnap = manifest.device_blob.bytes.clone();

    let zero_pages: Vec<(u64, Vec<u8>)> = (0..MEM / PAGE_SIZE)
        .map(|i| (i, vec![0u8; PAGE_SIZE as usize]))
        .collect();
    let put = |bytes: Vec<u8>, format: u32| {
        store
            .put_snapshot_from_parts(
                None,
                MEM,
                zero_pages.clone(),
                DeviceBlob {
                    format,
                    zstd: false,
                    raw_len: bytes.len() as u64,
                    bytes,
                },
            )
            .expect("put crafted snapshot")
    };
    let restore_err = |r: SnapshotRef| {
        let slot_b = sys.create_slot_vm(MEM).unwrap();
        let mut bus = test_bus();
        match restore_snapshot(
            &slot_b,
            SlotState::Paused,
            &mut bus,
            &config,
            r,
            None,
            None,
            &store,
        ) {
            Err(e) => e,
            Ok(_) => panic!("crafted container must be rejected"),
        }
    };

    // Wrong device-blob format tag.
    let r = put(valid_dhsnap.clone(), 0x1111_1111);
    assert!(matches!(restore_err(r), RestoreError::Codec(_)));

    // Unparseable DHSNAP bytes.
    let r = put(b"not a container".to_vec(), DEVICE_BLOB_FORMAT_DHSNAP);
    assert!(matches!(restore_err(r), RestoreError::Codec(_)));

    // Malformed LAPC must be refused, not skipped — silently dropping
    // interrupt state is a determinism bug.
    let r = put(
        rebuild(&valid_dhsnap, |s| {
            s.iter_mut().find(|(t, _, _)| *t == tag::LAPC).unwrap().2 = vec![0x01];
        }),
        DEVICE_BLOB_FORMAT_DHSNAP,
    );
    let err = restore_err(r);
    assert!(
        matches!(&err, RestoreError::Codec(m) if m.contains("LAPC")),
        "{err:?}"
    );

    // Missing ENTR: the engine requires the v2 split section.
    let r = put(
        rebuild(&valid_dhsnap, |s| s.retain(|(t, _, _)| *t != tag::ENTR)),
        DEVICE_BLOB_FORMAT_DHSNAP,
    );
    let err = restore_err(r);
    assert!(
        matches!(&err, RestoreError::Codec(m) if m.contains("ENTR")),
        "{err:?}"
    );

    // An extra device section with no device on this bus → shape mismatch.
    let r = put(
        rebuild(&valid_dhsnap, |s| {
            s.push((tag::NETL, 1, vec![0u8; 4]));
        }),
        DEVICE_BLOB_FORMAT_DHSNAP,
    );
    let err = restore_err(r);
    assert!(
        matches!(&err, RestoreError::Codec(m) if m.contains("shaped")),
        "{err:?}"
    );

    // A bus device whose section is missing → loud, names the device.
    // (Shape arithmetic alone cannot say WHICH device; remove CLKD and add
    // NETL so the count stays right and the per-device lookup must fire.)
    let r = put(
        rebuild(&valid_dhsnap, |s| {
            s.retain(|(t, _, _)| *t != tag::CLKD);
            s.push((tag::NETL, 1, vec![0u8; 4]));
        }),
        DEVICE_BLOB_FORMAT_DHSNAP,
    );
    let err = restore_err(r);
    assert!(
        matches!(&err, RestoreError::Codec(m) if m.contains("no section")),
        "{err:?}"
    );

    // ENTR downgraded to v1 (PRNG-only, 56 bytes): the engine requires the
    // v2 split — without device regs the pv-entropy device cannot be
    // restored, so v1 is refused, not partially accepted.
    let r = put(
        rebuild(&valid_dhsnap, |s| {
            let e = s.iter_mut().find(|(t, _, _)| *t == tag::ENTR).unwrap();
            e.1 = 1;
            e.2.truncate(56);
        }),
        DEVICE_BLOB_FORMAT_DHSNAP,
    );
    let err = restore_err(r);
    assert!(
        matches!(&err, RestoreError::Codec(m) if m.contains("ENTR")),
        "{err:?}"
    );

    // A present-but-malformed device section (wrong length): the DEVICE's
    // own restore rejects it and the engine names the device.
    let r = put(
        rebuild(&valid_dhsnap, |s| {
            s.iter_mut().find(|(t, _, _)| *t == tag::CLKD).unwrap().2 = vec![0u8; 4];
        }),
        DEVICE_BLOB_FORMAT_DHSNAP,
    );
    let err = restore_err(r);
    assert!(
        matches!(&err, RestoreError::Codec(m) if m.contains("rejected its section")),
        "{err:?}"
    );

    // A truncated VCPU section → loud codec error, never a partial vCPU.
    let r = put(
        rebuild(&valid_dhsnap, |s| {
            s.iter_mut()
                .find(|(t, _, _)| *t == tag::VCPU)
                .unwrap()
                .2
                .truncate(16);
        }),
        DEVICE_BLOB_FORMAT_DHSNAP,
    );
    let err = restore_err(r);
    assert!(
        matches!(&err, RestoreError::Codec(m) if m.contains("VCPU")),
        "{err:?}"
    );
}
