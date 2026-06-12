//! Joint regression for bead 0vl: a 32 MiB FULL snapshot
//! (8192 pages via `put_snapshot_from_parts`) used to hang the blocking
//! store client forever — the caller sat in ep_poll inside the client's
//! current_thread runtime, server idle, zero CPU, no error. Root cause
//! was in the sibling's `put_pages`: it pre-filled a bounded(16) mpsc
//! channel with all chunks BEFORE tonic polled the receiver, so the
//! 17th chunk-send parked forever. 16 MiB (4096 pages = exactly 16
//! chunks of 256) masked it, which is why entr_golden passed while the
//! 32 MiB ring-chaos slot hung. This pins the joint at the size that
//! deadlocked, with a watchdog so a regression fails loudly instead of
//! wedging the suite. MATTERS FOR 9sb: the M4 perf acceptance ships a
//! 128 MiB guest (128 chunks) through this exact path.
//!
//! No KVM needed — the hang lived entirely in client/store plumbing —
//! but the crate root stays x86_64-gated like every dh-worker joint
//! target (common/mod.rs only compiles there).

#![cfg(target_arch = "x86_64")]

mod common;

use common::spawn_store_blocking;
use snapstore_manifest::DeviceBlob;

const PAGES: u64 = 8192; // 32 MiB — the size that deadlocked
const PAGE: usize = 4096;

#[test]
fn full_32mib_put_snapshot_from_parts_completes() {
    // kvm_available is irrelevant here; reference it so this target keeps
    // compiling the shared rig warning-free.
    let _ = common::kvm_available();

    let (_rt, _handle, store, _dir) = spawn_store_blocking();

    let (done_tx, done_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        // Distinct page contents: dedup must not shrink what goes over
        // the wire below the deadlocking chunk count.
        let pages: Vec<(u64, Vec<u8>)> = (0..PAGES)
            .map(|i| {
                let mut data = vec![0u8; PAGE];
                data[..8].copy_from_slice(&i.to_le_bytes());
                (i, data)
            })
            .collect();
        let blob = DeviceBlob {
            format: 0,
            zstd: false,
            bytes: b"0vl-regression".to_vec(),
            raw_len: 14,
        };
        let result = store.put_snapshot_from_parts(None, PAGES * PAGE as u64, pages, blob);
        let sref = result.expect("put_snapshot_from_parts");
        // Roundtrip on the same client: the ref must serve back a
        // manifest referencing every page.
        let container = store.get_snapshot(sref).expect("get_snapshot");
        let manifest = snapstore_manifest::Manifest::decode(&container).expect("manifest");
        assert_eq!(manifest.entries.len() as u64, PAGES);
        assert_eq!(manifest.guest_ram_bytes, PAGES * PAGE as u64);
        let _ = done_tx.send(());
    });

    // The worker is deliberately NOT joined: on regression it is wedged in
    // ep_poll forever, and a join would turn this watchdog back into the
    // unbounded hang it exists to prevent. 120s (vs the client's ~30s retry
    // budget) so only the non-retryable deadlock can trip it.
    done_rx
        .recv_timeout(std::time::Duration::from_secs(120))
        .expect("32 MiB FULL put hung >120s — bead 0vl regressed");
}
