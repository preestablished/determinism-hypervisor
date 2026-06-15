//! M4 store-integration readiness gate (bead 4nj, risk R12; bead qwx).
//!
//! Pins, at compile time, that the sibling `snapstore-client` crate
//! (`../snapshot-store/crates/snapstore-client`) exposes the
//! TakeSnapshot/RestoreSnapshot-supporting surface described in
//! `.agents/docs/snapshot-store/API.md` §1. If snapshot-store renames or
//! removes any of these, this workspace's build breaks *here*, with a
//! readable name, instead of deep inside the M4 snapshot engine. The legs
//! the engine commits to first (`put_pages`, `put_snapshot`, `get_snapshot`,
//! `resolve_pages`) get full signature pins; the rest are existence pins
//! that catch renames/removals/visibility changes but not signature drift.
//!
//! The live test below pins the store contract this crate depends on without
//! going through dh-worker's snapshot engine: pages first, then a manifest
//! container, then hashes-only planning and byte-identical manifest fetch.
//! The page channel (localpath fast path) is internal to the client and not
//! directly pinnable from here: `Transport::Auto`'s `page_channel_path`
//! enables the sibling's SEQPACKET fast path for `put_pages` when a live
//! page-channel socket is present. The Transport pin below locks the field's
//! existence and type; dh-worker's store fixture owns the live-path coverage.

use snapstore_client::{
    blocking,
    snapstore_proto::{
        snapshot_store_client::SnapshotStoreClient as RawSnapstoreClient, ResolvePagesRequest,
    },
    ClientError, SnapstoreClient, Transport,
};
use snapstore_manifest::{DeviceBlob, Manifest, ManifestEntry};
use snapstore_server::{
    build_server::{serve_for_tests, ServerHandle},
    config::ServerConfig,
};
use snapstore_types::{PageHash, SnapshotRef, PAGE_SIZE};
use tempfile::TempDir;

/// Referencing inherent methods as function items fails to compile if a
/// method disappears or becomes private. Signatures are pinned separately
/// below where the M4 engine commits to them.
fn _surface_pins() {
    // Connection surface.
    let _ = SnapstoreClient::connect;
    let _ = SnapstoreClient::from_channel;

    // Take-side (TakeSnapshot orchestration: harvest → pages → manifest).
    let _ = SnapstoreClient::put_pages;
    let _ = SnapstoreClient::has_pages;
    let _ = SnapstoreClient::put_snapshot;
    let _ = SnapstoreClient::put_snapshot_from_parts;

    // Restore-side (RestoreSnapshot: manifest → pages).
    let _ = SnapstoreClient::get_snapshot;
    let _ = SnapstoreClient::resolve_pages;

    // Input-log legs (DHILOG containers ride the same store).
    let _ = SnapstoreClient::put_input_log;
    let _ = SnapstoreClient::get_input_log;

    // Blocking facade for non-tokio KVM vCPU worker loops — same legs as
    // the async pins above, input logs included.
    let _ = blocking::SnapstoreClient::connect;
    let _ = blocking::SnapstoreClient::put_pages;
    let _ = blocking::SnapstoreClient::put_snapshot;
    let _ = blocking::SnapstoreClient::get_snapshot;
    let _ = blocking::SnapstoreClient::resolve_pages;
    let _ = blocking::SnapstoreClient::put_input_log;
    let _ = blocking::SnapstoreClient::get_input_log;
}

/// Transport configuration surface: UDS / TCP / Auto with the optional
/// page-channel path (API.md §0: UDS at /run/snapstore/grpc.sock on-box).
/// Both `page_channel_path` shapes are pinned: `Some` for the fast-path arm,
/// `None` for plain-gRPC construction.
fn _transport_pins(uds: std::path::PathBuf, tcp: String) -> [Transport; 4] {
    [
        Transport::Auto {
            uds_path: uds.clone(),
            tcp_addr: tcp.clone(),
            page_channel_path: Some(uds.clone()),
        },
        Transport::Auto {
            uds_path: uds.clone(),
            tcp_addr: tcp.clone(),
            page_channel_path: None,
        },
        Transport::Uds(uds),
        Transport::Tcp(tcp),
    ]
}

