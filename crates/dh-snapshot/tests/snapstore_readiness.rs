//! M4 store-integration readiness gate (bead 4nj, risk R12).
//!
//! Pins, at compile time, that the sibling `snapstore-client` crate
//! (`../snapshot-store/crates/snapstore-client`) exposes the
//! TakeSnapshot/RestoreSnapshot-supporting surface described in
//! `.agents/docs/snapshot-store/API.md` §1. If snapshot-store renames or
//! removes any of these, this workspace's build breaks *here*, with a
//! readable name, instead of deep inside the M4 snapshot engine.
//!
//! The page channel (API.md §1.2 `hashes_only` / localpath fast path) is
//! internal to the client — it is selected via `Transport::Auto`'s
//! `page_channel_path` field, which the Transport pin below covers.

use snapstore_client::{blocking, ClientError, SnapstoreClient, Transport};

/// Referencing inherent methods as function items fails to compile if a
/// method disappears or becomes private. Signatures are pinned where cheap
/// (no sibling type names needed in this crate's namespace).
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

    // Blocking facade for non-tokio KVM vCPU worker loops.
    let _ = blocking::SnapstoreClient::connect;
    let _ = blocking::SnapstoreClient::put_pages;
    let _ = blocking::SnapstoreClient::put_snapshot;
    let _ = blocking::SnapstoreClient::get_snapshot;
    let _ = blocking::SnapstoreClient::resolve_pages;
}

/// Transport configuration surface: UDS / TCP / Auto with the reserved
/// page-channel path (API.md §0: UDS at /run/snapstore/grpc.sock on-box).
fn _transport_pins(uds: std::path::PathBuf, tcp: String) -> [Transport; 3] {
    [
        Transport::Auto {
            uds_path: uds.clone(),
            tcp_addr: tcp.clone(),
            page_channel_path: Some(uds.clone()),
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

#[test]
fn snapstore_client_surface_is_present() {
    // The real assertions are the compile-time pins above; this test exists
    // so the gate shows up in `cargo test` output as an explicit pass.
}
