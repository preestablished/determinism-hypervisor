//! M4 store-integration readiness gate (bead 4nj, risk R12).
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
//! The page channel (localpath fast path) is internal to the client and not
//! pinnable from here: `Transport::Auto`'s `page_channel_path` field is
//! reserved for the M5/WI3 page-channel arm and currently unused at runtime
//! (the sibling's `Transport::connect` destructures it as `_`). The
//! Transport pin below locks only the field's existence and type, so the
//! WI3 seam stays where INTEGRATION.md expects it.

use snapstore_client::{blocking, ClientError, SnapstoreClient, Transport};

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

/// Transport configuration surface: UDS / TCP / Auto with the reserved
/// page-channel path (API.md §0: UDS at /run/snapstore/grpc.sock on-box).
/// Both `page_channel_path` shapes are pinned: `Some` for the WI3 fast-path
/// arm, `None` for the plain-gRPC engine construction.
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

fn _error_pins(e: ClientError) -> impl std::error::Error {
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
    let _ = _put_pages_signature;
    let _ = _manifest_leg_signatures;
}