fn _error_variant_pins(e: &ClientError) {
    match e {
        ClientError::MissingPages {
            page_hashes,
            parent_ref,
        } => {
            let _: &Vec<PageHash> = page_hashes;
            let _: &Option<SnapshotRef> = parent_ref;
        }
        ClientError::MissingNodes { node_ids } => {
            let _: &Vec<u64> = node_ids;
        }
        ClientError::CasFailed { current_generation } => {
            let _: &u64 = current_generation;
        }
        ClientError::AlreadyExists => {}
        ClientError::Status(status) => {
            let _ = status.code();
        }
        ClientError::Transport(message) => {
            let _: &String = message;
        }
        ClientError::CorruptPayload(detail) => {
            let _ = (&detail.context, &detail.expected, &detail.actual);
        }
        ClientError::BatchBlake3Mismatch { expected, actual } => {
            let _: (&String, &String) = (expected, actual);
        }
        ClientError::CorruptInputLog { expected, actual } => {
            let _: (&String, &String) = (expected, actual);
        }
    }
}

struct LiveStore {
    rt: tokio::runtime::Runtime,
    handle: Option<ServerHandle>,
    client: blocking::SnapstoreClient,
    uds_path: std::path::PathBuf,
    _dir: TempDir,
}

impl LiveStore {
    fn resolve_pages_raw_hashes_only(
        &self,
        snapshot_ref: SnapshotRef,
        baseline_ref: Option<SnapshotRef>,
    ) -> Vec<(u64, PageHash, Vec<u8>)> {
        let uds_path = self.uds_path.clone();
        self.rt.block_on(async move {
            let channel = Transport::Uds(uds_path)
                .connect()
                .await
                .expect("raw transport connect");
            let mut client = RawSnapstoreClient::new(channel);
            let mut stream = client
                .resolve_pages(ResolvePagesRequest {
                    snapshot_ref: snapshot_ref.to_bytes().to_vec(),
                    baseline_ref: baseline_ref
                        .map(|r| r.to_bytes().to_vec())
                        .unwrap_or_default(),
                    hashes_only: true,
                })
                .await
                .expect("raw resolve_pages")
                .into_inner();

            let mut pages = Vec::new();
            while let Some(chunk) = stream.message().await.expect("raw resolve chunk") {
                for page in chunk.pages {
                    let hash: [u8; 32] = page
                        .page_hash
                        .as_slice()
                        .try_into()
                        .expect("raw page hash must be 32 bytes");
                    pages.push((page.page_index, PageHash::from_bytes(hash), page.payload));
                }
            }
            pages
        })
    }
}

impl Drop for LiveStore {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.shutdown();
        }
    }
}

fn spawn_live_store() -> LiveStore {
    let dir = TempDir::new().expect("tempdir");
    let data_root = dir.path().to_path_buf();
    let config = ServerConfig {
        data_root: data_root.clone(),
        grpc_tcp_addr: "127.0.0.1:0".parse().expect("grpc addr"),
        grpc_uds_path: Some(data_root.join("snapstore.sock")),
        page_channel_path: None,
        http_addr: "127.0.0.1:0".parse().expect("http addr"),
        pagestore: Default::default(),
        meta: Default::default(),
        page_channel: Default::default(),
    };

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let (handle, uds_path) = rt
        .block_on(serve_for_tests(config))
        .expect("serve_for_tests");
    let client = blocking::SnapstoreClient::connect(Transport::Uds(uds_path.clone()))
        .expect("snapstore client connect");

    LiveStore {
        rt,
        handle: Some(handle),
        client,
        uds_path,
        _dir: dir,
    }
}

