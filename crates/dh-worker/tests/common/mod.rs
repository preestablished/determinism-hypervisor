//! Shared rig for dh-worker's live joint tests (snapshot_engine,
//! restore_engine, m4_transparency): the KVM probe, the REAL in-process
//! snapshot-store (R12; docs/decisions/snapstore-server-for-tests.md),
//! and the canonical 4-device test bus. Every test target is x86_64-gated
//! at its crate root, so this module never compiles elsewhere.

use dh_devices::clock::PvClock;
use dh_devices::entropy::PvEntropy;
use dh_devices::pad::PvPad;
use dh_devices::serial::DebugSerial;
use dh_devices::MmioBus;
use snapstore_client::blocking::SnapstoreClient as BlockingClient;
use snapstore_client::Transport;
use snapstore_server::build_server::{serve_for_tests, ServerHandle};
use snapstore_server::config::ServerConfig;
use tempfile::TempDir;

pub const CLOCK_BASE: u64 = 0xD000_2000;

pub fn kvm_available() -> bool {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/kvm")
        .is_ok()
}

/// Real store on a side runtime; the engines stay synchronous and reach
/// it via the blocking facade (the production shape).
pub fn spawn_store_blocking() -> (
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
    // Readiness probe (same shape as tests/determinism/store_joint.rs).
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

/// The canonical joint-test bus: pad, clock, entropy, serial — one device
/// per DHSNAP tag the engines exercise, deterministic base order.
pub fn test_bus() -> MmioBus {
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
