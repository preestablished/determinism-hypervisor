//! LAPC persistence coverage for the deterministic userspace lAPIC model.
#![cfg(target_arch = "x86_64")]

mod common;

use common::{kvm_available, spawn_store_blocking, test_bus};
use dh_devices::entropy::DetEntropy;
use dh_snapshot::dhsnap::{Container, ContainerWriter, LapcSection, tag};
use dh_vmm::SlotState;
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::kvm::KvmSystem;
use dh_vmm::lapic::{LocalApic, XAPIC_MMIO_BASE};
use dh_worker::fork_engine::fork_slot_with_lapic;
use dh_worker::restore_engine::{RestoreError, restore_snapshot};
use dh_worker::snapshot_compare::compare_snapshots;
use dh_worker::snapshot_engine::{
    BoundaryState, DEVICE_BLOB_FORMAT_DHSNAP, PageSource, take_snapshot_with_lapic,
};
use snapstore_manifest::DeviceBlob;
use snapstore_types::SnapshotRef;

const MEM: u64 = 2 * 1024 * 1024;

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

fn non_reset_lapic() -> LocalApic {
    let mut lapic = LocalApic::new();
    lapic
        .write_mmio(XAPIC_MMIO_BASE + 0x80, &0x44u32.to_le_bytes())
        .unwrap();
    lapic
        .write_mmio(XAPIC_MMIO_BASE + 0xd0, &0x0102_0304u32.to_le_bytes())
        .unwrap();
    lapic
        .write_mmio(XAPIC_MMIO_BASE + 0xf0, &0x0000_01ffu32.to_le_bytes())
        .unwrap();
    lapic.accept_interrupt(0x41).unwrap();
    lapic
}

fn snapshot_dhsnap(
    store: &snapstore_client::blocking::SnapstoreClient,
    snapshot_ref: SnapshotRef,
) -> Vec<u8> {
    let container = store.get_snapshot(snapshot_ref).unwrap();
    let manifest = snapstore_manifest::Manifest::decode(&container).unwrap();
    manifest.device_blob.bytes
}

fn rebuild(orig: &[u8], mutate: impl FnOnce(&mut Vec<([u8; 4], u16, Vec<u8>)>)) -> Vec<u8> {
    let c = Container::parse(orig).expect("orig parses");
    let mut sections: Vec<([u8; 4], u16, Vec<u8>)> = c
        .sections()
        .map(|s| (s.tag, s.sec_version, s.contents.to_vec()))
        .collect();
    mutate(&mut sections);
    let mut w = ContainerWriter::new();
    for (tag, version, contents) in &sections {
        w.push_section(*tag, *version, contents).unwrap();
    }
    w.finish()
}

#[test]
fn lapc_snapshot_restore_roundtrips_non_reset_state() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let sys = KvmSystem::open().unwrap();
    let config = test_config();
    let slot = sys.create_slot_vm(MEM).unwrap();
    let bus = test_bus();
    let entropy = DetEntropy::from_seed([0x42; 32]);
    let lapic = non_reset_lapic();

    let snap = take_snapshot_with_lapic(
        &slot,
        SlotState::Paused,
        &bus,
        &lapic,
        &entropy,
        &config,
        boundary(),
        PageSource::Full,
        &store,
    )
    .expect("take_snapshot_with_lapic");

    let dhsnap = snapshot_dhsnap(&store, snap.snapshot_ref.clone());
    let parsed = Container::parse(&dhsnap).unwrap();
    let lapc = parsed.get(tag::LAPC).unwrap();
    assert_eq!(lapc.sec_version, LapcSection::VERSION);
    let decoded = LapcSection::decode(lapc.contents, lapc.sec_version).unwrap();
    assert_eq!(LocalApic::from_lapc_section(decoded).unwrap(), lapic);

    let restored_slot = sys.create_slot_vm(MEM).unwrap();
    let mut restored_bus = test_bus();
    let restored = restore_snapshot(
        &restored_slot,
        SlotState::Paused,
        &mut restored_bus,
        &config,
        snap.snapshot_ref.clone(),
        None,
        None,
        &store,
    )
    .expect("restore_snapshot");
    assert_eq!(restored.lapic, lapic);

    let resnap = take_snapshot_with_lapic(
        &restored_slot,
        SlotState::Paused,
        &restored_bus,
        &restored.lapic,
        &restored.entropy,
        &config,
        BoundaryState {
            icount: restored.cumulative_icount,
            vns: restored.vns,
            epoch_index: restored.epoch_index,
            hash_chain: restored.chain.value(),
            agenda_empty: true,
        },
        PageSource::Full,
        &store,
    )
    .expect("resnapshot");
    assert_eq!(resnap.snapshot_ref, snap.snapshot_ref);
}