fn page(index: u64, fill: u8) -> Vec<u8> {
    let mut data = vec![fill; PAGE_SIZE];
    data[..8].copy_from_slice(&index.to_le_bytes());
    data[PAGE_SIZE - 1] = fill.wrapping_mul(17);
    data
}

fn page_hash(data: &[u8]) -> PageHash {
    PageHash::from_bytes(*blake3::hash(data).as_bytes())
}

fn empty_blob() -> DeviceBlob {
    let bytes = b"dh-snapshot-readiness".to_vec();
    DeviceBlob {
        format: 0,
        zstd: false,
        raw_len: bytes.len() as u64,
        bytes,
    }
}

fn full_container(pages: &[(u64, Vec<u8>)]) -> Vec<u8> {
    let entries: Vec<ManifestEntry> = pages
        .iter()
        .map(|(idx, data)| ManifestEntry {
            page_index: *idx,
            page_hash: page_hash(data),
        })
        .collect();
    Manifest::new_full(pages.len() as u64 * PAGE_SIZE as u64, entries, empty_blob())
        .expect("full manifest")
        .encode()
}

fn sample_pages() -> Vec<(u64, Vec<u8>)> {
    vec![(2, page(2, 0x33)), (0, page(0, 0x11)), (1, page(1, 0x22))]
}

fn expected_pages(pages: &[(u64, Vec<u8>)]) -> Vec<(u64, PageHash, &[u8])> {
    let mut expected: Vec<_> = pages
        .iter()
        .map(|(idx, data)| (*idx, page_hash(data), data.as_slice()))
        .collect();
    expected.sort_by_key(|(idx, _, _)| *idx);
    expected
}

fn _error_pins(e: ClientError) -> impl std::error::Error {
    _error_variant_pins(&e);
    e
}

/// Signature pin for the page-upload leg: the (deduped, total) counts and the
/// `Vec<(gpa_page_index, page_bytes)>` shape the M4 engine will rely on.
fn _put_pages_signature(
    client: &SnapstoreClient,
    pages: Vec<(u64, Vec<u8>)>,
) -> impl std::future::Future<Output = Result<(u64, u64), ClientError>> + '_ {
    client.put_pages(pages)
}

/// Signature pins for the manifest legs, chained so the snapshot-ref type
/// flows from `put_snapshot` into `resolve_pages`/`get_snapshot` by inference
/// — pins argument order and shapes without naming `snapstore_types` types
/// (no second dev-dep needed):
/// - `put_snapshot(Vec<u8>) -> SnapshotRef`
/// - `resolve_pages(ref, baseline: Option<ref>, hashes_only: bool)
///   -> Vec<(page_index: u64, page_hash, payload: Option<bytes>)>`
///   (`hashes_only: true` ⇒ payloads omitted, API.md §1.2)
/// - `get_snapshot(ref) -> Vec<u8>` (the stored container, byte-identical)
async fn _manifest_leg_signatures(
    client: &SnapstoreClient,
    container: Vec<u8>,
) -> Result<Vec<u8>, ClientError> {
    let snapshot_ref = client.put_snapshot(container).await?;
    let pages = client
        .resolve_pages(snapshot_ref.clone(), Some(snapshot_ref.clone()), true)
        .await?;
    let _page_indices: Vec<u64> = pages.iter().map(|(idx, _hash, _payload)| *idx).collect();
    client.get_snapshot(snapshot_ref).await
}

#[test]
fn snapstore_client_surface_is_present() {
    // The real assertions are the compile-time pins above; this test exists
    // so the gate shows up in `cargo test` output as an explicit pass. The
    // references below keep the pin functions from looking deletable.
    let _ = _surface_pins;
    let _ = _transport_pins;
    let _ = _error_pins;
    let _ = _error_variant_pins;
    let _ = _put_pages_signature;
    let _ = _manifest_leg_signatures;
}

