//! Restore-engine joint tests (bead 9wa): a live KVM slot, the real device
//! bus, and the REAL snapshot-store — the full §8.3 tier-B path. The
//! headline acceptance is transparency: take → restore into a FRESH slot →
//! take again yields the IDENTICAL snapshot ref (byte-identical container,
//! same page set), for both FULL roots and DELTA chains.
#![cfg(target_arch = "x86_64")]

use dh_devices::clock::{PvClock, REG_TIMER_DEADLINE, REG_VNS};
use dh_devices::ctx::VecGuestMem;
use dh_devices::entropy::{DetEntropy, PvEntropy};
use dh_devices::pad::PvPad;
use dh_devices::serial::DebugSerial;
use dh_devices::{DevCtx, EntropySource, MmioBus};
use dh_inputlog::dhilog::{LogWriter, SegmentHeader};
use dh_snapshot::dhsnap::{tag, Container, ContainerWriter};
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::dirty::{enable_dirty_logging, DirtyPageSet, DirtyRing, PAGE_SIZE};
use dh_vmm::kvm::{classify_exit, ExitEvent, KvmSystem};
use dh_vmm::{vcpu_state, SlotState};
use dh_worker::restore_engine::{restore_snapshot, RestoreError};
use dh_worker::snapshot_engine::{
    take_snapshot, BoundaryState, PageSource, DEVICE_BLOB_FORMAT_DHSNAP,
};
use snapstore_client::blocking::SnapstoreClient as BlockingClient;
use snapstore_client::Transport;
use snapstore_manifest::DeviceBlob;
use snapstore_server::build_server::{serve_for_tests, ServerHandle};
use snapstore_server::config::ServerConfig;
use snapstore_types::SnapshotRef;
use tempfile::TempDir;
use vm_memory::{Bytes, GuestAddress};

const MEM: u64 = 2 * 1024 * 1024; // 512 pages
const CLOCK_BASE: u64 = 0xD000_2000;

fn kvm_available() -> bool {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/kvm")
        .is_ok()
}

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
    bus.register(CLOCK_BASE, Box::new(PvClock::new(1, 1)))
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
    let mut ring = DirtyRing::map(&slot_a.vcpu).expect("ring");
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

    // Non-empty LAPC (a newer writer's lapic stub) must be refused, not
    // skipped — silently dropping interrupt state is a determinism bug.
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
