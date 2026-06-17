//! Criterion latency distributions for the M4 perf surfaces (bead 9sb):
//! tier-A fork, incremental snapshot at 8k dirty pages, tier-B warm
//! restore — all on the 128 MiB canonical guest. This target is the
//! NIGHTLY TREND instrument; the ignored test in tests/perf_gates.rs
//! records the same surfaces as telemetry while correctness remains the
//! hard gate.
//!
//! Skips cleanly (empty benchmark set) without /dev/kvm. The crate root
//! is NOT cfg-gated: a harness=false bench must link a main() on every
//! arch (a cfg'd-empty file is E0601 on aarch64 dev hosts) — the x86
//! body lives in the cfg'd module below.

#![allow(clippy::type_complexity)]

#[cfg(target_arch = "x86_64")]
#[path = "../tests/common/mod.rs"]
mod common;

#[cfg(target_arch = "x86_64")]
mod imp {
    use std::time::Duration;

    use super::common::{kvm_available, spawn_store_blocking, test_bus};
    use criterion::{BatchSize, Criterion};
    use dh_devices::entropy::DetEntropy;
    use dh_vmm::config::{BootSpec, MachineConfig};
    use dh_vmm::dirty::{enable_dirty_logging, DirtyPageSet, DirtyRing};
    use dh_vmm::kvm::KvmSystem;
    use dh_vmm::SlotState;
    use dh_worker::fork_engine::fork_slot;
    use dh_worker::restore_engine::restore_snapshot;
    use dh_worker::snapshot_engine::{take_snapshot, BoundaryState, PageSource};
    use vm_memory::{Bytes, GuestAddress};

    const MEM: u64 = 128 << 20;
    const DIRTY_PAGES: u64 = 8192;

    fn config_128() -> MachineConfig {
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

    pub fn run() {
        if !kvm_available() {
            eprintln!("skipping perf benches: /dev/kvm not usable");
            return;
        }
        let mut c = Criterion::default().configure_from_args();

        let (_rt, _handle, store, _dir) = spawn_store_blocking();
        let sys = KvmSystem::open().unwrap();
        let config = config_128();

        // fork: frozen 128 MiB parent, one CoW child per iteration.
        let parent = sys.create_slot_vm(MEM).unwrap();
        parent
            .guest_mem
            .write_slice(&[0xAB; 4096], GuestAddress(0x10_0000))
            .unwrap();
        let bus_p = test_bus();
        let entropy_p = DetEntropy::from_seed([0x42; 32]);
        parent.freeze_ram().expect("freeze parent RAM");
        c.bench_function("fork_128mib", |b| {
            b.iter_batched(
                test_bus,
                |mut bus_c| {
                    fork_slot(
                        &sys,
                        &parent,
                        SlotState::Frozen,
                        &bus_p,
                        &entropy_p,
                        &config,
                        boundary(),
                        None,
                        &mut bus_c,
                        None,
                    )
                    .expect("fork")
                },
                BatchSize::PerIteration,
            )
        });

        // incremental snapshot: 8k-page dirty set per iteration. Content
        // VARIES per sample (sample counter mixed into every page) — the
        // pagestore dedups globally by content hash, so identical bytes
        // would make every sample after the first a dedup hit and this
        // would measure the manifest path, not the cold 8k-page write
        // (iteration-99 review I1). Cost: the tempdir store grows ~32 MiB
        // per sample; sample counts here are kept small for exactly that.
        let slot = sys.create_slot_vm(MEM).unwrap();
        let bus = test_bus();
        let entropy = DetEntropy::from_seed([0x43; 32]);
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
        enable_dirty_logging(&slot).expect("dirty logging");

        let mut group = c.benchmark_group("snapshot");
        group
            .sample_size(10)
            .measurement_time(Duration::from_secs(3))
            .warm_up_time(Duration::from_millis(500));
        let mut sample = 0u8;
        group.bench_function("incremental_8k_pages_128mib", |b| {
            b.iter(|| {
                // Setup inside the iteration but the fill + insert are
                // noise next to 32 MiB of durable I/O; the test gate
                // (tests/perf_gates.rs) keeps them out of its window.
                sample = sample.wrapping_add(1);
                for page in 0..DIRTY_PAGES {
                    slot.guest_mem
                        .write_slice(&[(page as u8) ^ sample], GuestAddress(page * 4096))
                        .unwrap();
                    dirty.insert(page).unwrap();
                }
                take_snapshot(
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
                .expect("incremental snapshot")
            })
        });
        group.finish();

        // warm restore: fresh slot per iteration OUTSIDE the timed window
        // (RestoreSnapshot targets an existing slot, §8.3). Warm page
        // cache across iterations is the CORRECT tier-B regime.
        let mut group = c.benchmark_group("restore");
        group
            .sample_size(10)
            .measurement_time(Duration::from_secs(5))
            .warm_up_time(Duration::from_millis(500));
        group.bench_function("warm_full_128mib", |b| {
            b.iter_batched(
                || (sys.create_slot_vm(MEM).unwrap(), test_bus()),
                |(fresh, mut bus_f)| {
                    restore_snapshot(
                        &fresh,
                        SlotState::Paused,
                        &mut bus_f,
                        &config,
                        root.snapshot_ref.clone(),
                        None,
                        None,
                        &store,
                    )
                    .expect("restore")
                },
                BatchSize::PerIteration,
            )
        });
        group.finish();

        c.final_summary();
    }
}

#[cfg(target_arch = "x86_64")]
fn main() {
    imp::run()
}

// harness=false must still link a main on every arch (E0601 otherwise).
#[cfg(not(target_arch = "x86_64"))]
fn main() {}
