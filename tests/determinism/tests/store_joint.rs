//! R12 joint tests against the REAL snapshot-store (bead wbq; decision:
//! docs/decisions/snapstore-server-for-tests.md). The server is the real
//! `snapstore-server` spawned in-process on a `TempDir` over UDS — never a
//! mock. `spawn_store` is the shared seam the snapshot engine (qmp) and
//! M4 ACCEPT (6hg) build on.
#![cfg(target_arch = "x86_64")]

use snapstore_client::helpers::build_snapshot_container;
use snapstore_client::{SnapstoreClient, Transport};
use snapstore_manifest::DeviceBlob;
use snapstore_server::build_server::{serve_for_tests, ServerHandle};
use snapstore_server::config::ServerConfig;
use tempfile::TempDir;

const PAGE_SIZE: usize = 4096;

/// A fresh real store: hermetic per call, UDS-only, TempDir owns its life.
async fn spawn_store() -> (ServerHandle, SnapstoreClient, TempDir) {
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
        gc: Default::default(),
    };
    let (handle, uds_path) = serve_for_tests(config).await.expect("serve_for_tests");
    // Readiness probe instead of a fixed settle sleep: retry the connect
    // briefly (cold parallel startup can outlive any fixed delay).
    let mut client = None;
    for _ in 0..50 {
        match SnapstoreClient::connect(Transport::Uds(uds_path.clone())).await {
            Ok(c) => {
                client = Some(c);
                break;
            }
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
        }
    }
    let client = client.expect("server did not become ready within 500ms");
    // LIFETIME NOTES for engine authors (qmp/6hg): dropping `ServerHandle`
    // initiates graceful shutdown — keep it alive for the test's duration;
    // same for the TempDir (it owns the store). The synchronous KVM engine
    // should use snapstore_client::blocking::SnapstoreClient (the sibling's
    // sync-async bridge for vCPU worker loops) rather than going async.
    (handle, client, dir)
}

fn page(fill: u8) -> Box<[u8; PAGE_SIZE]> {
    let mut p = Box::new([fill; PAGE_SIZE]);
    // Make pages distinct beyond the fill byte so dedup can't collapse them.
    p[0] = fill.wrapping_add(1);
    p
}

fn blob(bytes: Vec<u8>) -> DeviceBlob {
    DeviceBlob {
        format: 1, // DHSNAP once qmp wires it; opaque to the store
        zstd: false,
        raw_len: bytes.len() as u64,
        bytes,
    }
}

/// FULL-manifest container over contiguous pages 0..N (the builder
/// requires completeness for FULL).
fn full_container(pages: &[Box<[u8; PAGE_SIZE]>], device: Vec<u8>) -> Vec<u8> {
    let refs: Vec<(u64, &[u8; PAGE_SIZE])> = pages
        .iter()
        .enumerate()
        .map(|(i, p)| (i as u64, &**p))
        .collect();
    build_snapshot_container(None, (pages.len() * PAGE_SIZE) as u64, &refs, blob(device))
        .expect("build container")
}

/// The R12 ground truth: pages and a manifest go in, the ref comes back
/// only after the store has them, and the container returns byte-identical.
#[tokio::test]
async fn put_pages_put_snapshot_get_snapshot_roundtrip() {
    let (_handle, client, _dir) = spawn_store().await;

    let pages = vec![page(0x11), page(0x22), page(0x33)];
    let wire: Vec<(u64, Vec<u8>)> = pages
        .iter()
        .enumerate()
        .map(|(i, p)| (i as u64, p.to_vec()))
        .collect();
    // Tuple is (pages_new, pages_deduped). The SPLIT is not assertable on
    // a fresh put: the client wraps the content-idempotent upload in
    // with_retry (Transport/Unavailable/DeadlineExceeded), and a retry
    // after a server-side commit reports everything deduped. Observed
    // exactly once — (0, 3) on this fresh put during a COLD parallel
    // first run; 246 warm attempts could not reproduce it (iteration-72
    // review), so treat the flip as rare-but-real. The SUM is the
    // invariant.
    let (new, deduped) = client.put_pages(wire).await.expect("put_pages");
    assert_eq!(new + deduped, 3, "every page accounted for");

    // A real FULL .spm container via the client's own helper surface:
    // put_snapshot stores it byte-identically and returns the ref.
    let container = full_container(&pages, vec![0xD5; 64]);

    let snapshot_ref = client
        .put_snapshot(container.clone())
        .await
        .expect("put_snapshot");

    // The ref round-trips to the SAME bytes: durability before ref (R12).
    let back = client
        .get_snapshot(snapshot_ref)
        .await
        .expect("get_snapshot");
    assert_eq!(back, container, "stored container must be byte-identical");
}

/// Idempotent re-put: same pages dedup to zero new, same container yields
/// the same ref (content addressing is the lineage backbone).
#[tokio::test]
async fn re_put_is_deduped_and_ref_stable() {
    let (_handle, client, _dir) = spawn_store().await;

    let pages = vec![page(0x44), page(0x55)];
    let wire: Vec<(u64, Vec<u8>)> = pages
        .iter()
        .enumerate()
        .map(|(i, p)| (i as u64, p.to_vec()))
        .collect();
    client.put_pages(wire.clone()).await.expect("first put");
    let (new, deduped) = client.put_pages(wire).await.expect("second put");
    assert_eq!(new, 0, "a re-put can never store new pages");
    assert_eq!(deduped, 2, "second put must dedup everything");

    let container = full_container(&pages, vec![0xD6; 32]);
    let r1 = client.put_snapshot(container.clone()).await.expect("put 1");
    let r2 = client.put_snapshot(container).await.expect("put 2");
    assert_eq!(r1, r2, "same container, same ref");
}

/// Two hermetic stores share nothing: a ref minted in one is NOT_FOUND in
/// the other (the TempDir isolation the decision doc promises).
#[tokio::test]
async fn stores_are_hermetically_isolated() {
    let (_h1, client1, _d1) = spawn_store().await;
    let (_h2, client2, _d2) = spawn_store().await;

    let pages = vec![page(0x66)];
    client1
        .put_pages(vec![(0u64, pages[0].to_vec())])
        .await
        .expect("put");
    let container = full_container(&pages, vec![0xD7; 16]);
    let r = client1.put_snapshot(container).await.expect("put_snapshot");

    assert!(
        client2.get_snapshot(r).await.is_err(),
        "a fresh store must not know the other store's ref"
    );
}
