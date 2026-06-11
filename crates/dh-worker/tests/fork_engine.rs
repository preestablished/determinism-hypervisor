//! Tier-A CoW fork joint tests (bead 9e4): a live frozen parent, CoW
//! children with fresh KVM fds, state stuffed through the one §8.3 codec.
//! The kernel CoW semantics (private mapping of a sealed memfd) were
//! proven by the iteration-66 probe; these tests prove the ENGINE — the
//! child is the parent's machine byte-for-byte, writes never travel in
//! either direction, and a forked child snapshots to the parent's exact
//! ref (the a6s fork-transparency ACCEPT builds on that identity).
#![cfg(target_arch = "x86_64")]

mod common;

use common::{kvm_available, spawn_store_blocking, test_bus, CLOCK_BASE};
use dh_devices::clock::REG_TIMER_DEADLINE;
use dh_devices::ctx::VecGuestMem;
use dh_devices::entropy::DetEntropy;
use dh_devices::{DevCtx, EntropySource};
use dh_inputlog::dhilog::{LogWriter, SegmentHeader};
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::kvm::{classify_exit, ExitEvent, KvmSystem, SlotVm};
use dh_vmm::{vcpu_state, SlotState};
use dh_worker::fork_engine::{fork_slot, ForkError};
use dh_worker::snapshot_engine::{take_snapshot, BoundaryState, PageSource};
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

/// All device snapshot blobs of a bus, in base order — bus state equality.
fn bus_state(bus: &dh_devices::MmioBus) -> Vec<(u16, Vec<u8>)> {
    bus.devices()
        .map(|(_b, d)| {
            let mut s = Vec::new();
            d.snapshot(&mut s);
            (d.device_id(), s)
        })
        .collect()
}

/// Drive one MMIO access against a bus outside a live VM (test-only
/// DevCtx plumbing; the bus only cares about a correct icount).
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

/// A parent slot with recognizable RAM, vCPU regs, NON-DEFAULT device
/// state (armed clock deadline — so bus equality below is not a
/// default-vs-default tautology), and an advanced PRNG, frozen and
/// ready to fork.
fn frozen_parent(sys: &KvmSystem) -> (SlotVm, dh_devices::MmioBus, DetEntropy, MachineConfig) {
    let slot = sys.create_slot_vm(MEM).unwrap();
    let mut bus = test_bus();
    with_ctx(0, |ctx| {
        bus.write(
            CLOCK_BASE + REG_TIMER_DEADLINE,
            &5_555_555u64.to_le_bytes(),
            ctx,
        )
        .unwrap();
    });
    slot.guest_mem
        .write_slice(&[0xAB; 64], GuestAddress(0x4000))
        .unwrap();
    let mut regs = slot.vcpu.get_regs().unwrap();
    regs.rbx = 0x000F_0CCA_CC1A_u64;
    regs.rip = 0x1234;
    slot.vcpu.set_regs(&regs).unwrap();
    let mut entropy = DetEntropy::from_seed([0x61; 32]);
    let mut burn = [0u8; 40];
    entropy.fill(&mut burn);
    slot.freeze_ram().expect("freeze parent RAM");
    (slot, bus, entropy, test_config())
}

#[test]
fn fork_inherits_the_exact_machine_and_cow_isolates_host_writes() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let sys = KvmSystem::open().unwrap();
    let (parent, bus_p, entropy_p, config) = frozen_parent(&sys);

    let mut bus_c = test_bus();
    let outcome = fork_slot(
        &sys,
        &parent,
        SlotState::Frozen,
        &bus_p,
        &entropy_p,
        &config,
        boundary(),
        &mut bus_c,
        None,
    )
    .expect("fork");

    // The child IS the parent's machine.
    assert_eq!(outcome.cumulative_icount, 1_000_000);
    assert_eq!(outcome.vns, 1_000_000);
    assert_eq!(outcome.epoch_index, 2);
    assert_eq!(outcome.chain.value(), [0xCA; 32]);
    assert_eq!(outcome.entropy.state(), entropy_p.state());
    assert_eq!(
        vcpu_state::capture(&outcome.child).unwrap(),
        vcpu_state::capture(&parent).unwrap()
    );
    assert_eq!(bus_state(&bus_c), bus_state(&bus_p));
    let mut ram = [0u8; 64];
    outcome
        .child
        .guest_mem
        .read_slice(&mut ram, GuestAddress(0x4000))
        .unwrap();
    assert_eq!(ram, [0xAB; 64]);

    // CoW isolation, host side: a write through the child's mapping must
    // never surface in the parent's bytes.
    outcome
        .child
        .guest_mem
        .write_slice(&[0x99; 64], GuestAddress(0x4000))
        .unwrap();
    let mut parent_ram = [0u8; 64];
    parent
        .guest_mem
        .read_slice(&mut parent_ram, GuestAddress(0x4000))
        .unwrap();
    assert_eq!(parent_ram, [0xAB; 64], "child write leaked into the parent");
}

