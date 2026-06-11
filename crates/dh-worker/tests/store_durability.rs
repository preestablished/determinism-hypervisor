//! M4 ACCEPT: joint integration vs the REAL snapshot-store, risk R12
//! (bead 6hg) — "the ref IS the durability receipt." The round-trip
//! fidelity, parent-relative deltas, and ref-after-ack legs of this
//! acceptance already live in snapshot_engine.rs / restore_engine.rs /
//! m4_transparency.rs (every one against the real in-process server,
//! never a mock) and tests/determinism/store_joint.rs (the R12 wire
//! pins). What this file adds is the half no live-server test had: the
//! DURABILITY of the receipt. A ref handed out by `take_snapshot` must
//! survive the server process dying — a NEW server instance over the
//! same data root must serve the full chain (root + delta) back to a
//! byte-identical restore.
//!
//! If this fails, the store acked before persisting (or persisted
//! somewhere the next instance cannot see) and every ref the engine ever
//! returned was a lie — exactly the R12 failure mode.
//!
//! HARDWARE-GATED: kvm-intel lane + lab box; self-skips elsewhere.

#![cfg(target_arch = "x86_64")]

mod common;

use common::{kvm_available, spawn_store_at, test_bus};
use dh_devices::entropy::DetEntropy;
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::dirty::{enable_dirty_logging, DirtyPageSet, DirtyRing};
use dh_vmm::kvm::{classify_exit, ExitEvent, KvmSystem};
use dh_vmm::{vcpu_state, SlotState};
use dh_worker::restore_engine::restore_snapshot;
use dh_worker::snapshot_engine::{take_snapshot, BoundaryState, PageSource};
use tempfile::TempDir;
use vm_memory::{Bytes, GuestAddress};

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

#[test]
fn refs_survive_a_server_restart_and_restore_byte_identically() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let dir = TempDir::new().expect("tempdir");
    let data_root = dir.path().to_path_buf();
    let sys = KvmSystem::open().unwrap();
    let config = test_config();

    // ── Instance 1: root + guest-dirtied delta, then a reference restore ──
    let (rt1, handle1, store1) = spawn_store_at(data_root.clone(), "first.sock");

    let mut slot_a = sys.create_slot_vm(MEM).unwrap();
    let bus_a = test_bus();
    let entropy = DetEntropy::from_seed([0x51; 32]);
    slot_a
        .guest_mem
        .write_slice(&[0xEE; 32], GuestAddress(0x8000))
        .unwrap();

    let root = take_snapshot(
        &slot_a,
        SlotState::Paused,
        &bus_a,
        &entropy,
        &config,
        boundary(),
        PageSource::Full,
        &store1,
    )
    .expect("root snapshot");

    // Dirty three pages from INSIDE the guest (the ring only sees guest
    // writes), then ship the delta — the chain the restart must preserve.
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
        &entropy,
        &config,
        boundary(),
        PageSource::Incremental {
            parent: root.snapshot_ref.clone(),
            ring: &mut ring,
            dirty: &mut dirty,
        },
        &store1,
    )
    .expect("delta snapshot");

    // Reference point: restore + re-snapshot against the SAME instance.
    let slot_b1 = sys.create_slot_vm(MEM).unwrap();
    let mut bus_b1 = test_bus();
    let outcome1 = restore_snapshot(
        &slot_b1,
        SlotState::Paused,
        &mut bus_b1,
        &config,
        delta.snapshot_ref.clone(),
        None,
        None,
        &store1,
    )
    .expect("restore against instance 1");
    let ref_b1 = take_snapshot(
        &slot_b1,
        SlotState::Paused,
        &bus_b1,
        &outcome1.entropy,
        &config,
        BoundaryState {
            icount: outcome1.cumulative_icount,
            vns: outcome1.vns,
            epoch_index: outcome1.epoch_index,
            hash_chain: outcome1.chain.value(),
            agenda_empty: true,
        },
        PageSource::Full,
        &store1,
    )
    .expect("re-snapshot against instance 1")
    .snapshot_ref;

    // ── KILL the server. Drop the client and its runtime too: nothing of
    //    instance 1 survives but the bytes under data_root. ───────────────
    drop(store1);
    handle1.shutdown();
    drop(rt1);

    // ── Instance 2: same data root, fresh process state ──────────────────
    let (_rt2, _handle2, store2) = spawn_store_at(data_root, "second.sock");

    // The receipt must still resolve: manifest, flattened chain, restore.
    let slot_b2 = sys.create_slot_vm(MEM).unwrap();
    let mut bus_b2 = test_bus();
    let outcome2 = restore_snapshot(
        &slot_b2,
        SlotState::Paused,
        &mut bus_b2,
        &config,
        delta.snapshot_ref.clone(),
        None,
        None,
        &store2,
    )
    .expect("restore against the RESTARTED store — the ref must be durable");

    // Byte-identical machine: guest-dirtied delta pages, pre-delta RAM,
    // and the full vCPU state all match the live source slot.
    for (gpa, want) in [(0x2000u64, 0x42u8), (0x5000, 0x43), (0x9000, 0x44)] {
        let mut b = [0u8; 1];
        slot_b2
            .guest_mem
            .read_slice(&mut b, GuestAddress(gpa))
            .unwrap();
        assert_eq!(b[0], want, "delta page at {gpa:#x} after restart");
    }
    let mut ram = [0u8; 32];
    slot_b2
        .guest_mem
        .read_slice(&mut ram, GuestAddress(0x8000))
        .unwrap();
    assert_eq!(ram, [0xEE; 32], "root-era page after restart");
    assert_eq!(
        vcpu_state::capture(&slot_b2).unwrap(),
        vcpu_state::capture(&slot_a).unwrap(),
        "vCPU state after restart"
    );
    assert_eq!(outcome2.chain.value(), outcome1.chain.value());

    // The strongest form: re-snapshot through the restarted instance and
    // the ref is the SAME 32 bytes instance 1 issued — content-addressed
    // identity carried entirely by the persisted bytes.
    let ref_b2 = take_snapshot(
        &slot_b2,
        SlotState::Paused,
        &bus_b2,
        &outcome2.entropy,
        &config,
        BoundaryState {
            icount: outcome2.cumulative_icount,
            vns: outcome2.vns,
            epoch_index: outcome2.epoch_index,
            hash_chain: outcome2.chain.value(),
            agenda_empty: true,
        },
        PageSource::Full,
        &store2,
    )
    .expect("re-snapshot against instance 2")
    .snapshot_ref;
    assert_eq!(
        ref_b2, ref_b1,
        "restart changed the bytes a restore reproduces"
    );
}