#[test]
fn lapc_restore_rejects_malformed_sections() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let sys = KvmSystem::open().unwrap();
    let config = test_config();
    let slot = sys.create_slot_vm(MEM).unwrap();
    let bus = test_bus();
    let entropy = DetEntropy::from_seed([0x43; 32]);
    let lapic = non_reset_lapic();
    let snap = take_snapshot_with_lapic(
        &slot,
        SlotState::Paused,
        &bus,
        &lapic,
        &entropy,
        &config,
        boundary(),
        PageSource::Full,
        &store,
    )
    .unwrap();
    let valid = snapshot_dhsnap(&store, snap.snapshot_ref);
    let zero_pages: Vec<(u64, Vec<u8>)> = (0..MEM / dh_vmm::dirty::PAGE_SIZE)
        .map(|idx| (idx, vec![0u8; dh_vmm::dirty::PAGE_SIZE as usize]))
        .collect();
    let put = |bytes: Vec<u8>| {
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
            .unwrap()
    };
    let restore_err = |snapshot_ref| {
        let slot = sys.create_slot_vm(MEM).unwrap();
        let mut bus = test_bus();
        match restore_snapshot(
            &slot,
            SlotState::Paused,
            &mut bus,
            &config,
            snapshot_ref,
            None,
            None,
            &store,
        ) {
            Err(e) => e,
            Ok(_) => panic!("malformed LAPC snapshot must be rejected"),
        }
    };

    let bad_len = put(rebuild(&valid, |sections| {
        let lapc = sections
            .iter_mut()
            .find(|(section_tag, _, _)| *section_tag == tag::LAPC)
            .unwrap();
        lapc.2 = vec![0x01];
    }));
    assert!(matches!(restore_err(bad_len), RestoreError::Codec(m) if m.contains("LAPC")));

    let bad_semantics = put(rebuild(&valid, |sections| {
        let lapc = sections
            .iter_mut()
            .find(|(section_tag, _, _)| *section_tag == tag::LAPC)
            .unwrap();
        lapc.2[136..140].copy_from_slice(&0u32.to_le_bytes());
    }));
    assert!(matches!(restore_err(bad_semantics), RestoreError::Codec(m) if m.contains("LAPC")));
}

#[test]
fn lapc_fork_inherits_parent_lapic_state() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let sys = KvmSystem::open().unwrap();
    let config = test_config();
    let parent = sys.create_slot_vm(MEM).unwrap();
    let parent_bus = test_bus();
    let parent_entropy = DetEntropy::from_seed([0x44; 32]);
    let parent_lapic = non_reset_lapic();
    parent.freeze_ram().unwrap();

    let mut child_bus = test_bus();
    let outcome = fork_slot_with_lapic(
        &sys,
        &parent,
        SlotState::Frozen,
        &parent_bus,
        &parent_lapic,
        &parent_entropy,
        &config,
        boundary(),
        None,
        &mut child_bus,
        None,
    )
    .expect("fork_slot_with_lapic");
    assert_eq!(outcome.lapic, parent_lapic);
}

#[test]
fn lapc_snapshot_compare_reports_lapic_differences() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let sys = KvmSystem::open().unwrap();
    let config = test_config();
    let slot = sys.create_slot_vm(MEM).unwrap();
    let bus = test_bus();
    let entropy = DetEntropy::from_seed([0x45; 32]);
    let reset = LocalApic::new();
    let changed = non_reset_lapic();

    let expected = take_snapshot_with_lapic(
        &slot,
        SlotState::Paused,
        &bus,
        &reset,
        &entropy,
        &config,
        boundary(),
        PageSource::Full,
        &store,
    )
    .unwrap();
    let actual = take_snapshot_with_lapic(
        &slot,
        SlotState::Paused,
        &bus,
        &changed,
        &entropy,
        &config,
        boundary(),
        PageSource::Full,
        &store,
    )
    .unwrap();
    let comparison = compare_snapshots(&store, expected.snapshot_ref, actual.snapshot_ref).unwrap();
    assert!(comparison.diff_page_idx.is_empty());
    assert!(comparison.reg_diffs.iter().any(|diff| diff.name == "lapic"));
}