#[test]
fn guest_writes_in_the_child_cow_and_never_reach_the_parent() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let sys = KvmSystem::open().unwrap();

    // Parent carries the writer program in RAM but never executes it.
    let parent = sys.create_slot_vm(MEM).unwrap();
    let bus_p = test_bus();
    parent
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
    let entropy_p = DetEntropy::from_seed([0x62; 32]);
    let config = test_config();
    parent.freeze_ram().expect("freeze");

    let mut bus_c = test_bus();
    let outcome = fork_slot(
        &sys,
        &parent,
        SlotState::Frozen,
        &bus_p,
        &entropy_p,
        &config,
        boundary(),
        &mut bus_c,
        None,
    )
    .expect("fork");
    let mut child = outcome.child;

    // Run the program IN THE CHILD: KVM faults the pages through the
    // private mapping — EPT-level CoW, the §8.4 hot path.
    let mut sregs = child.vcpu.get_sregs().unwrap();
    sregs.cs.base = 0;
    sregs.cs.selector = 0;
    child.vcpu.set_sregs(&sregs).unwrap();
    let mut regs = child.vcpu.get_regs().unwrap();
    regs.rip = 0;
    regs.rflags = 2;
    child.vcpu.set_regs(&regs).unwrap();
    // Three MOVs and a HLT, no MMIO and no dirty logging: the first exit
    // is the HLT or something is broken.
    let exit = classify_exit(child.vcpu.run().unwrap());
    assert!(matches!(exit, ExitEvent::Hlt), "unexpected exit: {exit:?}");

    for (gpa, want) in [(0x2000u64, 0x42u8), (0x5000, 0x43), (0x9000, 0x44)] {
        let mut b = [0u8; 1];
        child
            .guest_mem
            .read_slice(&mut b, GuestAddress(gpa))
            .unwrap();
        assert_eq!(b[0], want, "child guest write at {gpa:#x}");
        parent
            .guest_mem
            .read_slice(&mut b, GuestAddress(gpa))
            .unwrap();
        assert_eq!(b[0], 0, "guest write leaked into the frozen parent");
    }
}

#[test]
fn second_child_sees_the_pristine_parent_after_first_child_diverged() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let sys = KvmSystem::open().unwrap();
    let (parent, bus_p, entropy_p, config) = frozen_parent(&sys);

    let mut bus_c1 = test_bus();
    let child1 = fork_slot(
        &sys,
        &parent,
        SlotState::Frozen,
        &bus_p,
        &entropy_p,
        &config,
        boundary(),
        &mut bus_c1,
        None,
    )
    .expect("fork child1")
    .child;
    // Child 1 diverges hard.
    child1
        .guest_mem
        .write_slice(&[0xFF; 4096], GuestAddress(0x4000))
        .unwrap();

    // Child 2, forked AFTER the divergence, sees the parent's bytes —
    // the frozen parent is a stable fork base (a6s's reproducibility
    // property at the RAM level).
    let mut bus_c2 = test_bus();
    let child2 = fork_slot(
        &sys,
        &parent,
        SlotState::Frozen,
        &bus_p,
        &entropy_p,
        &config,
        boundary(),
        &mut bus_c2,
        None,
    )
    .expect("fork child2")
    .child;
    let mut ram = [0u8; 64];
    child2
        .guest_mem
        .read_slice(&mut ram, GuestAddress(0x4000))
        .unwrap();
    assert_eq!(ram, [0xAB; 64]);
    assert_eq!(
        vcpu_state::capture(&child2).unwrap(),
        vcpu_state::capture(&parent).unwrap()
    );
}