#[test]
fn live_snapstore_roundtrip_pins_hashes_only_contract() {
    let store = spawn_live_store();
    let pages = sample_pages();
    let expected = expected_pages(&pages);

    assert_eq!(
        store.client.put_pages(pages.clone()).expect("put_pages"),
        (pages.len() as u64, 0)
    );

    let container = full_container(&pages);
    let expected_ref = Manifest::snapshot_ref(&container);
    let snapshot_ref = store
        .client
        .put_snapshot(container.clone())
        .expect("put_snapshot");
    assert_eq!(snapshot_ref, expected_ref);

    let fetched = store
        .client
        .get_snapshot(snapshot_ref.clone())
        .expect("get_snapshot");
    assert_eq!(fetched, container);

    let resolved_full = store
        .client
        .resolve_pages(snapshot_ref.clone(), None, false)
        .expect("resolve full");
    assert_eq!(resolved_full.len(), expected.len());
    for ((idx, hash, payload), (expected_idx, expected_hash, expected_payload)) in
        resolved_full.iter().zip(expected.iter())
    {
        assert_eq!(*idx, *expected_idx);
        assert_eq!(*hash, *expected_hash);
        assert_eq!(payload.as_deref(), Some(*expected_payload));
    }

    let resolved_hashes = store
        .client
        .resolve_pages(snapshot_ref.clone(), None, true)
        .expect("resolve hashes_only");
    assert_eq!(resolved_hashes.len(), expected.len());
    for ((idx, hash, payload), (expected_idx, expected_hash, _expected_payload)) in
        resolved_hashes.iter().zip(expected.iter())
    {
        assert_eq!(*idx, *expected_idx);
        assert_eq!(*hash, *expected_hash);
        assert!(payload.is_none(), "hashes_only must omit page payloads");
    }

    let raw_hashes = store.resolve_pages_raw_hashes_only(snapshot_ref.clone(), None);
    assert_eq!(raw_hashes.len(), expected.len());
    for ((idx, hash, payload), (expected_idx, expected_hash, _expected_payload)) in
        raw_hashes.iter().zip(expected.iter())
    {
        assert_eq!(*idx, *expected_idx);
        assert_eq!(*hash, *expected_hash);
        assert!(
            payload.is_empty(),
            "hashes_only must omit payload bytes on the wire"
        );
    }

    let same_snapshot_delta = store
        .client
        .resolve_pages(snapshot_ref.clone(), Some(snapshot_ref.clone()), true)
        .expect("resolve same snapshot baseline");
    assert!(
        same_snapshot_delta.is_empty(),
        "same-snapshot baseline should have no page delta"
    );
    let raw_same_snapshot_delta =
        store.resolve_pages_raw_hashes_only(snapshot_ref.clone(), Some(snapshot_ref));
    assert!(
        raw_same_snapshot_delta.is_empty(),
        "same-snapshot baseline should have no raw page delta"
    );
}

#[test]
fn live_snapstore_missing_pages_error_is_typed_and_complete() {
    let store = spawn_live_store();
    let pages = sample_pages();
    let uploaded = pages
        .iter()
        .find(|(idx, _data)| *idx == 1)
        .expect("uploaded page")
        .clone();
    assert_eq!(
        store
            .client
            .put_pages(vec![uploaded])
            .expect("seed one page"),
        (1, 0)
    );
    let expected_hashes: Vec<PageHash> = expected_pages(&pages)
        .into_iter()
        .filter(|(idx, _hash, _data)| *idx != 1)
        .map(|(_idx, hash, _data)| hash)
        .collect();
    let err = store
        .client
        .put_snapshot(full_container(&pages))
        .expect_err("put_snapshot must reject manifests that reference absent pages");

    assert!(err.is_non_retryable());
    match err {
        ClientError::MissingPages {
            page_hashes,
            parent_ref,
        } => {
            assert_eq!(parent_ref, None);
            assert_eq!(page_hashes, expected_hashes);
        }
        other => panic!("expected MissingPages, got {other:?}"),
    }
}