#[test]
fn fork_preconditions_fail_loudly() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let sys = KvmSystem::open().unwrap();
    let (parent, bus_p, entropy_p, config) = frozen_parent(&sys);

    // State-machine guard: anything but Frozen is refused (R9 — Paused
    // means the parent could still run while children share its pages).
    for state in [SlotState::Paused, SlotState::Running, SlotState::Empty] {
        let mut bus_c = test_bus();
        assert!(matches!(
            fork_slot(
                &sys,
                &parent,
                state,
                &bus_p,
                &entropy_p,
                &config,
                boundary(),
                &mut bus_c,
                None,
            ),
            Err(ForkError::ParentNotFrozen { .. })
        ));
    }

    // Boundary attestation.
    let mut b = boundary();
    b.agenda_empty = false;
    let mut bus_c = test_bus();
    assert!(matches!(
        fork_slot(
            &sys,
            &parent,
            SlotState::Frozen,
            &bus_p,
            &entropy_p,
            &config,
            b,
            &mut bus_c,
            None,
        ),
        Err(ForkError::AgendaNotEmpty)
    ));

    // Shape guard: a child bus that cannot consume the parent's DHSNAP
    // (here: no devices at all) is refused by the apply step — a fork
    // must never produce a differently-shaped machine.
    let mut empty_bus = dh_devices::MmioBus::new();
    match fork_slot(
        &sys,
        &parent,
        SlotState::Frozen,
        &bus_p,
        &entropy_p,
        &config,
        boundary(),
        &mut empty_bus,
        None,
    ) {
        Err(ForkError::Apply(m)) => assert!(m.contains("pv-entropy"), "{m}"),
        Err(e) => panic!("wrong error class: {e:?}"),
        Ok(_) => panic!("mis-shaped child bus must be rejected"),
    }

    // Kernel guard: a caller LYING about Frozen (state says Frozen, memfd
    // never sealed) is caught by fork_slot_vm's seal check — the two
    // guards are independent on purpose.
    let unsealed = sys.create_slot_vm(MEM).unwrap();
    let mut bus_c = test_bus();
    match fork_slot(
        &sys,
        &unsealed,
        SlotState::Frozen,
        &bus_p,
        &entropy_p,
        &config,
        boundary(),
        &mut bus_c,
        None,
    ) {
        Err(ForkError::Kvm(m)) => assert!(m.contains("UNFROZEN"), "{m}"),
        Err(e) => panic!("wrong error class: {e:?}"),
        Ok(_) => panic!("unsealed parent must not fork"),
    }
}

/// A CoW child's diverged pages live in anonymous memory the memfd never
/// sees — freezing or re-forking it would silently operate on the
/// PARENT's bytes. Both fail closed (iteration-77 review I1); the
/// documented path to a new fork base is TakeSnapshot + restore into a
/// fresh slot.
#[test]
fn cow_children_cannot_be_frozen_or_re_forked() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let sys = KvmSystem::open().unwrap();
    let (parent, bus_p, entropy_p, config) = frozen_parent(&sys);

    let mut bus_c = test_bus();
    let outcome = fork_slot(
        &sys,
        &parent,
        SlotState::Frozen,
        &bus_p,
        &entropy_p,
        &config,
        boundary(),
        &mut bus_c,
        None,
    )
    .expect("fork");
    assert!(outcome.child.ram_is_cow);

    match outcome.child.freeze_ram() {
        Err(e) => assert!(format!("{e:?}").contains("CoW"), "{e:?}"),
        Ok(()) => panic!("freezing a CoW child must fail closed"),
    }

    let mut bus_g = test_bus();
    match fork_slot(
        &sys,
        &outcome.child,
        SlotState::Frozen,
        &bus_c,
        &outcome.entropy,
        &config,
        boundary(),
        &mut bus_g,
        None,
    ) {
        Err(ForkError::Kvm(m)) => assert!(m.contains("CoW"), "{m}"),
        Err(e) => panic!("wrong error class: {e:?}"),
        Ok(_) => panic!("fork-of-fork must fail closed"),
    }
}

/// The identity the a6s ACCEPT builds on: snapshot(parent) and
/// snapshot(fork(parent)) are the SAME ref — the fork is exactly the
/// snapshot, delivered over CoW instead of the store.
#[test]
fn forked_child_snapshots_to_the_parents_exact_ref() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let sys = KvmSystem::open().unwrap();

    // Snapshot the parent while still Paused (snapshots require Paused;
    // freezing comes after, for the fork).
    let parent = sys.create_slot_vm(MEM).unwrap();
    let bus_p = test_bus();
    parent
        .guest_mem
        .write_slice(&[0x77; 32], GuestAddress(0x6000))
        .unwrap();
    let mut entropy_p = DetEntropy::from_seed([0x63; 32]);
    let mut burn = [0u8; 100];
    entropy_p.fill(&mut burn);
    let config = test_config();
    let parent_ref = take_snapshot(
        &parent,
        SlotState::Paused,
        &bus_p,
        &entropy_p,
        &config,
        boundary(),
        PageSource::Full,
        &store,
    )
    .expect("snapshot parent")
    .snapshot_ref;

    parent.freeze_ram().expect("freeze");
    let mut bus_c = test_bus();
    let outcome = fork_slot(
        &sys,
        &parent,
        SlotState::Frozen,
        &bus_p,
        &entropy_p,
        &config,
        boundary(),
        &mut bus_c,
        None,
    )
    .expect("fork");

    let child_ref = take_snapshot(
        &outcome.child,
        SlotState::Paused,
        &bus_c,
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
    .expect("snapshot child")
    .snapshot_ref;

    assert_eq!(
        child_ref, parent_ref,
        "fork is not snapshot-equivalent to its parent"
    );
}
