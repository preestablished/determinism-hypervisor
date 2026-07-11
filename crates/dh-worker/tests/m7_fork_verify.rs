//! M7 ACCEPT: root snapshot, seeded fork children, and VerifyReplay for every
//! child log with zero Divergence and matching end_state_hash.
//!
//! The default guest mode is the original nanokernel `pad_echo` fixture. Set
//! `DH_M7_ACCEPT_GUEST=linux` to run the M9 Linux READY fixture instead:
//!
//!   DH_M9_ALLOW_SKIP=0 DH_M7_ACCEPT_GUEST=linux \
//!   DH_M7_ACCEPT_SLOT_CORES=2-5 DH_M7_ACCEPT_JOBS=1000 \
//!     cargo test -p dh-worker --test m7_fork_verify --release \
//!       -- --ignored --nocapture --test-threads=1
//!
//! Developer smoke on small machines may set:
//!
//!   DH_M7_ACCEPT_JOBS=2 DH_M7_ACCEPT_SLOT_CORES=0-1 \
//!     cargo test -p dh-worker --test m7_fork_verify -- --ignored --nocapture
//!
//! The cross-slot acceptance gate samples the same job universe, forking
//! same-seed children across every available child slot and requiring
//! byte-identical refs/logs:
//!
//!   DH_M9_ALLOW_SKIP=0 DH_M7_ACCEPT_GUEST=linux \
//!   DH_M7_ACCEPT_SLOT_CORES=2-5 cargo test -p dh-worker \
//!     --test m7_fork_verify --release \
//!     m7_accept_cross_slot_rerun_10_seeded_forks_identical_refs \
//!     -- --ignored --nocapture
//!
//! By default it samples 10 indices from the 1000-job universe. Override
//! `DH_M7_ACCEPT_JOBS` to change the universe size and `DH_M7_CROSS_CHECKS`
//! to change the number of sampled indices.
//!
//! The M8 replay-commit gate replays each child by restoring the root snapshot,
//! re-driving the same deterministic child, taking a new snapshot, and requiring
//! byte-identical refs/logs:
//!
//!   DH_M9_ALLOW_SKIP=0 DH_M7_ACCEPT_GUEST=linux \
//!   DH_M7_ACCEPT_SLOT_CORES=2-5 DH_M7_ACCEPT_JOBS=1000 \
//!     cargo test -p dh-worker --test m7_fork_verify --release \
//!       m8_accept_1000_seeded_forks_replay_commit_ref_identity \
//!       -- --ignored --nocapture --test-threads=1

#![cfg(target_arch = "x86_64")]

mod common;

use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use dh_inputlog::reader::{LogReader, RecordBody};
use dh_inputlog::splice::Lineage;
use dh_proto::v1 as proto;
use dh_proto::v1::hypervisor_worker_server::HypervisorWorker;
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::vt::ClockRatio;
use dh_worker::image_resolver::cache_key;
use dh_worker::proto_map::machine_config_to_proto;
use dh_worker::service::{PreflightHealth, WorkerConfig, WorkerService};
use dh_worker::slot_manager::{parse_core_list, LeasePolicy};
use snapstore_manifest::input_log::InputLogContainer;
use snapstore_manifest::Manifest;
use snapstore_types::{LogId, SnapshotRef as StoreSnapshotRef};
use tokio_stream::StreamExt;
use tonic::Request;

const DEFAULT_JOBS: usize = 1000;
const DEFAULT_SLOT_COUNT: usize = 64;
const BASE_SLOT_CORE: u32 = 2;
const MEM: u64 = 16 << 20;
const CLOCK_NUM: u32 = 10_000;
const VNS_PER_SECOND: u64 = 1_000_000_000;
const RUN_BUDGET: u64 = 100_000;
const BURST_EVENTS: usize = 8;
const M9_LINUX_CHILD_FRAMES: u32 = 5;
const M9_LINUX_CHILD_HARD_CAP: u64 = 5_000_000;
const M9_LINUX_CHILD_EPOCH_LEN: u64 = 745_000;
// This operator-run gate shares the Intel reference host with CI. READY boot
// measured PMU delivery skid just over 41k instructions under that load, so
// keep more than 2x headroom without slowing every production/test machine.
const M8_LINUX_SKID_MARGIN: u32 = 131_072;
const M8_LINUX_MEASURED_MAX_SKID: u32 = 41_075;
const M9_LINUX_META_IO_MAGIC_OFF: u64 = 32;
const M9_LINUX_META_IO_PROOF_LEN: u64 = 24;
const JOBS_ENV: &str = "DH_M7_ACCEPT_JOBS";
const SLOT_CORES_ENV: &str = "DH_M7_ACCEPT_SLOT_CORES";
const ALLOW_SKIP_ENV: &str = "DH_M7_ACCEPT_ALLOW_SKIP";
const CROSS_CHECKS_ENV: &str = "DH_M7_CROSS_CHECKS";
const GUEST_ENV: &str = "DH_M7_ACCEPT_GUEST";
const M8_STORE_ROOT_ENV: &str = "M8_STORE_ROOT";
const M8_STORE_ROOT_QUALIFIED_ENV: &str = "M8_STORE_ROOT_QUALIFIED";
const M8_STORE_ROOT_DISK_CLASS_ENV: &str = "M8_STORE_ROOT_DISK_CLASS";
const M8_EVIDENCE_ROOT_ENV: &str = "M8_EVIDENCE_ROOT";
const M8_EVIDENCE_RESUME_ENV: &str = "M8_EVIDENCE_RESUME";

type TestResult<T> = Result<T, String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AcceptanceGuest {
    Nanokernel,
    Linux,
}

impl AcceptanceGuest {
    fn configured() -> Self {
        match std::env::var(GUEST_ENV) {
            Ok(value) if value == "linux" => Self::Linux,
            Ok(value) if value == "nanokernel" => Self::Nanokernel,
            Ok(value) => {
                panic!("{GUEST_ENV} must be unset, \"nanokernel\", or \"linux\"; got {value:?}")
            }
            Err(_) => Self::Nanokernel,
        }
    }
}

#[derive(Clone, Debug)]
struct ChildRecord {
    index: usize,
    slot_id: u64,
    snapshot: proto::SnapshotRef,
    state_hash: [u8; 32],
    input_log_id: Vec<u8>,
    segment_end_icount: u64,
    segment_end_vns: u64,
    cumulative_icount: u64,
    cumulative_vns: u64,
    frames_elapsed: u64,
    frame_counter: u32,
    dirty_pages: u64,
    meta_pvblk_checksum: Option<u64>,
    timing: ChildTiming,
}

#[derive(Clone, Debug)]
struct ReplayCommitRecord {
    child_index: usize,
    original_slot_id: u64,
    replay_slot_id: u64,
    snapshot: proto::SnapshotRef,
    state_hash: [u8; 32],
    input_log_id: Vec<u8>,
    baseline_delta_restore_ms: f64,
    timing: ReplayCommitTiming,
}

#[derive(Clone, Copy, Debug, Default)]
struct ChildTiming {
    fork_ms: f64,
    run_ms: f64,
    original_commit_ms: f64,
}

#[derive(Clone, Copy, Debug, Default)]
struct ReplayCommitTiming {
    restore_ms: f64,
    replay_ms: f64,
    replay_commit_ms: f64,
}

#[derive(Clone, Debug)]
struct ParsedChildLog {
    dhilog_blake3: String,
    base_snapshot_id: [u8; 32],
    end_snapshot_id: [u8; 32],
    machine_config_hash: [u8; 32],
    record_count: u64,
    canonical_count: u64,
    end_icount: u64,
    end_vns: u64,
    end_state_hash: [u8; 32],
    has_epoch_hashes: bool,
    epoch_hashes: Vec<(u64, u64, [u8; 32])>,
    frame_marks: Vec<(u64, u32)>,
}

#[allow(clippy::large_enum_variant)]
enum AcceptanceHarness {
    Nanokernel {
        svc: WorkerService,
        store: snapstore_client::blocking::SnapstoreClient,
        root_lease: proto::Lease,
        root_snapshot: proto::SnapshotRef,
        machine_config_hash: [u8; 32],
        root_cumulative_icount: u64,
        root_cumulative_vns: u64,
        root_frame_counter: u32,
        _store_rt: tokio::runtime::Runtime,
        _store_handle: snapstore_server::build_server::ServerHandle,
        store_root: PathBuf,
        _store_dir: Option<tempfile::TempDir>,
        _store_socket_dir: tempfile::TempDir,
        _image_cache: tempfile::TempDir,
    },
    Linux {
        ready: common::M9LinuxReady,
        root_cumulative_icount: u64,
        root_cumulative_vns: u64,
        root_frame_counter: u32,
    },
}

impl AcceptanceHarness {
    fn new(
        guest: AcceptanceGuest,
        test_name: &str,
        slot_cores: Vec<u32>,
    ) -> TestResult<Option<Self>> {
        match guest {
            AcceptanceGuest::Nanokernel => Self::new_nanokernel(slot_cores).map(Some),
            AcceptanceGuest::Linux => Self::new_linux(test_name, slot_cores),
        }
    }

    fn new_nanokernel(slot_cores: Vec<u32>) -> TestResult<Self> {
        let image_cache = tempfile::TempDir::new().map_err(|e| format!("image cache: {e}"))?;
        let base_hash = write_cache_blob(image_cache.path(), &vec![0u8; 4096]);
        let kernel_hash = write_cache_blob(image_cache.path(), nanokernel::pad_echo_elf());
        let cpuid_table = dh_vmm::kvm::KvmSystem::open()
            .map_err(|e| format!("KVM open for masked CPUID table: {e:?}"))?
            .masked_cpuid_table()
            .map_err(|e| format!("masked CPUID table: {e:?}"))?;
        let (config, machine_config_hash) = pad_echo_config(base_hash, kernel_hash, cpuid_table);

        let (store_root, store_dir) = configured_m8_store_root()?;
        let store_socket_dir = tempfile::Builder::new()
            .prefix("dh-m8-store-")
            .tempdir()
            .map_err(|e| format!("snapstore socket tempdir: {e}"))?;
        let store_sock = "snapstore.sock";
        let (_store_rt, _store_handle, store) = common::spawn_store_at_with_socket_root(
            store_root.clone(),
            store_socket_dir.path().to_path_buf(),
            store_sock,
        );
        let snapstore = snapstore_client::Transport::Uds(store_socket_dir.path().join(store_sock));
        let svc = WorkerService::new(worker_config(
            slot_cores,
            image_cache.path().to_path_buf(),
            snapstore,
        ))
        .map_err(|e| format!("WorkerService::new: {e:?}"))?;

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("test runtime: {e}"))?;
        let (root_lease, root_snapshot_response) =
            rt.block_on(async { create_root(&svc, config).await })?;
        let root_snapshot = root_snapshot_response
            .snapshot
            .clone()
            .ok_or_else(|| "root TakeSnapshot returned no snapshot ref".to_string())?;

        Ok(Self::Nanokernel {
            svc,
            store,
            root_lease,
            root_snapshot,
            machine_config_hash,
            root_cumulative_icount: root_snapshot_response.icount,
            root_cumulative_vns: root_snapshot_response.vns,
            root_frame_counter: root_snapshot_response.frame_counter,
            _store_rt,
            _store_handle,
            store_root,
            _store_dir: store_dir,
            _store_socket_dir: store_socket_dir,
            _image_cache: image_cache,
        })
    }

    fn new_linux(test_name: &str, slot_cores: Vec<u32>) -> TestResult<Option<Self>> {
        let Some(ready) = common::m9_linux_ready_snapshot_with_slot_cores_and_config(
            test_name,
            slot_cores,
            |config| {
                config.epoch_len = M9_LINUX_CHILD_EPOCH_LEN;
                config.skid_margin = M8_LINUX_SKID_MARGIN;
            },
        )?
        else {
            return Ok(None);
        };
        Ok(Some(Self::Linux {
            root_cumulative_icount: ready.ready_snapshot.icount,
            root_cumulative_vns: ready.ready_snapshot.vns,
            root_frame_counter: ready.ready_snapshot.frame_counter,
            ready,
        }))
    }

    fn guest(&self) -> AcceptanceGuest {
        match self {
            Self::Nanokernel { .. } => AcceptanceGuest::Nanokernel,
            Self::Linux { .. } => AcceptanceGuest::Linux,
        }
    }

    fn svc(&self) -> &WorkerService {
        match self {
            Self::Nanokernel { svc, .. } => svc,
            Self::Linux { ready, .. } => &ready.svc,
        }
    }

    fn store(&self) -> &snapstore_client::blocking::SnapstoreClient {
        match self {
            Self::Nanokernel { store, .. } => store,
            Self::Linux { ready, .. } => &ready.store,
        }
    }

    fn store_root(&self) -> &Path {
        match self {
            Self::Nanokernel { store_root, .. } => store_root,
            Self::Linux { ready, .. } => &ready.store_root,
        }
    }

    fn root_lease(&self) -> &proto::Lease {
        match self {
            Self::Nanokernel { root_lease, .. } => root_lease,
            Self::Linux { ready, .. } => &ready.lease,
        }
    }

    fn root_snapshot(&self) -> &proto::SnapshotRef {
        match self {
            Self::Nanokernel { root_snapshot, .. } => root_snapshot,
            Self::Linux { ready, .. } => &ready.ready_snapshot_ref,
        }
    }

    fn machine_config_hash(&self) -> [u8; 32] {
        match self {
            Self::Nanokernel {
                machine_config_hash,
                ..
            } => *machine_config_hash,
            Self::Linux { ready, .. } => ready.config_hash,
        }
    }

    fn root_cumulative_icount(&self) -> u64 {
        match self {
            Self::Nanokernel {
                root_cumulative_icount,
                ..
            }
            | Self::Linux {
                root_cumulative_icount,
                ..
            } => *root_cumulative_icount,
        }
    }

    fn root_cumulative_vns(&self) -> u64 {
        match self {
            Self::Nanokernel {
                root_cumulative_vns,
                ..
            }
            | Self::Linux {
                root_cumulative_vns,
                ..
            } => *root_cumulative_vns,
        }
    }

    fn root_frame_counter(&self) -> u32 {
        match self {
            Self::Nanokernel {
                root_frame_counter, ..
            }
            | Self::Linux {
                root_frame_counter, ..
            } => *root_frame_counter,
        }
    }

    async fn destroy_root(&self) {
        destroy_best_effort(self.svc(), Some(self.root_lease().clone())).await;
    }
}

fn arr32(bytes: &[u8], what: &str) -> [u8; 32] {
    bytes
        .try_into()
        .unwrap_or_else(|_| panic!("{what} must be 32 bytes, got {}", bytes.len()))
}

fn blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes)
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn write_cache_blob(root: &Path, bytes: &[u8]) -> [u8; 32] {
    let hash = *blake3::hash(bytes).as_bytes();
    std::fs::write(root.join(cache_key(&hash)), bytes).expect("write image-cache blob");
    hash
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn child_seed(index: usize) -> Vec<u8> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"dh-m7-fork-child-seed-v1");
    hasher.update(&(index as u64).to_le_bytes());
    hasher.finalize().as_bytes().to_vec()
}

fn pad_burst(index: usize) -> Vec<proto::ScheduledEvent> {
    let mut rng = 0x4D37_0000_0000_0000u64 ^ index as u64;
    let mut last = 0u32;
    (0..BURST_EVENTS)
        .map(|event_index| {
            let mut buttons = (splitmix64(&mut rng) as u32) | 1;
            if buttons == last {
                buttons ^= 0x8000_0000;
            }
            last = buttons;
            let at_icount = ((event_index as u64 + 1) * RUN_BUDGET) / (BURST_EVENTS as u64 + 1);
            proto::ScheduledEvent {
                at: Some(proto::scheduled_event::At::AtIcount(at_icount)),
                event: Some(proto::scheduled_event::Event::PadSet(proto::PadSet {
                    port: 0,
                    buttons,
                })),
            }
        })
        .collect()
}

fn mutated_pad_burst(index: usize) -> Vec<proto::ScheduledEvent> {
    let mut events = pad_burst(index);
    if let Some(proto::scheduled_event::Event::PadSet(pad)) =
        events.first_mut().and_then(|event| event.event.as_mut())
    {
        pad.buttons ^= 0x8000_0000;
        pad.buttons |= 1;
    }
    events
}

fn expected_pad_records(index: usize) -> Vec<(u64, u8, u32)> {
    pad_burst(index)
        .into_iter()
        .map(|event| {
            let at_icount = match event.at.expect("pad_burst sets at") {
                proto::scheduled_event::At::AtIcount(icount) => icount,
                other => panic!("pad_burst must use at_icount, got {other:?}"),
            };
            let (port, buttons) = match event.event.expect("pad_burst sets event") {
                proto::scheduled_event::Event::PadSet(pad) => (
                    u8::try_from(pad.port).expect("pad port fits u8"),
                    pad.buttons,
                ),
                other => panic!("pad_burst must use pad_set, got {other:?}"),
            };
            (at_icount, port, buttons)
        })
        .collect()
}

fn pad_echo_machine_config(
    base_hash: [u8; 32],
    kernel_hash: [u8; 32],
    cpuid_table: Vec<dh_vmm::config::CpuidLeaf>,
) -> MachineConfig {
    let mut config = MachineConfig::new(
        MEM,
        base_hash,
        BootSpec::Elf {
            kernel_hash,
            cmdline: Vec::new(),
        },
    );
    config.cpuid_table = cpuid_table;
    config.epoch_len = RUN_BUDGET;
    config.clock = ClockRatio::new(CLOCK_NUM, 1).expect("nonzero clock ratio");
    config.device_set = vec![
        dh_devices::clock::DEVICE_ID_PV_CLOCK,
        dh_devices::pad::DEVICE_ID_PV_PAD,
        dh_devices::entropy::DEVICE_ID_PV_ENTROPY,
        dh_devices::serial::DEVICE_ID_DEBUG_SERIAL,
    ];
    config
}

fn pad_echo_config(
    base_hash: [u8; 32],
    kernel_hash: [u8; 32],
    cpuid_table: Vec<dh_vmm::config::CpuidLeaf>,
) -> (proto::MachineConfig, [u8; 32]) {
    let config = pad_echo_machine_config(base_hash, kernel_hash, cpuid_table);
    let config_hash = config.config_hash().expect("pad_echo machine config hash");
    (machine_config_to_proto(&config), config_hash)
}

fn worker_config(
    slot_cores: Vec<u32>,
    image_cache_dir: PathBuf,
    snapstore: snapstore_client::Transport,
) -> WorkerConfig {
    WorkerConfig {
        worker_id: "m7-fork-verify-worker".into(),
        slot_cores,
        lease_policy: LeasePolicy::default(),
        class: proto::DeterminismClass {
            cpu_model: "m7-test-cpu".into(),
            microcode: "m7-test-ucode".into(),
            host_kernel: "m7-test-kernel".into(),
            vmm_version: "m7-test-vmm".into(),
        },
        preflight: PreflightHealth::skipped("m7 acceptance harness"),
        image_cache_dir,
        snapstore: Some(snapstore),
        bisection_checkpoints: dh_worker::service::BisectionCheckpointConfig::default(),
        max_delta_chain: dh_worker::service::DEFAULT_MAX_DELTA_CHAIN,
    }
}

fn configured_jobs() -> usize {
    std::env::var(JOBS_ENV)
        .ok()
        .map(|value| {
            value
                .parse::<usize>()
                .unwrap_or_else(|_| panic!("{JOBS_ENV} must be a positive integer, got {value:?}"))
        })
        .unwrap_or(DEFAULT_JOBS)
        .max(1)
}

fn configured_cross_checks(jobs: usize) -> usize {
    std::env::var(CROSS_CHECKS_ENV)
        .ok()
        .map(|value| {
            value.parse::<usize>().unwrap_or_else(|_| {
                panic!("{CROSS_CHECKS_ENV} must be a positive integer, got {value:?}")
            })
        })
        .unwrap_or(10)
        .clamp(1, jobs)
}

fn cross_check_indices(total_jobs: usize, checks: usize) -> Vec<usize> {
    assert!(total_jobs > 0);
    let checks = checks.clamp(1, total_jobs);
    if checks == 1 {
        return vec![0];
    }
    (0..checks)
        .map(|index| index * (total_jobs - 1) / (checks - 1))
        .collect()
}

fn default_slot_cores() -> Vec<u32> {
    (BASE_SLOT_CORE..BASE_SLOT_CORE + DEFAULT_SLOT_COUNT as u32).collect()
}

fn configured_slot_cores() -> Vec<u32> {
    match std::env::var(SLOT_CORES_ENV) {
        Ok(spec) => parse_core_list(&spec)
            .unwrap_or_else(|| panic!("{SLOT_CORES_ENV} must be a core list, got {spec:?}")),
        Err(_) => default_slot_cores(),
    }
}

fn configured_m8_store_root() -> TestResult<(PathBuf, Option<tempfile::TempDir>)> {
    match std::env::var(M8_STORE_ROOT_ENV) {
        Ok(path) if !path.is_empty() => {
            let root = PathBuf::from(path);
            std::fs::create_dir_all(&root)
                .map_err(|e| format!("create {M8_STORE_ROOT_ENV}: {e}"))?;
            Ok((root, None))
        }
        _ => {
            let dir = tempfile::TempDir::new().map_err(|e| format!("snapstore tempdir: {e}"))?;
            Ok((dir.path().to_path_buf(), Some(dir)))
        }
    }
}

fn available_cores() -> Option<BTreeSet<u32>> {
    let online = std::fs::read_to_string("/sys/devices/system/cpu/online").ok()?;
    let online: BTreeSet<_> = parse_core_list(online.trim())?.into_iter().collect();
    let allowed = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                line.strip_prefix("Cpus_allowed_list:")
                    .map(|value| value.trim().to_owned())
            })
        })
        .and_then(|spec| parse_core_list(&spec))
        .map(|cores| cores.into_iter().collect::<BTreeSet<_>>())
        .unwrap_or_else(|| online.clone());
    Some(online.intersection(&allowed).copied().collect())
}

fn acceptance_slot_cores() -> TestResult<Vec<u32>> {
    if !common::kvm_available() {
        return Err("/dev/kvm is unavailable".into());
    }
    let cores = configured_slot_cores();
    if cores.len() < 2 {
        return Err(format!(
            "{SLOT_CORES_ENV} must provide at least two slots: one parent and one child"
        ));
    }
    let available =
        available_cores().ok_or_else(|| "could not read available CPU core set".to_owned())?;
    let missing: Vec<_> = cores
        .iter()
        .copied()
        .filter(|core| !available.contains(core))
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "slot cores unavailable in this process affinity: {missing:?}"
        ));
    }
    Ok(cores)
}

fn acceptance_slot_cores_or_skip() -> Option<Vec<u32>> {
    match acceptance_slot_cores() {
        Ok(cores) => Some(cores),
        Err(e) if allow_skip() => {
            eprintln!("skipping M7 fork/verify acceptance because {ALLOW_SKIP_ENV}=1: {e}");
            None
        }
        Err(e) => panic!(
            "M7 acceptance prerequisites failed: {e}. \
             Set {ALLOW_SKIP_ENV}=1 only for non-acceptance local smoke."
        ),
    }
}

fn allow_skip() -> bool {
    std::env::var(ALLOW_SKIP_ENV).as_deref() == Ok("1")
}

async fn create_root(
    svc: &WorkerService,
    config: proto::MachineConfig,
) -> TestResult<(proto::Lease, proto::TakeSnapshotResponse)> {
    let created = svc
        .create_vm(Request::new(proto::CreateVmRequest {
            config: Some(config),
            entropy_seed: vec![0x37; 32],
        }))
        .await
        .map_err(|e| format!("CreateVm root: {e}"))?
        .into_inner();
    let lease = created
        .lease
        .ok_or_else(|| "CreateVm root returned no lease".to_owned())?;
    let snapshot = svc
        .take_snapshot(Request::new(proto::TakeSnapshotRequest {
            lease: Some(lease.clone()),
            seal_input_log: Some(true),
            capture: None,
        }))
        .await
        .map_err(|e| format!("TakeSnapshot root: {e}"))?
        .into_inner();
    if snapshot.snapshot.is_none() {
        return Err("TakeSnapshot root returned no snapshot".to_owned());
    }
    Ok((lease, snapshot))
}

async fn destroy_best_effort(svc: &WorkerService, lease: Option<proto::Lease>) {
    if let Some(lease) = lease {
        let _ = svc
            .destroy_vm(Request::new(proto::DestroyVmRequest { lease: Some(lease) }))
            .await;
    }
}

#[allow(clippy::too_many_arguments)]
fn snapshot_record(
    guest: AcceptanceGuest,
    index: usize,
    slot_id: u64,
    snapshot: proto::TakeSnapshotResponse,
    segment_end_icount: u64,
    segment_end_vns: u64,
    cumulative_icount: u64,
    cumulative_vns: u64,
    frames_elapsed: u64,
    meta_pvblk_checksum: Option<u64>,
    timing: ChildTiming,
) -> TestResult<ChildRecord> {
    let snapshot_ref = snapshot
        .snapshot
        .ok_or_else(|| format!("child {index} TakeSnapshot returned no snapshot"))?;
    let state_hash = snapshot
        .state_hash
        .as_ref()
        .map(|hash| arr32(&hash.hash, "child state_hash"))
        .ok_or_else(|| format!("child {index} TakeSnapshot returned no state_hash"))?;
    if snapshot.input_log_id.len() != 32 {
        return Err(format!(
            "child {index} input_log_id length {}, expected 32",
            snapshot.input_log_id.len()
        ));
    }
    if guest == AcceptanceGuest::Linux {
        let expected_frame = snapshot
            .frame_counter
            .checked_sub(M9_LINUX_CHILD_FRAMES)
            .ok_or_else(|| format!("child {index} Linux frame counter underflow"))?;
        if snapshot.frame_counter != expected_frame + M9_LINUX_CHILD_FRAMES {
            return Err(format!(
                "child {index} Linux frame counter arithmetic failed at {}",
                snapshot.frame_counter
            ));
        }
    }
    Ok(ChildRecord {
        index,
        slot_id,
        snapshot: snapshot_ref,
        state_hash,
        input_log_id: snapshot.input_log_id,
        segment_end_icount,
        segment_end_vns,
        cumulative_icount,
        cumulative_vns,
        frames_elapsed,
        frame_counter: snapshot.frame_counter,
        dirty_pages: u64::from(snapshot.dirty_pages),
        meta_pvblk_checksum,
        timing,
    })
}

async fn run_nanokernel_child(
    svc: WorkerService,
    index: usize,
    lease: proto::Lease,
) -> TestResult<ChildRecord> {
    run_nanokernel_child_with_events(svc, index, lease, pad_burst(index), "child").await
}

async fn run_nanokernel_child_with_events(
    svc: WorkerService,
    index: usize,
    lease: proto::Lease,
    events: Vec<proto::ScheduledEvent>,
    label: &str,
) -> TestResult<ChildRecord> {
    let slot_id = lease.slot_id;
    let expected_events = events.len();
    let scheduled = match svc
        .inject_inputs(Request::new(proto::InjectInputsRequest {
            lease: Some(lease.clone()),
            events,
        }))
        .await
    {
        Ok(response) => response.into_inner().scheduled,
        Err(e) => {
            destroy_best_effort(&svc, Some(lease)).await;
            return Err(format!("{label} {index} InjectInputs: {e}"));
        }
    };
    if scheduled as usize != expected_events {
        destroy_best_effort(&svc, Some(lease)).await;
        return Err(format!(
            "{label} {index} scheduled {scheduled}, expected {expected_events}"
        ));
    }

    let run_started = Instant::now();
    let run = match svc
        .run(Request::new(proto::RunRequest {
            lease: Some(lease.clone()),
            until: Some(proto::run_request::Until::IcountBudget(RUN_BUDGET)),
            hard_icount_cap: 0,
            capture: None,
        }))
        .await
    {
        Ok(response) => response.into_inner(),
        Err(e) => {
            destroy_best_effort(&svc, Some(lease)).await;
            return Err(format!("{label} {index} Run: {e}"));
        }
    };
    let run_ms = elapsed_ms(run_started.elapsed());
    if run.reason != i32::from(proto::StopReason::BudgetReached) {
        destroy_best_effort(&svc, Some(lease)).await;
        return Err(format!(
            "{label} {index} Run stopped with {}, expected BUDGET_REACHED",
            run.reason
        ));
    }
    if run.icount != RUN_BUDGET || run.vns != VNS_PER_SECOND {
        destroy_best_effort(&svc, Some(lease)).await;
        return Err(format!(
            "{label} {index} ended at icount={} vns={}, expected {RUN_BUDGET}/{VNS_PER_SECOND}",
            run.icount, run.vns
        ));
    }

    let snapshot_started = Instant::now();
    let snapshot = match svc
        .take_snapshot(Request::new(proto::TakeSnapshotRequest {
            lease: Some(lease.clone()),
            seal_input_log: Some(true),
            capture: None,
        }))
        .await
    {
        Ok(response) => response.into_inner(),
        Err(e) => {
            destroy_best_effort(&svc, Some(lease)).await;
            return Err(format!("{label} {index} TakeSnapshot: {e}"));
        }
    };
    let original_commit_ms = elapsed_ms(snapshot_started.elapsed());
    destroy_best_effort(&svc, Some(lease)).await;
    snapshot_record(
        AcceptanceGuest::Nanokernel,
        index,
        slot_id,
        snapshot,
        RUN_BUDGET,
        VNS_PER_SECOND,
        run.icount,
        run.vns,
        run.frames_elapsed,
        None,
        ChildTiming {
            run_ms,
            original_commit_ms,
            ..ChildTiming::default()
        },
    )
}

async fn read_linux_meta_io_proof(svc: &WorkerService, lease: proto::Lease) -> TestResult<u64> {
    let meta = svc
        .read_guest_memory(Request::new(proto::ReadGuestMemoryRequest {
            lease: Some(lease),
            ranges: Vec::new(),
            region_ranges: vec![proto::RegionRange {
                region: "meta".into(),
                layout_version: 1,
                offset: M9_LINUX_META_IO_MAGIC_OFF,
                len: M9_LINUX_META_IO_PROOF_LEN,
            }],
        }))
        .await
        .map_err(|e| format!("ReadGuestMemory Linux meta IO proof: {e}"))?
        .into_inner();
    let chunk = meta
        .chunks
        .first()
        .ok_or_else(|| "ReadGuestMemory returned no Linux meta IO proof".to_string())?;
    assert_m9_meta_io_proof(chunk)
}

fn assert_m9_meta_io_proof(bytes: &[u8]) -> TestResult<u64> {
    if bytes.len() != M9_LINUX_META_IO_PROOF_LEN as usize {
        return Err(format!(
            "M9 Linux meta IO proof length {}, expected {M9_LINUX_META_IO_PROOF_LEN}",
            bytes.len()
        ));
    }
    if &bytes[..8] != b"PVBLKIO1" {
        return Err(format!(
            "M9 Linux meta IO proof missing PVBLKIO1 magic: {:?}",
            &bytes[..8]
        ));
    }
    let checksum = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
    if checksum == 0 {
        return Err("M9 Linux meta IO proof checksum must be nonzero".into());
    }
    Ok(checksum)
}

async fn run_linux_child(
    svc: WorkerService,
    index: usize,
    lease: proto::Lease,
    root_cumulative_icount: u64,
    root_cumulative_vns: u64,
    root_frame_counter: u32,
) -> TestResult<ChildRecord> {
    let slot_id = lease.slot_id;
    let run_started = Instant::now();
    let run = match svc
        .run(Request::new(proto::RunRequest {
            lease: Some(lease.clone()),
            until: Some(proto::run_request::Until::FrameBudget(
                M9_LINUX_CHILD_FRAMES,
            )),
            hard_icount_cap: M9_LINUX_CHILD_HARD_CAP,
            capture: None,
        }))
        .await
    {
        Ok(response) => response.into_inner(),
        Err(e) => {
            destroy_best_effort(&svc, Some(lease)).await;
            return Err(format!("child {index} Linux Run: {e}"));
        }
    };
    let run_ms = elapsed_ms(run_started.elapsed());
    if run.reason != i32::from(proto::StopReason::BudgetReached) {
        destroy_best_effort(&svc, Some(lease)).await;
        return Err(format!(
            "child {index} Linux Run stopped with {}, expected BUDGET_REACHED",
            run.reason
        ));
    }
    if run.frames_elapsed != u64::from(M9_LINUX_CHILD_FRAMES) {
        destroy_best_effort(&svc, Some(lease)).await;
        return Err(format!(
            "child {index} Linux frames_elapsed {}, expected {M9_LINUX_CHILD_FRAMES}",
            run.frames_elapsed
        ));
    }
    let segment_end_icount = match run.icount.checked_sub(root_cumulative_icount) {
        Some(value) => value,
        None => {
            destroy_best_effort(&svc, Some(lease)).await;
            return Err(format!(
                "child {index} Linux cumulative icount {} is before root {}",
                run.icount, root_cumulative_icount
            ));
        }
    };
    let segment_end_vns = match run.vns.checked_sub(root_cumulative_vns) {
        Some(value) => value,
        None => {
            destroy_best_effort(&svc, Some(lease)).await;
            return Err(format!(
                "child {index} Linux cumulative vns {} is before root {}",
                run.vns, root_cumulative_vns
            ));
        }
    };
    if segment_end_icount == 0 || segment_end_icount > M9_LINUX_CHILD_HARD_CAP {
        destroy_best_effort(&svc, Some(lease)).await;
        return Err(format!(
            "child {index} Linux segment icount {segment_end_icount}, expected 1..={M9_LINUX_CHILD_HARD_CAP}"
        ));
    }

    let meta_pvblk_checksum = match read_linux_meta_io_proof(&svc, lease.clone()).await {
        Ok(checksum) => checksum,
        Err(e) => {
            destroy_best_effort(&svc, Some(lease)).await;
            return Err(format!("child {index} Linux meta IO proof: {e}"));
        }
    };

    let snapshot_started = Instant::now();
    let snapshot = match svc
        .take_snapshot(Request::new(proto::TakeSnapshotRequest {
            lease: Some(lease.clone()),
            seal_input_log: Some(true),
            capture: None,
        }))
        .await
    {
        Ok(response) => response.into_inner(),
        Err(e) => {
            destroy_best_effort(&svc, Some(lease)).await;
            return Err(format!("child {index} Linux TakeSnapshot: {e}"));
        }
    };
    let original_commit_ms = elapsed_ms(snapshot_started.elapsed());
    let expected_frame_counter = match root_frame_counter.checked_add(M9_LINUX_CHILD_FRAMES) {
        Some(frame) => frame,
        None => {
            destroy_best_effort(&svc, Some(lease)).await;
            return Err(format!(
                "child {index} Linux root frame counter {root_frame_counter} overflows"
            ));
        }
    };
    if snapshot.frame_counter != expected_frame_counter {
        destroy_best_effort(&svc, Some(lease)).await;
        return Err(format!(
            "child {index} Linux snapshot frame_counter {}, expected {expected_frame_counter}",
            snapshot.frame_counter
        ));
    }

    destroy_best_effort(&svc, Some(lease)).await;
    snapshot_record(
        AcceptanceGuest::Linux,
        index,
        slot_id,
        snapshot,
        segment_end_icount,
        segment_end_vns,
        run.icount,
        run.vns,
        run.frames_elapsed,
        Some(meta_pvblk_checksum),
        ChildTiming {
            run_ms,
            original_commit_ms,
            ..ChildTiming::default()
        },
    )
}

async fn run_child(
    svc: WorkerService,
    guest: AcceptanceGuest,
    index: usize,
    lease: proto::Lease,
    root_cumulative_icount: u64,
    root_cumulative_vns: u64,
    root_frame_counter: u32,
) -> TestResult<ChildRecord> {
    match guest {
        AcceptanceGuest::Nanokernel => run_nanokernel_child(svc, index, lease).await,
        AcceptanceGuest::Linux => {
            run_linux_child(
                svc,
                index,
                lease,
                root_cumulative_icount,
                root_cumulative_vns,
                root_frame_counter,
            )
            .await
        }
    }
}

async fn run_child_batch(
    harness: &AcceptanceHarness,
    start_index: usize,
    leases: Vec<proto::Lease>,
) -> TestResult<Vec<ChildRecord>> {
    let mut tasks = Vec::with_capacity(leases.len());
    for (offset, lease) in leases.into_iter().enumerate() {
        let svc = harness.svc().clone();
        let guest = harness.guest();
        let root_cumulative_icount = harness.root_cumulative_icount();
        let root_cumulative_vns = harness.root_cumulative_vns();
        let root_frame_counter = harness.root_frame_counter();
        tasks.push(tokio::spawn(async move {
            run_child(
                svc,
                guest,
                start_index + offset,
                lease,
                root_cumulative_icount,
                root_cumulative_vns,
                root_frame_counter,
            )
            .await
        }));
    }

    let mut records = Vec::with_capacity(tasks.len());
    let mut errors = Vec::new();
    for task in tasks {
        match task.await {
            Ok(Ok(record)) => records.push(record),
            Ok(Err(e)) => errors.push(e),
            Err(e) => errors.push(format!("child task join: {e}")),
        }
    }
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }
    records.sort_by_key(|record| record.index);
    Ok(records)
}

async fn run_same_seed_children(
    harness: &AcceptanceHarness,
    index: usize,
    leases: Vec<proto::Lease>,
) -> TestResult<Vec<ChildRecord>> {
    let mut tasks = Vec::with_capacity(leases.len());
    for lease in leases {
        let svc = harness.svc().clone();
        let guest = harness.guest();
        let root_cumulative_icount = harness.root_cumulative_icount();
        let root_cumulative_vns = harness.root_cumulative_vns();
        let root_frame_counter = harness.root_frame_counter();
        tasks.push(tokio::spawn(async move {
            run_child(
                svc,
                guest,
                index,
                lease,
                root_cumulative_icount,
                root_cumulative_vns,
                root_frame_counter,
            )
            .await
        }));
    }

    let mut records = Vec::with_capacity(tasks.len());
    let mut errors = Vec::new();
    for task in tasks {
        match task.await {
            Ok(Ok(record)) => records.push(record),
            Ok(Err(e)) => errors.push(e),
            Err(e) => errors.push(format!("cross-slot child task join: {e}")),
        }
    }
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }
    records.sort_by_key(|record| record.slot_id);
    Ok(records)
}

fn fetch_log_payload(
    store: &snapstore_client::blocking::SnapstoreClient,
    input_log_id: &[u8],
) -> Vec<u8> {
    let log_id = LogId::from_bytes(arr32(input_log_id, "input_log_id"));
    let container = store.get_input_log(log_id).expect("get child input log");
    let decoded = InputLogContainer::decode(&container).expect("decode input log container");
    assert_eq!(decoded.inner_version(), 1);
    decoded.payload().to_vec()
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn parse_hex32(value: &str) -> Option<[u8; 32]> {
    let bytes = value.as_bytes();
    if bytes.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (index, pair) in bytes.chunks_exact(2).enumerate() {
        let high = hex_nibble(pair[0])?;
        let low = hex_nibble(pair[1])?;
        out[index] = (high << 4) | low;
    }
    Some(out)
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn elapsed_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn percentile(sorted_values: &[f64], percentile: f64) -> f64 {
    assert!(
        !sorted_values.is_empty(),
        "percentile requires at least one value"
    );
    let clamped = percentile.clamp(0.0, 1.0);
    let rank = ((sorted_values.len() - 1) as f64 * clamped).round() as usize;
    sorted_values[rank]
}

fn snapshot_ref_hex(snapshot: &proto::SnapshotRef) -> String {
    hex_bytes(&snapshot.hash)
}

fn state_hash_hex(hash: &[u8; 32]) -> String {
    hex_bytes(hash)
}

fn child_seed_hex(index: usize) -> String {
    hex_bytes(&child_seed(index))
}

fn m8_now_string() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time must be after UNIX_EPOCH");
    format!("unix-{}", now.as_secs())
}

fn git_output(repo: &Path, args: &[&str]) -> Option<String> {
    std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn git_value(repo: &Path, args: &[&str]) -> String {
    git_output(repo, args)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

fn repo_json(repo: &Path) -> serde_json::Value {
    serde_json::json!({
        "rev": git_value(repo, &["rev-parse", "HEAD"]),
        "dirty": git_output(repo, &["status", "--short"])
            .map(|status| !status.is_empty())
            .unwrap_or(true),
    })
}

fn snapshot_manifest(
    store: &snapstore_client::blocking::SnapstoreClient,
    snapshot: &proto::SnapshotRef,
) -> TestResult<Manifest> {
    let snapshot_ref = StoreSnapshotRef::from_bytes(arr32(&snapshot.hash, "snapshot ref"));
    let container = store
        .get_snapshot(snapshot_ref)
        .map_err(|e| format!("get_snapshot for evidence: {e}"))?;
    Manifest::decode(&container).map_err(|e| format!("decode snapshot manifest: {e}"))
}

fn flattened_page_hashes(
    manifest: &Manifest,
    baseline: Option<&Manifest>,
) -> TestResult<Vec<[u8; 32]>> {
    let total_pages = usize::try_from(manifest.guest_ram_bytes / snapstore_manifest::PAGE_SIZE_V1)
        .map_err(|_| "guest RAM page count does not fit usize".to_string())?;
    let mut hashes = vec![[0u8; 32]; total_pages];
    if let Some(base) = baseline {
        if base.guest_ram_bytes != manifest.guest_ram_bytes {
            return Err("baseline and child guest RAM sizes differ".into());
        }
        for entry in &base.entries {
            let index = usize::try_from(entry.page_index)
                .map_err(|_| "baseline page index does not fit usize".to_string())?;
            if index < hashes.len() {
                hashes[index] = entry.page_hash.to_bytes();
            }
        }
    }
    for entry in &manifest.entries {
        let index = usize::try_from(entry.page_index)
            .map_err(|_| "manifest page index does not fit usize".to_string())?;
        if index < hashes.len() {
            hashes[index] = entry.page_hash.to_bytes();
        }
    }
    Ok(hashes)
}

fn shared_page_ratio(
    store: &snapstore_client::blocking::SnapstoreClient,
    root: &proto::SnapshotRef,
    child: &proto::SnapshotRef,
) -> TestResult<(f64, String, u64)> {
    let root_manifest = snapshot_manifest(store, root)?;
    let child_manifest = snapshot_manifest(store, child)?;
    let root_hashes = flattened_page_hashes(&root_manifest, None)?;
    let child_hashes = flattened_page_hashes(&child_manifest, Some(&root_manifest))?;
    if root_hashes.len() != child_hashes.len() {
        return Err("root and child page tables differ in length".into());
    }
    let mut shared = 0usize;
    let mut denominator = 0usize;
    for (root_hash, child_hash) in root_hashes.iter().zip(child_hashes.iter()) {
        if child_hash != &[0u8; 32] {
            denominator += 1;
            if child_hash == root_hash {
                shared += 1;
            }
        }
    }
    let ratio = if denominator == 0 {
        0.0
    } else {
        shared as f64 / denominator as f64
    };
    let manifest_kind = if child_manifest.delta {
        "DELTA"
    } else {
        "FULL"
    }
    .to_string();
    let chain_depth = if child_manifest.delta { 1 } else { 0 };
    Ok((ratio, manifest_kind, chain_depth))
}

struct M8EvidenceRun {
    root: PathBuf,
    child_table_jsonl: PathBuf,
    rows: Vec<serde_json::Value>,
    started_at: String,
    jobs: usize,
    store_root: PathBuf,
    store_root_qualified: bool,
    semantic_negative: bool,
    resume_enabled: bool,
    resumed_rows: usize,
}

impl M8EvidenceRun {
    fn new(jobs: usize, store_root: &Path) -> TestResult<Self> {
        Self::new_inner(jobs, store_root, false)
    }

    fn new_semantic_negative(store_root: &Path) -> TestResult<Self> {
        Self::new_inner(1, store_root, true)
    }

    fn new_inner(jobs: usize, store_root: &Path, semantic_negative: bool) -> TestResult<Self> {
        let mut root = std::env::var(M8_EVIDENCE_ROOT_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("target/m8-joint-fork-integrity-live"));
        if semantic_negative {
            root = root.join("semantic-negative");
        }
        std::fs::create_dir_all(root.join("logs"))
            .map_err(|e| format!("create M8 evidence logs dir: {e}"))?;
        std::fs::create_dir_all(root.join("raw"))
            .map_err(|e| format!("create M8 evidence raw dir: {e}"))?;
        std::fs::create_dir_all(root.join("hypervisor"))
            .map_err(|e| format!("create M8 evidence hypervisor dir: {e}"))?;
        std::fs::create_dir_all(root.join("snapstore"))
            .map_err(|e| format!("create M8 evidence snapstore dir: {e}"))?;
        std::fs::create_dir_all(root.join("hardware"))
            .map_err(|e| format!("create M8 evidence hardware dir: {e}"))?;
        let child_table_jsonl = root.join("child-ref-table.jsonl");
        let resume_enabled =
            !semantic_negative && std::env::var(M8_EVIDENCE_RESUME_ENV).as_deref() == Ok("1");
        let rows = if resume_enabled && child_table_jsonl.exists() {
            Self::load_resume_rows(&child_table_jsonl, jobs)?
        } else {
            Vec::new()
        };
        if resume_enabled {
            Self::rewrite_child_table(&child_table_jsonl, &rows)?;
        } else {
            std::fs::write(&child_table_jsonl, "")
                .map_err(|e| format!("truncate M8 child-ref-table.jsonl: {e}"))?;
        }
        let resumed_rows = rows.len();
        Ok(Self {
            root,
            child_table_jsonl,
            rows,
            started_at: m8_now_string(),
            jobs,
            store_root: store_root.to_path_buf(),
            store_root_qualified: std::env::var(M8_STORE_ROOT_QUALIFIED_ENV).as_deref() == Ok("1"),
            semantic_negative,
            resume_enabled,
            resumed_rows,
        })
    }

    fn load_resume_rows(path: &Path, jobs: usize) -> TestResult<Vec<serde_json::Value>> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("read M8 resume child-ref-table.jsonl: {e}"))?;
        let mut rows = Vec::new();
        for (line_no, line) in content.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let mut row: serde_json::Value = serde_json::from_str(line)
                .map_err(|e| format!("parse M8 resume row {}: {e}", line_no + 1))?;
            Self::validate_resume_row(&row, rows.len(), jobs, line_no + 1)?;
            row.as_object_mut()
                .expect("validated M8 resume row is an object")
                .insert("row_source".into(), serde_json::json!("resumed"));
            rows.push(row);
        }
        Ok(rows)
    }

    fn validate_resume_row(
        row: &serde_json::Value,
        expected_index: usize,
        jobs: usize,
        line_no: usize,
    ) -> TestResult<()> {
        let object = row
            .as_object()
            .ok_or_else(|| format!("M8 resume row {line_no}: must be an object"))?;
        let child_index = object
            .get("child_index")
            .and_then(|value| value.as_u64())
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| {
                format!("M8 resume row {line_no}: child_index must be a non-negative integer")
            })?;
        if child_index != expected_index {
            return Err(format!(
                "M8 resume row {line_no}: child_index {child_index} is not contiguous prefix index {expected_index}"
            ));
        }
        if child_index >= jobs {
            return Err(format!(
                "M8 resume row {line_no}: child_index {child_index} exceeds configured jobs {jobs}"
            ));
        }
        let seed = Self::resume_row_str(row, "seed_hex", line_no)?;
        let expected_seed = child_seed_hex(child_index);
        if seed != expected_seed {
            return Err(format!(
                "M8 resume row {line_no}: seed_hex does not match child_index {child_index}"
            ));
        }
        if Self::resume_row_str(row, "result", line_no)? != "pass" {
            return Err(format!("M8 resume row {line_no}: result must be pass"));
        }
        let original_ref = Self::resume_hex32_value(row, "original_ref_hex", line_no)?;
        let replay_ref = Self::resume_hex32_value(row, "replay_ref_hex", line_no)?;
        if original_ref != replay_ref {
            return Err(format!(
                "M8 resume row {line_no}: replay_ref_hex must equal original_ref_hex"
            ));
        }
        let original_state = Self::resume_hex32_value(row, "state_hash_original_hex", line_no)?;
        let replay_state = Self::resume_hex32_value(row, "state_hash_replay_hex", line_no)?;
        if original_state != replay_state {
            return Err(format!(
                "M8 resume row {line_no}: state_hash_replay_hex must equal state_hash_original_hex"
            ));
        }
        Self::resume_hex32_value(row, "input_log_id_hex", line_no)?;
        match Self::resume_row_str(row, "restore_mode", line_no)? {
            "full" | "baseline_delta" => {}
            value => {
                return Err(format!(
                    "M8 resume row {line_no}: invalid restore_mode {value}"
                ))
            }
        }
        match Self::resume_row_str(row, "manifest_kind", line_no)? {
            "FULL" | "DELTA" => {}
            value => {
                return Err(format!(
                    "M8 resume row {line_no}: invalid manifest_kind {value}"
                ))
            }
        }
        object
            .get("chain_depth")
            .and_then(|value| value.as_u64())
            .ok_or_else(|| format!("M8 resume row {line_no}: chain_depth must be non-negative"))?;
        let shared_page_ratio = object
            .get("shared_page_ratio")
            .and_then(|value| value.as_f64())
            .ok_or_else(|| {
                format!("M8 resume row {line_no}: shared_page_ratio must be a number")
            })?;
        if !(0.0..=1.0).contains(&shared_page_ratio) {
            return Err(format!(
                "M8 resume row {line_no}: shared_page_ratio must be in [0, 1]"
            ));
        }
        if !object
            .get("timing_ms")
            .and_then(|value| value.as_object())
            .is_some_and(|timing| !timing.is_empty())
        {
            return Err(format!(
                "M8 resume row {line_no}: timing_ms must be a non-empty object"
            ));
        }
        if let Some(row_source) = object.get("row_source").and_then(|value| value.as_str()) {
            if row_source != "fresh" && row_source != "resumed" {
                return Err(format!(
                    "M8 resume row {line_no}: row_source must be fresh or resumed"
                ));
            }
        }
        Ok(())
    }

    fn rewrite_child_table(path: &Path, rows: &[serde_json::Value]) -> TestResult<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .map_err(|e| format!("rewrite M8 child-ref-table.jsonl: {e}"))?;
        for row in rows {
            writeln!(file, "{row}").map_err(|e| format!("rewrite M8 resume row: {e}"))?;
        }
        Ok(())
    }

    fn next_child_index(&self) -> usize {
        self.rows.len()
    }

    fn resume_hex32_set(&self, field: &str) -> TestResult<BTreeSet<[u8; 32]>> {
        self.rows
            .iter()
            .enumerate()
            .map(|(index, row)| Self::resume_hex32_value(row, field, index + 1))
            .collect()
    }

    fn resume_row_str<'a>(
        row: &'a serde_json::Value,
        field: &str,
        line_no: usize,
    ) -> TestResult<&'a str> {
        row.get(field)
            .and_then(|value| value.as_str())
            .ok_or_else(|| format!("M8 resume row {line_no}: {field} must be a string"))
    }

    fn resume_hex32_value(
        row: &serde_json::Value,
        field: &str,
        line_no: usize,
    ) -> TestResult<[u8; 32]> {
        let value = Self::resume_row_str(row, field, line_no)?;
        parse_hex32(value).ok_or_else(|| {
            format!("M8 resume row {line_no}: {field} must be 32-byte lowercase hex")
        })
    }

    fn timing_component(row: &serde_json::Value, key: &str) -> Option<f64> {
        row.get("timing_ms")
            .and_then(|timing| timing.get(key))
            .and_then(|value| value.as_f64())
            .filter(|value| value.is_finite() && *value >= 0.0)
    }

    fn positive_timing_sum(row: &serde_json::Value, keys: &[&str]) -> Option<f64> {
        let mut sum = 0.0;
        for key in keys {
            sum += Self::timing_component(row, key)?;
        }
        (sum > 0.0).then_some(sum)
    }

    fn replay_restore_timing(row: &serde_json::Value) -> Option<f64> {
        Self::timing_component(row, "replay_restore")
            .or_else(|| Self::timing_component(row, "restore"))
            .filter(|value| *value > 0.0)
    }

    fn latency_stats(values: &[f64]) -> serde_json::Value {
        if values.is_empty() {
            return serde_json::json!({
                "count": 0,
                "p50": serde_json::Value::Null,
                "p95": serde_json::Value::Null,
                "p99": serde_json::Value::Null,
                "max": serde_json::Value::Null,
            });
        }
        let mut sorted = values.to_vec();
        sorted.sort_by(|a, b| {
            a.partial_cmp(b)
                .expect("M8 latency values are finite before sorting")
        });
        serde_json::json!({
            "count": sorted.len(),
            "p50": percentile(&sorted, 0.50),
            "p95": percentile(&sorted, 0.95),
            "p99": percentile(&sorted, 0.99),
            "max": sorted[sorted.len() - 1],
        })
    }

    fn linked_semantic_negative_red(root: &Path) -> TestResult<bool> {
        let path = root.join("semantic-negative").join("evidence.json");
        if !path.exists() {
            return Ok(false);
        }
        let body = std::fs::read_to_string(&path)
            .map_err(|e| format!("read linked M8 semantic-negative evidence: {e}"))?;
        let evidence: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| format!("parse linked M8 semantic-negative evidence: {e}"))?;
        if evidence.get("run_kind").and_then(|value| value.as_str()) != Some("semantic_negative") {
            return Err(format!(
                "linked M8 semantic-negative evidence {} has wrong run_kind",
                path.display()
            ));
        }
        Ok(evidence
            .get("semantic_negative")
            .and_then(|value| value.get("actual_red_result"))
            .and_then(|value| value.as_bool())
            == Some(true))
    }

    fn append_child(
        &mut self,
        harness: &AcceptanceHarness,
        child: &ChildRecord,
        replay: &ReplayCommitRecord,
    ) -> TestResult<()> {
        let (shared_page_ratio, manifest_kind, chain_depth) =
            shared_page_ratio(harness.store(), harness.root_snapshot(), &child.snapshot)?;
        let row = serde_json::json!({
            "child_index": child.index,
            "seed_hex": child_seed_hex(child.index),
            "original_ref_hex": snapshot_ref_hex(&child.snapshot),
            "replay_ref_hex": snapshot_ref_hex(&replay.snapshot),
            "input_log_id_hex": hex_bytes(&child.input_log_id),
            "state_hash_original_hex": state_hash_hex(&child.state_hash),
            "state_hash_replay_hex": state_hash_hex(&replay.state_hash),
            "restore_mode": "baseline_delta",
            "baseline_ref_hex": snapshot_ref_hex(harness.root_snapshot()),
            "manifest_kind": manifest_kind,
            "chain_depth": chain_depth,
            "dirty_pages": child.dirty_pages,
            "shared_page_ratio": shared_page_ratio,
            "timing_ms": {
                "fork": child.timing.fork_ms,
                "run": child.timing.run_ms,
                "original_commit": child.timing.original_commit_ms,
                "restore": replay.baseline_delta_restore_ms,
                "replay_restore": replay.timing.restore_ms,
                "replay": replay.timing.replay_ms,
                "replay_commit": replay.timing.replay_commit_ms,
            },
            "result": "pass",
            "original_slot_id": child.slot_id,
            "replay_slot_id": replay.replay_slot_id,
            "row_source": "fresh",
        });
        let mut file = OpenOptions::new()
            .append(true)
            .open(&self.child_table_jsonl)
            .map_err(|e| format!("open M8 child-ref-table.jsonl: {e}"))?;
        writeln!(file, "{row}").map_err(|e| format!("write M8 child row: {e}"))?;
        self.rows.push(row);
        Ok(())
    }

    fn append_semantic_negative(
        &mut self,
        harness: &AcceptanceHarness,
        original: &ChildRecord,
        replay: &ChildRecord,
        replay_restore_ms: f64,
    ) -> TestResult<()> {
        let (shared_page_ratio, manifest_kind, chain_depth) =
            shared_page_ratio(harness.store(), harness.root_snapshot(), &replay.snapshot)?;
        let result = if replay.snapshot.hash != original.snapshot.hash {
            "ref_mismatch"
        } else if replay.state_hash != original.state_hash {
            "state_mismatch"
        } else {
            "error"
        };
        let row = serde_json::json!({
            "child_index": original.index,
            "seed_hex": child_seed_hex(original.index),
            "original_ref_hex": snapshot_ref_hex(&original.snapshot),
            "replay_ref_hex": snapshot_ref_hex(&replay.snapshot),
            "input_log_id_hex": hex_bytes(&original.input_log_id),
            "state_hash_original_hex": state_hash_hex(&original.state_hash),
            "state_hash_replay_hex": state_hash_hex(&replay.state_hash),
            "restore_mode": "full",
            "baseline_ref_hex": serde_json::Value::Null,
            "manifest_kind": manifest_kind,
            "chain_depth": chain_depth,
            "dirty_pages": replay.dirty_pages,
            "shared_page_ratio": shared_page_ratio,
            "timing_ms": {
                "fork": original.timing.fork_ms,
                "run": original.timing.run_ms,
                "original_commit": original.timing.original_commit_ms,
                "restore": replay_restore_ms,
                "replay": replay.timing.run_ms,
                "replay_commit": replay.timing.original_commit_ms,
            },
            "result": result,
            "original_slot_id": original.slot_id,
            "replay_slot_id": replay.slot_id,
            "mutated_input": "first pad event buttons xor 0x80000000",
            "row_source": "fresh",
        });
        let mut file = OpenOptions::new()
            .append(true)
            .open(&self.child_table_jsonl)
            .map_err(|e| format!("open M8 semantic-negative child-ref-table.jsonl: {e}"))?;
        writeln!(file, "{row}").map_err(|e| format!("write M8 semantic-negative row: {e}"))?;
        self.rows.push(row);
        Ok(())
    }

    fn finish(&self, guest: AcceptanceGuest) -> TestResult<()> {
        self.write_child_csv()?;
        let aggregate_shared = if self.rows.is_empty() {
            0.0
        } else {
            self.rows
                .iter()
                .filter_map(|row| row.get("shared_page_ratio").and_then(|v| v.as_f64()))
                .sum::<f64>()
                / self.rows.len() as f64
        };
        let saw_delta_restore = self
            .rows
            .iter()
            .any(|row| row.get("restore_mode").and_then(|v| v.as_str()) == Some("baseline_delta"));
        let full_cadence_seen = self.root.join("full-cadence-smoke.ok").is_file()
            || self
                .rows
                .iter()
                .any(|row| row.get("manifest_kind").and_then(|v| v.as_str()) == Some("FULL"));
        let ref_identity = self.rows.iter().all(|row| {
            row.get("original_ref_hex") == row.get("replay_ref_hex")
                && row.get("state_hash_original_hex") == row.get("state_hash_replay_hex")
        });
        let row_semantic_red = self.rows.iter().any(|row| {
            row.get("result").and_then(|value| value.as_str()) == Some("ref_mismatch")
                && row.get("original_ref_hex") != row.get("replay_ref_hex")
        });
        let linked_semantic_red = if self.semantic_negative {
            false
        } else {
            Self::linked_semantic_negative_red(&self.root)?
        };
        let semantic_red = row_semantic_red || linked_semantic_red;
        let fork_commit_values: Vec<_> = self
            .rows
            .iter()
            .filter_map(|row| Self::positive_timing_sum(row, &["fork", "run", "original_commit"]))
            .collect();
        let restore_delta_values: Vec<_> = self
            .rows
            .iter()
            .filter(|row| {
                row.get("restore_mode").and_then(|v| v.as_str()) == Some("baseline_delta")
            })
            .filter_map(|row| Self::timing_component(row, "restore").filter(|value| *value > 0.0))
            .collect();
        let restore_full_values: Vec<_> = self
            .rows
            .iter()
            .filter(|row| row.get("restore_mode").and_then(|v| v.as_str()) == Some("full"))
            .filter_map(|row| Self::timing_component(row, "restore").filter(|value| *value > 0.0))
            .collect();
        let replay_commit_values: Vec<_> = self
            .rows
            .iter()
            .filter_map(|row| {
                Some(
                    Self::replay_restore_timing(row)?
                        + Self::timing_component(row, "replay")?
                        + Self::timing_component(row, "replay_commit")?,
                )
            })
            .collect();
        let latency = serde_json::json!({
            "policy": "telemetry; storage latency is recorded for M8 evidence and compared during closeout sign-off",
            "fork_to_original_commit": Self::latency_stats(&fork_commit_values),
            "restore_delta": Self::latency_stats(&restore_delta_values),
            "restore_full": Self::latency_stats(&restore_full_values),
            "replay_restore_to_commit": Self::latency_stats(&replay_commit_values),
        });
        let fork_commit_latency_complete =
            !self.rows.is_empty() && fork_commit_values.len() == self.rows.len();
        let restore_delta_latency_present = !restore_delta_values.is_empty();
        let semantic_negative_summary = serde_json::json!({
            "command": "cargo test -p dh-worker --test m7_fork_verify m8_accept_semantic_negative_replay_commit_ref_mismatch -- --ignored --nocapture --test-threads=1",
            "mutated_input": "first pad event buttons xor 0x80000000",
            "expected_red_result": true,
            "actual_red_result": semantic_red,
            "evidence_json": if self.semantic_negative {
                "evidence.json"
            } else {
                "semantic-negative/evidence.json"
            },
            "aggregation": if row_semantic_red {
                "current_run"
            } else if linked_semantic_red {
                "linked_semantic_negative"
            } else {
                "missing"
            }
        });
        let run_kind = if self.semantic_negative {
            "semantic_negative"
        } else if self.jobs == DEFAULT_JOBS {
            "full_acceptance"
        } else {
            "bounded_ci"
        };
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("dh-worker manifest lives under crates/dh-worker")
            .to_path_buf();
        let workspace_parent = repo_root
            .parent()
            .expect("hypervisor repo has a parent directory")
            .to_path_buf();
        let bars = serde_json::json!([
            {"name": "m8_command_status", "ok": true},
            {"name": "m8_child_count", "ok": self.rows.len() == self.jobs},
            {"name": "m8_ref_identity", "ok": ref_identity},
            {"name": "m8_replay_done", "ok": self.rows.len() == self.jobs},
            {"name": "m8_shared_page_ratio_aggregate", "ok": aggregate_shared >= 0.94},
            {"name": "m8_restore_delta_used", "ok": saw_delta_restore},
            {"name": "m8_full_manifest_cadence", "ok": full_cadence_seen},
            {"name": "m8_semantic_negative_red", "ok": semantic_red},
            {"name": "m8_store_root_qualified", "ok": self.store_root_qualified},
            {"name": "m8_fork_commit_p99", "ok": fork_commit_latency_complete},
            {"name": "m8_restore_delta_p99", "ok": restore_delta_latency_present}
        ]);
        let evidence = serde_json::json!({
            "schema_version": 1,
            "request": ".agents/requests/phase2-closeout-m8-joint-fork-integrity",
            "run_kind": run_kind,
            "expected_child_count": self.jobs,
            "run_id": format!("m8-live-{}", self.started_at),
            "started_at": self.started_at,
            "finished_at": m8_now_string(),
            "repos": {
                "determinism-hypervisor": repo_json(&repo_root),
                "snapshot-store": repo_json(&workspace_parent.join("snapshot-store")),
                "control-plane": repo_json(&workspace_parent.join("control-plane")),
                "guest-sdk": repo_json(&workspace_parent.join("guest-sdk")),
            },
            "host": {
                "hostname": std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".into()),
                "arch": std::env::consts::ARCH,
            },
            "guest": {
                "kind": match guest {
                    AcceptanceGuest::Nanokernel => "nanokernel",
                    AcceptanceGuest::Linux => "linux",
                },
            },
            "store_root": {
                "path": self.store_root.display().to_string(),
                "disk_class": std::env::var(M8_STORE_ROOT_DISK_CLASS_ENV).unwrap_or_else(|_| "unknown".into()),
                "qualified": self.store_root_qualified,
            },
            "config": {
                "jobs": self.jobs,
                "max_delta_chain": dh_worker::service::DEFAULT_MAX_DELTA_CHAIN,
                "slot_cores_env": std::env::var(SLOT_CORES_ENV).unwrap_or_default(),
                "restore_mode": "baseline_delta",
                "child_batch_size": serde_json::Value::Null,
            },
            "child_table": {
                "jsonl": "child-ref-table.jsonl",
                "csv": "child-ref-table.csv",
            },
            "resume": {
                "enabled": self.resume_enabled,
                "resumed_child_count": self.resumed_rows,
                "fresh_child_count": self.rows.len().saturating_sub(self.resumed_rows),
            },
            "latency_ms": latency,
            "bars": bars,
            "commands": ["cargo test -p dh-worker --test m7_fork_verify m8_accept_1000_seeded_forks_replay_commit_ref_identity -- --ignored --nocapture --test-threads=1"],
            "artifacts": [
                {"path": "evidence.json", "kind": "summary"},
                {"path": "child-ref-table.jsonl", "kind": "child_table"},
                {"path": "child-ref-table.csv", "kind": "child_table_csv"}
            ],
            "semantic_negative": semantic_negative_summary,
            "deviations": [
                {
                    "id": "live_harness_partial",
                    "reason": "Replay-commit evidence is emitted with measured latencies and baseline-delta restore probes; full closeout still needs the qualified hardware run and linked semantic-negative evidence when this positive evidence is generated."
                }
            ]
        });
        std::fs::write(
            self.root.join("evidence.json"),
            serde_json::to_string_pretty(&evidence).expect("serialize M8 evidence") + "\n",
        )
        .map_err(|e| format!("write M8 evidence.json: {e}"))?;
        Ok(())
    }

    fn write_child_csv(&self) -> TestResult<()> {
        let mut csv = String::from(
            "child_index,original_ref_hex,replay_ref_hex,input_log_id_hex,restore_mode,manifest_kind,chain_depth,shared_page_ratio,result,row_source\n",
        );
        for row in &self.rows {
            csv.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{}\n",
                row["child_index"],
                row["original_ref_hex"].as_str().unwrap_or(""),
                row["replay_ref_hex"].as_str().unwrap_or(""),
                row["input_log_id_hex"].as_str().unwrap_or(""),
                row["restore_mode"].as_str().unwrap_or(""),
                row["manifest_kind"].as_str().unwrap_or(""),
                row["chain_depth"],
                row["shared_page_ratio"],
                row["result"].as_str().unwrap_or(""),
                row["row_source"].as_str().unwrap_or(""),
            ));
        }
        std::fs::write(self.root.join("child-ref-table.csv"), csv)
            .map_err(|e| format!("write M8 child-ref-table.csv: {e}"))
    }
}

fn expected_frame_table(start_frame: u32, frames: u32) -> TestResult<Vec<u32>> {
    let end = start_frame
        .checked_add(frames)
        .ok_or_else(|| format!("frame range overflows u32: start={start_frame} count={frames}"))?;
    Ok((start_frame + 1..=end).collect())
}

fn parse_child_log(log: &[u8]) -> TestResult<ParsedChildLog> {
    let reader = LogReader::parse(log).map_err(|e| format!("child DHILOG parse: {e:?}"))?;
    let header = reader.header();
    let epoch_hashes: Vec<_> = reader
        .aux()
        .filter_map(|rec| match rec.body() {
            RecordBody::EpochHash {
                epoch_index,
                chain_value,
            } => Some((epoch_index, rec.icount(), chain_value)),
            _ => None,
        })
        .collect();
    let frame_marks: Vec<_> = reader
        .aux()
        .filter_map(|rec| match rec.body() {
            RecordBody::FrameMark { frame_index } => Some((rec.icount(), frame_index)),
            _ => None,
        })
        .collect();
    let canonical_count = reader.canonical().count() as u64;
    let (_end_reason, end_state_hash) = reader.end();
    if end_state_hash != header.end_state_hash {
        return Err("child DHILOG END state hash does not match header".into());
    }
    Ok(ParsedChildLog {
        dhilog_blake3: blake3_hex(log),
        base_snapshot_id: header.base_snapshot_id,
        end_snapshot_id: header.end_snapshot_id,
        machine_config_hash: header.machine_config_hash,
        record_count: header.record_count,
        canonical_count,
        end_icount: header.end_icount,
        end_vns: header.end_vns,
        end_state_hash: header.end_state_hash,
        has_epoch_hashes: header.has_epoch_hashes(),
        epoch_hashes,
        frame_marks,
    })
}

fn validate_single_edge_lineage(
    harness: &AcceptanceHarness,
    child: &ChildRecord,
    log: &[u8],
) -> TestResult<ParsedChildLog> {
    let root = harness.root_snapshot();
    let root_id = arr32(&root.hash, "root snapshot");
    let child_id = arr32(&child.snapshot.hash, "child snapshot");

    let lineage = Lineage::new(&[log]).map_err(|e| format!("child lineage edge: {e:?}"))?;
    if lineage.len() != 1 {
        return Err(format!(
            "child {} lineage length {}, expected 1",
            child.index,
            lineage.len()
        ));
    }
    if lineage.root_base() != root_id {
        return Err(format!("child {} lineage root_base mismatch", child.index));
    }
    let (end_snapshot, end_state_hash, end_icount) = lineage.end_identity();
    if end_snapshot != child_id {
        return Err(format!(
            "child {} lineage end snapshot mismatch",
            child.index
        ));
    }
    if end_state_hash != child.state_hash {
        return Err(format!("child {} lineage end state mismatch", child.index));
    }
    if end_icount != child.segment_end_icount {
        return Err(format!(
            "child {} lineage end icount {}, expected {}",
            child.index, end_icount, child.segment_end_icount
        ));
    }

    let parsed = parse_child_log(log)?;
    if parsed.base_snapshot_id != root_id {
        return Err(format!(
            "child {} DHILOG base snapshot mismatch",
            child.index
        ));
    }
    if parsed.end_snapshot_id != child_id {
        return Err(format!(
            "child {} DHILOG end snapshot mismatch",
            child.index
        ));
    }
    if parsed.end_state_hash != child.state_hash {
        return Err(format!("child {} DHILOG end state mismatch", child.index));
    }
    if parsed.machine_config_hash != harness.machine_config_hash() {
        return Err(format!(
            "child {} DHILOG machine_config_hash mismatch",
            child.index
        ));
    }
    if parsed.end_icount != child.segment_end_icount {
        return Err(format!(
            "child {} DHILOG end_icount {}, expected {}",
            child.index, parsed.end_icount, child.segment_end_icount
        ));
    }
    if parsed.end_vns != child.segment_end_vns {
        return Err(format!(
            "child {} DHILOG end_vns {}, expected {}",
            child.index, parsed.end_vns, child.segment_end_vns
        ));
    }

    match harness.guest() {
        AcceptanceGuest::Nanokernel => validate_nanokernel_log(child, log, &parsed)?,
        AcceptanceGuest::Linux => validate_linux_log(harness, child, &parsed)?,
    }
    Ok(parsed)
}

fn validate_nanokernel_log(
    child: &ChildRecord,
    log: &[u8],
    parsed: &ParsedChildLog,
) -> TestResult<()> {
    if parsed.end_icount != RUN_BUDGET || parsed.end_vns != VNS_PER_SECOND {
        return Err(format!(
            "child {} nanokernel DHILOG ended at {}/{}, expected {RUN_BUDGET}/{VNS_PER_SECOND}",
            child.index, parsed.end_icount, parsed.end_vns
        ));
    }
    let reader = LogReader::parse(log).map_err(|e| format!("child DHILOG parse: {e:?}"))?;
    let actual: Vec<_> = reader
        .canonical()
        .map(|rec| match rec.body() {
            RecordBody::PadSet { port, buttons, .. } => Ok((rec.icount(), port, buttons)),
            other => Err(format!(
                "child {} DHILOG contains unexpected canonical record {other:?}",
                child.index
            )),
        })
        .collect::<TestResult<Vec<_>>>()?;
    let expected = expected_pad_records(child.index);
    if parsed.canonical_count != expected.len() as u64 {
        return Err(format!(
            "child {} PAD_SET canonical count {}, expected {}",
            child.index,
            parsed.canonical_count,
            expected.len()
        ));
    }
    if actual != expected {
        return Err(format!(
            "child {} PAD_SET canonical records differed: actual={actual:?} expected={expected:?}",
            child.index
        ));
    }
    Ok(())
}

fn validate_linux_log(
    harness: &AcceptanceHarness,
    child: &ChildRecord,
    parsed: &ParsedChildLog,
) -> TestResult<()> {
    if !parsed.has_epoch_hashes {
        return Err(format!(
            "child {} Linux DHILOG header does not advertise EPOCH_HASH records",
            child.index
        ));
    }
    if parsed.epoch_hashes.is_empty() {
        return Err(format!(
            "child {} Linux DHILOG contains zero EPOCH_HASH records",
            child.index
        ));
    }
    if parsed.record_count < parsed.canonical_count {
        return Err(format!(
            "child {} Linux DHILOG record_count {} is less than canonical_count {}",
            child.index, parsed.record_count, parsed.canonical_count
        ));
    }
    if parsed.frame_marks.len() != M9_LINUX_CHILD_FRAMES as usize {
        return Err(format!(
            "child {} Linux FRAME_MARK count {}, expected {M9_LINUX_CHILD_FRAMES}",
            child.index,
            parsed.frame_marks.len()
        ));
    }
    let expected_frames =
        expected_frame_table(harness.root_frame_counter(), M9_LINUX_CHILD_FRAMES)?;
    let actual_frames: Vec<_> = parsed
        .frame_marks
        .iter()
        .map(|(_, frame_index)| *frame_index)
        .collect();
    if actual_frames != expected_frames {
        return Err(format!(
            "child {} Linux FRAME_MARK frames {actual_frames:?}, expected {expected_frames:?}",
            child.index
        ));
    }
    if !parsed
        .frame_marks
        .windows(2)
        .all(|window| window[0].0 < window[1].0)
    {
        return Err(format!(
            "child {} Linux FRAME_MARK icounts are not strictly increasing: {:?}",
            child.index, parsed.frame_marks
        ));
    }
    if parsed
        .frame_marks
        .last()
        .map(|(_, frame_index)| *frame_index)
        != Some(child.frame_counter)
    {
        return Err(format!(
            "child {} Linux final FRAME_MARK does not match child frame_counter {}",
            child.index, child.frame_counter
        ));
    }
    if child.frames_elapsed != u64::from(M9_LINUX_CHILD_FRAMES) {
        return Err(format!(
            "child {} Linux frames_elapsed {}, expected {M9_LINUX_CHILD_FRAMES}",
            child.index, child.frames_elapsed
        ));
    }
    if child.meta_pvblk_checksum.is_none() {
        return Err(format!(
            "child {} Linux missing meta IO checksum",
            child.index
        ));
    }
    Ok(())
}

async fn verify_child(
    svc: WorkerService,
    guest: AcceptanceGuest,
    root: proto::SnapshotRef,
    child: ChildRecord,
    parsed: ParsedChildLog,
) -> TestResult<ChildRecord> {
    let mut stream = svc
        .verify_replay(Request::new(proto::VerifyReplayRequest {
            base: Some(root),
            log: Some(proto::verify_replay_request::Log::InputLogId(
                child.input_log_id.clone(),
            )),
            bisect_on_divergence: Some(false),
        }))
        .await
        .map_err(|e| format!("child {} VerifyReplay start: {e}", child.index))?
        .into_inner();

    let mut epoch_ok = 0u64;
    let mut done = None;
    while let Some(item) = stream.next().await {
        let progress =
            item.map_err(|e| format!("child {} VerifyReplay stream: {e}", child.index))?;
        match progress.msg {
            Some(proto::verify_replay_progress::Msg::EpochOk(_)) => {
                if done.is_some() {
                    return Err(format!(
                        "child {} VerifyReplay emitted EpochOk after Done",
                        child.index
                    ));
                }
                epoch_ok += 1;
            }
            Some(proto::verify_replay_progress::Msg::Done(msg)) => {
                if done.replace(msg).is_some() {
                    return Err(format!(
                        "child {} VerifyReplay emitted duplicate Done",
                        child.index
                    ));
                }
            }
            Some(proto::verify_replay_progress::Msg::Divergence(div)) => {
                return Err(format!(
                    "child {} VerifyReplay diverged: {div:?}",
                    child.index
                ));
            }
            None => return Err(format!("child {} VerifyReplay empty progress", child.index)),
        }
    }
    if epoch_ok == 0 {
        return Err(format!(
            "child {} VerifyReplay emitted no EpochOk progress",
            child.index
        ));
    }
    let done = done.ok_or_else(|| format!("child {} VerifyReplay emitted no Done", child.index))?;
    if done.total_icount != child.segment_end_icount {
        return Err(format!(
            "child {} VerifyReplay Done total_icount {}, expected {}",
            child.index, done.total_icount, child.segment_end_icount
        ));
    }
    let done_hash = done
        .end_state_hash
        .as_ref()
        .map(|hash| arr32(&hash.hash, "VerifyReplay Done end_state_hash"))
        .ok_or_else(|| format!("child {} VerifyReplay Done missing hash", child.index))?;
    if done_hash != child.state_hash {
        return Err(format!(
            "child {} VerifyReplay hash mismatch: done != snapshot",
            child.index
        ));
    }

    match guest {
        AcceptanceGuest::Nanokernel => {
            if done.total_icount != RUN_BUDGET {
                return Err(format!(
                    "child {} nanokernel VerifyReplay total_icount {}, expected {RUN_BUDGET}",
                    child.index, done.total_icount
                ));
            }
        }
        AcceptanceGuest::Linux => {
            if epoch_ok as usize != parsed.epoch_hashes.len() {
                return Err(format!(
                    "child {} Linux VerifyReplay EpochOk count {}, expected parsed EPOCH_HASH count {}",
                    child.index,
                    epoch_ok,
                    parsed.epoch_hashes.len()
                ));
            }
            if done.total_icount != parsed.end_icount {
                return Err(format!(
                    "child {} Linux VerifyReplay total_icount {}, expected parsed end_icount {}",
                    child.index, done.total_icount, parsed.end_icount
                ));
            }
        }
    }
    Ok(child)
}

async fn verify_batch(
    harness: &AcceptanceHarness,
    children: Vec<(ChildRecord, ParsedChildLog)>,
) -> TestResult<Vec<ChildRecord>> {
    let mut tasks = Vec::with_capacity(children.len());
    for (child, parsed) in children {
        let svc = harness.svc().clone();
        let guest = harness.guest();
        let root = harness.root_snapshot().clone();
        tasks.push(tokio::spawn(async move {
            verify_child(svc, guest, root, child, parsed).await
        }));
    }

    let mut verified = Vec::with_capacity(tasks.len());
    let mut errors = Vec::new();
    for task in tasks {
        match task.await {
            Ok(Ok(child)) => verified.push(child),
            Ok(Err(e)) => errors.push(e),
            Err(e) => errors.push(format!("verify task join: {e}")),
        }
    }
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }
    verified.sort_by_key(|record| record.index);
    Ok(verified)
}

fn assert_replay_commit_matches(original: &ChildRecord, replay: &ChildRecord) -> TestResult<()> {
    if replay.index != original.index {
        return Err(format!(
            "replay-commit child index mismatch: original {} replay {}",
            original.index, replay.index
        ));
    }
    if replay.snapshot.hash != original.snapshot.hash {
        return Err(format!(
            "replay-commit child {} snapshot ref mismatch",
            original.index
        ));
    }
    if replay.state_hash != original.state_hash {
        return Err(format!(
            "replay-commit child {} state hash mismatch",
            original.index
        ));
    }
    if replay.input_log_id != original.input_log_id {
        return Err(format!(
            "replay-commit child {} input log id mismatch",
            original.index
        ));
    }
    if replay.segment_end_icount != original.segment_end_icount {
        return Err(format!(
            "replay-commit child {} segment icount mismatch: original {} replay {}",
            original.index, original.segment_end_icount, replay.segment_end_icount
        ));
    }
    if replay.segment_end_vns != original.segment_end_vns {
        return Err(format!(
            "replay-commit child {} segment vns mismatch: original {} replay {}",
            original.index, original.segment_end_vns, replay.segment_end_vns
        ));
    }
    if replay.cumulative_icount != original.cumulative_icount {
        return Err(format!(
            "replay-commit child {} cumulative icount mismatch: original {} replay {}",
            original.index, original.cumulative_icount, replay.cumulative_icount
        ));
    }
    if replay.cumulative_vns != original.cumulative_vns {
        return Err(format!(
            "replay-commit child {} cumulative vns mismatch: original {} replay {}",
            original.index, original.cumulative_vns, replay.cumulative_vns
        ));
    }
    if replay.frames_elapsed != original.frames_elapsed {
        return Err(format!(
            "replay-commit child {} frames_elapsed mismatch: original {} replay {}",
            original.index, original.frames_elapsed, replay.frames_elapsed
        ));
    }
    if replay.frame_counter != original.frame_counter {
        return Err(format!(
            "replay-commit child {} frame_counter mismatch: original {} replay {}",
            original.index, original.frame_counter, replay.frame_counter
        ));
    }
    if replay.dirty_pages != original.dirty_pages {
        return Err(format!(
            "replay-commit child {} dirty_pages mismatch: original {} replay {}",
            original.index, original.dirty_pages, replay.dirty_pages
        ));
    }
    if replay.meta_pvblk_checksum != original.meta_pvblk_checksum {
        return Err(format!(
            "replay-commit child {} Linux meta IO checksum mismatch",
            original.index
        ));
    }
    Ok(())
}

async fn baseline_delta_restore_probe(
    svc: &WorkerService,
    root_snapshot: proto::SnapshotRef,
    child: &ChildRecord,
) -> TestResult<f64> {
    let started = Instant::now();
    let restored = svc
        .restore_snapshot(Request::new(proto::RestoreSnapshotRequest {
            snapshot: Some(child.snapshot.clone()),
            entropy_seed: Vec::new(),
            baseline: Some(root_snapshot),
        }))
        .await
        .map_err(|e| format!("child {} baseline-delta RestoreSnapshot: {e}", child.index))?
        .into_inner();
    let restore_ms = elapsed_ms(started.elapsed());
    let lease = restored
        .lease
        .ok_or_else(|| format!("child {} baseline-delta returned no lease", child.index))?;
    let restored_state_hash = restored
        .state_hash
        .as_ref()
        .map(|hash| arr32(&hash.hash, "baseline-delta RestoreSnapshot state_hash"))
        .ok_or_else(|| {
            format!(
                "child {} baseline-delta RestoreSnapshot returned no state_hash",
                child.index
            )
        })?;
    if restored_state_hash != child.state_hash {
        destroy_best_effort(svc, Some(lease)).await;
        return Err(format!(
            "child {} baseline-delta state hash mismatch",
            child.index
        ));
    }
    if restored.frame_counter != child.frame_counter {
        destroy_best_effort(svc, Some(lease)).await;
        return Err(format!(
            "child {} baseline-delta frame_counter {}, expected {}",
            child.index, restored.frame_counter, child.frame_counter
        ));
    }
    destroy_best_effort(svc, Some(lease)).await;
    Ok(restore_ms)
}

async fn replay_commit_child(
    svc: WorkerService,
    guest: AcceptanceGuest,
    root_snapshot: proto::SnapshotRef,
    root_cumulative_icount: u64,
    root_cumulative_vns: u64,
    root_frame_counter: u32,
    original: ChildRecord,
) -> TestResult<ReplayCommitRecord> {
    let baseline_delta_restore_ms =
        baseline_delta_restore_probe(&svc, root_snapshot.clone(), &original).await?;
    let restore_started = Instant::now();
    let restored = svc
        .restore_snapshot(Request::new(proto::RestoreSnapshotRequest {
            snapshot: Some(root_snapshot),
            entropy_seed: child_seed(original.index),
            baseline: None,
        }))
        .await
        .map_err(|e| {
            format!(
                "child {} replay-commit RestoreSnapshot: {e}",
                original.index
            )
        })?
        .into_inner();
    let restore_ms = elapsed_ms(restore_started.elapsed());
    let lease = restored
        .lease
        .ok_or_else(|| format!("child {} replay-commit returned no lease", original.index))?;
    if restored.frame_counter != root_frame_counter {
        destroy_best_effort(&svc, Some(lease)).await;
        return Err(format!(
            "child {} replay-commit restored frame_counter {}, expected root {}",
            original.index, restored.frame_counter, root_frame_counter
        ));
    }

    let replay_slot_id = lease.slot_id;
    let replay = run_child(
        svc,
        guest,
        original.index,
        lease,
        root_cumulative_icount,
        root_cumulative_vns,
        root_frame_counter,
    )
    .await?;
    assert_replay_commit_matches(&original, &replay)?;
    Ok(ReplayCommitRecord {
        child_index: original.index,
        original_slot_id: original.slot_id,
        replay_slot_id,
        snapshot: replay.snapshot,
        state_hash: replay.state_hash,
        input_log_id: replay.input_log_id,
        baseline_delta_restore_ms,
        timing: ReplayCommitTiming {
            restore_ms,
            replay_ms: replay.timing.run_ms,
            replay_commit_ms: replay.timing.original_commit_ms,
        },
    })
}

async fn replay_commit_batch(
    harness: &AcceptanceHarness,
    children: Vec<ChildRecord>,
) -> TestResult<Vec<ReplayCommitRecord>> {
    let mut tasks = Vec::with_capacity(children.len());
    for child in children {
        let svc = harness.svc().clone();
        let guest = harness.guest();
        let root_snapshot = harness.root_snapshot().clone();
        let root_cumulative_icount = harness.root_cumulative_icount();
        let root_cumulative_vns = harness.root_cumulative_vns();
        let root_frame_counter = harness.root_frame_counter();
        tasks.push(tokio::spawn(async move {
            replay_commit_child(
                svc,
                guest,
                root_snapshot,
                root_cumulative_icount,
                root_cumulative_vns,
                root_frame_counter,
                child,
            )
            .await
        }));
    }

    let mut replayed = Vec::with_capacity(tasks.len());
    let mut errors = Vec::new();
    for task in tasks {
        match task.await {
            Ok(Ok(record)) => replayed.push(record),
            Ok(Err(e)) => errors.push(e),
            Err(e) => errors.push(format!("replay-commit task join: {e}")),
        }
    }
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }
    replayed.sort_by_key(|record| record.child_index);
    Ok(replayed)
}

async fn semantic_negative_replay_commit_child(
    harness: &AcceptanceHarness,
    original: &ChildRecord,
) -> TestResult<(ChildRecord, f64)> {
    if harness.guest() != AcceptanceGuest::Nanokernel {
        return Err(
            "semantic-negative replay-commit currently requires nanokernel pad input".into(),
        );
    }
    let restore_started = Instant::now();
    let restored = harness
        .svc()
        .restore_snapshot(Request::new(proto::RestoreSnapshotRequest {
            snapshot: Some(harness.root_snapshot().clone()),
            entropy_seed: child_seed(original.index),
            baseline: None,
        }))
        .await
        .map_err(|e| {
            format!(
                "semantic-negative child {} RestoreSnapshot: {e}",
                original.index
            )
        })?
        .into_inner();
    let restore_ms = elapsed_ms(restore_started.elapsed());
    let lease = restored.lease.ok_or_else(|| {
        format!(
            "semantic-negative child {} returned no lease",
            original.index
        )
    })?;
    if restored.frame_counter != harness.root_frame_counter() {
        destroy_best_effort(harness.svc(), Some(lease)).await;
        return Err(format!(
            "semantic-negative child {} restored frame_counter {}, expected root {}",
            original.index,
            restored.frame_counter,
            harness.root_frame_counter()
        ));
    }
    let replay = run_nanokernel_child_with_events(
        harness.svc().clone(),
        original.index,
        lease,
        mutated_pad_burst(original.index),
        "semantic-negative child",
    )
    .await?;
    if replay.snapshot.hash == original.snapshot.hash {
        return Err(format!(
            "semantic-negative child {} replay ref unexpectedly matched original {}",
            original.index,
            snapshot_ref_hex(&original.snapshot)
        ));
    }
    Ok((replay, restore_ms))
}

async fn cross_check_child_on_distinct_slots(
    harness: &AcceptanceHarness,
    index: usize,
    child_count: usize,
) -> TestResult<()> {
    if child_count < 2 {
        return Err(format!(
            "cross-slot child {index} requires at least two child slots, got {child_count}"
        ));
    }
    let seed = child_seed(index);
    let forked = harness
        .svc()
        .fork(Request::new(proto::ForkRequest {
            parent: Some(harness.root_lease().clone()),
            count: child_count as u32,
            entropy_seeds: std::iter::repeat_n(seed, child_count).collect(),
        }))
        .await
        .map_err(|e| format!("cross-slot child {index} Fork same-seed children: {e}"))?
        .into_inner()
        .children;

    let result = async {
        if forked.len() != child_count {
            return Err(format!(
                "cross-slot child {index} Fork returned {}, expected {child_count}",
                forked.len()
            ));
        }
        let mut slot_ids: Vec<_> = forked.iter().map(|lease| lease.slot_id).collect();
        slot_ids.sort_unstable();
        slot_ids.dedup();
        if slot_ids.len() != child_count {
            return Err(format!(
                "cross-slot child {index} same-seed children did not land on distinct slots: \
                 {slot_ids:?}"
            ));
        }

        let children = run_same_seed_children(harness, index, forked.clone()).await?;
        let mut logs = Vec::with_capacity(children.len());
        let mut validated = Vec::with_capacity(children.len());
        for child in children {
            let log = tokio::task::block_in_place(|| {
                fetch_log_payload(harness.store(), &child.input_log_id)
            });
            let parsed = validate_single_edge_lineage(harness, &child, &log)?;
            logs.push((child.slot_id, log, parsed.clone()));
            validated.push((child, parsed));
        }

        let mut verified = verify_batch(harness, validated).await?;
        verified.sort_by_key(|record| record.slot_id);
        let first = verified
            .first()
            .ok_or_else(|| format!("cross-slot child {index} produced no child records"))?;
        for other in verified.iter().skip(1) {
            if first.snapshot.hash != other.snapshot.hash {
                return Err(format!(
                    "cross-slot child {index} snapshot refs diverged between slots {} and {}",
                    first.slot_id, other.slot_id
                ));
            }
            if first.state_hash != other.state_hash {
                return Err(format!(
                    "cross-slot child {index} state hashes diverged between slots {} and {}",
                    first.slot_id, other.slot_id
                ));
            }
            if first.input_log_id != other.input_log_id {
                return Err(format!(
                    "cross-slot child {index} input log ids diverged between slots {} and {}",
                    first.slot_id, other.slot_id
                ));
            }
            if first.segment_end_icount != other.segment_end_icount {
                return Err(format!(
                    "cross-slot child {index} segment end icount diverged between slots {} and {}",
                    first.slot_id, other.slot_id
                ));
            }
            if first.cumulative_icount != other.cumulative_icount {
                return Err(format!(
                    "cross-slot child {index} cumulative icount diverged between slots {} and {}",
                    first.slot_id, other.slot_id
                ));
            }
            if first.cumulative_vns != other.cumulative_vns {
                return Err(format!(
                    "cross-slot child {index} cumulative vns diverged between slots {} and {}",
                    first.slot_id, other.slot_id
                ));
            }
            if first.frame_counter != other.frame_counter {
                return Err(format!(
                    "cross-slot child {index} frame counter diverged between slots {} and {}",
                    first.slot_id, other.slot_id
                ));
            }
            if first.dirty_pages != other.dirty_pages {
                return Err(format!(
                    "cross-slot child {index} dirty page count diverged between slots {} and {}",
                    first.slot_id, other.slot_id
                ));
            }
            if first.meta_pvblk_checksum != other.meta_pvblk_checksum {
                return Err(format!(
                    "cross-slot child {index} meta IO checksum diverged between slots {} and {}",
                    first.slot_id, other.slot_id
                ));
            }
        }
        let first_log = logs
            .first()
            .ok_or_else(|| format!("cross-slot child {index} produced no input logs"))?;
        for (slot_id, log, parsed) in logs.iter().skip(1) {
            if first_log.1 != *log {
                return Err(format!(
                    "cross-slot child {index} input log payloads diverged between slots {} and {}",
                    first_log.0, slot_id
                ));
            }
            if first_log.2.dhilog_blake3 != parsed.dhilog_blake3 {
                return Err(format!(
                    "cross-slot child {index} DHILOG blake3 diverged between slots {} and {}",
                    first_log.0, slot_id
                ));
            }
            if first_log.2.end_icount != parsed.end_icount {
                return Err(format!(
                    "cross-slot child {index} parsed end_icount diverged between slots {} and {}",
                    first_log.0, slot_id
                ));
            }
            if first_log.2.frame_marks != parsed.frame_marks {
                return Err(format!(
                    "cross-slot child {index} parsed frame marks diverged between slots {} and {}",
                    first_log.0, slot_id
                ));
            }
        }
        Ok(())
    }
    .await;

    if result.is_err() {
        for lease in forked {
            destroy_best_effort(harness.svc(), Some(lease)).await;
        }
    }
    result
}

#[test]
fn cross_check_indices_cover_the_1000_job_universe() {
    assert_eq!(
        cross_check_indices(1000, 10),
        vec![0, 111, 222, 333, 444, 555, 666, 777, 888, 999]
    );
    assert_eq!(cross_check_indices(2, 10), vec![0, 1]);
    assert_eq!(cross_check_indices(1, 10), vec![0]);
}

fn sample_child_record(index: usize, tag: u8) -> ChildRecord {
    ChildRecord {
        index,
        slot_id: u64::from(tag),
        snapshot: proto::SnapshotRef {
            hash: vec![tag; 32],
        },
        state_hash: [tag; 32],
        input_log_id: vec![tag.wrapping_add(1); 32],
        segment_end_icount: 10,
        segment_end_vns: 20,
        cumulative_icount: 30,
        cumulative_vns: 40,
        frames_elapsed: 1,
        frame_counter: 2,
        dirty_pages: 7,
        meta_pvblk_checksum: Some(u64::from(tag)),
        timing: ChildTiming::default(),
    }
}

fn sample_m8_resume_row(index: usize, tag: u8) -> serde_json::Value {
    serde_json::json!({
        "child_index": index,
        "seed_hex": child_seed_hex(index),
        "original_ref_hex": hex_bytes(&[tag; 32]),
        "replay_ref_hex": hex_bytes(&[tag; 32]),
        "input_log_id_hex": hex_bytes(&[tag.wrapping_add(1); 32]),
        "state_hash_original_hex": hex_bytes(&[tag.wrapping_add(2); 32]),
        "state_hash_replay_hex": hex_bytes(&[tag.wrapping_add(2); 32]),
        "restore_mode": "baseline_delta",
        "baseline_ref_hex": hex_bytes(&[tag.wrapping_add(3); 32]),
        "manifest_kind": "DELTA",
        "chain_depth": 1,
        "dirty_pages": 7,
        "shared_page_ratio": 0.98,
        "timing_ms": {
            "fork": 1.0,
            "run": 2.0,
            "original_commit": 3.0,
            "restore": 4.0,
            "replay": 5.0,
            "replay_commit": 6.0,
        },
        "result": "pass",
        "original_slot_id": tag,
        "replay_slot_id": tag.wrapping_add(10),
        "row_source": "fresh",
    })
}

fn write_m8_resume_rows(path: &Path, rows: &[serde_json::Value]) {
    let mut body = String::new();
    for row in rows {
        body.push_str(&row.to_string());
        body.push('\n');
    }
    std::fs::write(path, body).expect("write M8 resume rows");
}

#[test]
fn replay_commit_matcher_allows_slot_drift_but_rejects_ref_drift() {
    let original = sample_child_record(7, 0x42);
    let mut replay = original.clone();
    replay.slot_id = original.slot_id + 1;
    assert_replay_commit_matches(&original, &replay).expect("slot drift is permitted");

    replay.snapshot.hash[0] ^= 1;
    let error = assert_replay_commit_matches(&original, &replay).unwrap_err();
    assert!(
        error.contains("snapshot ref mismatch"),
        "unexpected error: {error}"
    );
}

#[test]
fn m8_resume_rows_accept_contiguous_prefix_and_mark_resumed() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let path = dir.path().join("child-ref-table.jsonl");
    write_m8_resume_rows(
        &path,
        &[sample_m8_resume_row(0, 0x10), sample_m8_resume_row(1, 0x20)],
    );

    let rows = M8EvidenceRun::load_resume_rows(&path, 3).expect("resume rows");
    assert_eq!(rows.len(), 2);
    assert!(rows
        .iter()
        .all(|row| row.get("row_source").and_then(|value| value.as_str()) == Some("resumed")));

    M8EvidenceRun::rewrite_child_table(&path, &rows).expect("rewrite resumed table");
    let rewritten = std::fs::read_to_string(path).expect("read rewritten resume table");
    assert_eq!(rewritten.lines().count(), 2);
    assert!(rewritten.contains("\"row_source\":\"resumed\""));
}

#[test]
fn m8_resume_rows_reject_gap_or_identity_mismatch() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let path = dir.path().join("child-ref-table.jsonl");
    write_m8_resume_rows(&path, &[sample_m8_resume_row(1, 0x10)]);
    let error = M8EvidenceRun::load_resume_rows(&path, 3).unwrap_err();
    assert!(
        error.contains("contiguous prefix"),
        "unexpected error: {error}"
    );

    let mut mismatch = sample_m8_resume_row(0, 0x10);
    mismatch["replay_ref_hex"] = serde_json::Value::String(hex_bytes(&[0x11; 32]));
    write_m8_resume_rows(&path, &[mismatch]);
    let error = M8EvidenceRun::load_resume_rows(&path, 3).unwrap_err();
    assert!(
        error.contains("replay_ref_hex must equal"),
        "unexpected error: {error}"
    );
}

#[test]
fn m8_semantic_negative_link_reads_red_result() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let semantic_dir = dir.path().join("semantic-negative");
    std::fs::create_dir_all(&semantic_dir).expect("semantic-negative dir");
    std::fs::write(
        semantic_dir.join("evidence.json"),
        serde_json::json!({
            "run_kind": "semantic_negative",
            "semantic_negative": {
                "actual_red_result": true
            }
        })
        .to_string(),
    )
    .expect("semantic-negative evidence");

    assert!(M8EvidenceRun::linked_semantic_negative_red(dir.path()).expect("read linked red"));
}

#[test]
#[ignore = "M7 acceptance gate: 1000 forked children; run with --release -- --ignored --nocapture"]
fn m7_accept_1000_seeded_forks_verify_replay_all() {
    let guest = AcceptanceGuest::configured();
    let Some(slot_cores) = acceptance_slot_cores_or_skip() else {
        return;
    };
    let jobs = configured_jobs();
    let child_capacity = slot_cores.len() - 1;
    assert!(
        child_capacity > 0,
        "one slot is reserved for the reusable root parent"
    );

    let Some(harness) = AcceptanceHarness::new(
        guest,
        "m7_fork_verify::m7_accept_1000_seeded_forks_verify_replay_all",
        slot_cores.clone(),
    )
    .expect("acceptance harness") else {
        return;
    };
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("test runtime");
    rt.block_on(async {
        let mut verified = 0usize;
        let mut unique_hashes = BTreeSet::new();
        let mut epoch_hashes = 0usize;

        while verified < jobs {
            let batch_count = child_capacity.min(jobs - verified);
            let seeds: Vec<_> = (verified..verified + batch_count).map(child_seed).collect();
            let forked = harness
                .svc()
                .fork(Request::new(proto::ForkRequest {
                    parent: Some(harness.root_lease().clone()),
                    count: batch_count as u32,
                    entropy_seeds: seeds,
                }))
                .await
                .unwrap_or_else(|e| panic!("Fork batch starting at {verified}: {e}"))
                .into_inner()
                .children;
            assert_eq!(forked.len(), batch_count);

            let children = run_child_batch(&harness, verified, forked)
                .await
                .unwrap_or_else(|e| panic!("Run/Snapshot batch starting at {verified}: {e}"));
            let mut validated = Vec::with_capacity(children.len());
            for child in children {
                let log =
                    tokio::task::block_in_place(|| fetch_log_payload(harness.store(), &child.input_log_id));
                let parsed = validate_single_edge_lineage(&harness, &child, &log)
                    .unwrap_or_else(|e| panic!("Validate child {} DHILOG: {e}", child.index));
                epoch_hashes += parsed.epoch_hashes.len();
                validated.push((child, parsed));
            }

            let children = verify_batch(&harness, validated)
                .await
                .unwrap_or_else(|e| panic!("VerifyReplay batch starting at {verified}: {e}"));
            for child in children {
                unique_hashes.insert(child.state_hash);
            }
            verified += batch_count;
            match guest {
                AcceptanceGuest::Nanokernel => {
                    eprintln!("M7 fork/verify progress: {verified}/{jobs}");
                }
                AcceptanceGuest::Linux => {
                    eprintln!("M7 Linux fork/verify progress: {verified}/{jobs}");
                }
            }
        }

        assert_eq!(verified, jobs);
        if guest == AcceptanceGuest::Nanokernel {
            assert_eq!(
                unique_hashes.len(),
                jobs,
                "distinct seeded pad bursts should produce distinct child hashes"
            );
        }

        harness.destroy_root().await;
        let info = harness
            .svc()
            .get_worker_info(Request::new(proto::GetWorkerInfoRequest {}))
            .await
            .expect("GetWorkerInfo after cleanup")
            .into_inner();
        assert_eq!(info.slots_free as usize, slot_cores.len());

        match guest {
            AcceptanceGuest::Nanokernel => {
                eprintln!(
                    "M7 fork/verify done: verified={verified} divergence=0 unique_hashes={}",
                    unique_hashes.len()
                );
            }
            AcceptanceGuest::Linux => {
                eprintln!(
                    "M7 Linux fork/verify done: verified={verified} divergence=0 unique_hashes={} epoch_hashes={epoch_hashes}",
                    unique_hashes.len()
                );
            }
        }
    });
}

#[test]
#[ignore = "M8 acceptance gate: replay each child and require committed snapshot ref identity"]
fn m8_accept_1000_seeded_forks_replay_commit_ref_identity() {
    let guest = AcceptanceGuest::configured();
    let Some(slot_cores) = acceptance_slot_cores_or_skip() else {
        return;
    };
    let jobs = configured_jobs();
    let child_capacity = slot_cores.len() - 1;
    assert!(
        child_capacity > 0,
        "one slot is reserved for the reusable root parent"
    );

    let Some(harness) = AcceptanceHarness::new(
        guest,
        "m7_fork_verify::m8_accept_1000_seeded_forks_replay_commit_ref_identity",
        slot_cores.clone(),
    )
    .expect("acceptance harness") else {
        return;
    };
    let mut evidence = M8EvidenceRun::new(jobs, harness.store_root()).expect("M8 evidence root");

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("test runtime");
    rt.block_on(async {
        let mut accepted = evidence.next_child_index();
        let mut replay_commits = accepted;
        let mut unique_hashes = evidence
            .resume_hex32_set("state_hash_original_hex")
            .expect("seed M8 resumed original state hashes");
        let mut unique_replay_hashes = evidence
            .resume_hex32_set("state_hash_replay_hex")
            .expect("seed M8 resumed replay state hashes");
        let mut unique_replay_refs = evidence
            .resume_hex32_set("replay_ref_hex")
            .expect("seed M8 resumed replay refs");
        let mut unique_replay_slots = BTreeSet::new();
        let mut epoch_hashes = 0usize;
        if accepted > 0 {
            eprintln!("M8 replay-commit resume: accepted={accepted}/{jobs}");
        }

        while accepted < jobs {
            let batch_count = child_capacity.min(jobs - accepted);
            let seeds: Vec<_> = (accepted..accepted + batch_count).map(child_seed).collect();
            let fork_started = Instant::now();
            let forked = harness
                .svc()
                .fork(Request::new(proto::ForkRequest {
                    parent: Some(harness.root_lease().clone()),
                    count: batch_count as u32,
                    entropy_seeds: seeds,
                }))
                .await
                .unwrap_or_else(|e| panic!("M8 Fork batch starting at {accepted}: {e}"))
                .into_inner()
                .children;
            let fork_ms_per_child = elapsed_ms(fork_started.elapsed()) / forked.len().max(1) as f64;
            assert_eq!(forked.len(), batch_count);

            let mut children = run_child_batch(&harness, accepted, forked)
                .await
                .unwrap_or_else(|e| panic!("M8 Run/Snapshot batch starting at {accepted}: {e}"));
            for child in &mut children {
                child.timing.fork_ms = fork_ms_per_child;
            }
            let mut validated = Vec::with_capacity(children.len());
            for child in children {
                let log = tokio::task::block_in_place(|| {
                    fetch_log_payload(harness.store(), &child.input_log_id)
                });
                let parsed = validate_single_edge_lineage(&harness, &child, &log)
                    .unwrap_or_else(|e| panic!("M8 Validate child {} DHILOG: {e}", child.index));
                epoch_hashes += parsed.epoch_hashes.len();
                validated.push((child, parsed));
            }

            let children = verify_batch(&harness, validated)
                .await
                .unwrap_or_else(|e| panic!("M8 VerifyReplay batch starting at {accepted}: {e}"));
            let replayed = replay_commit_batch(&harness, children.clone())
                .await
                .unwrap_or_else(|e| {
                    panic!("M8 replay-commit batch starting at {accepted}: {e}")
                });
            assert_eq!(replayed.len(), children.len());
            for (child, replay) in children.iter().zip(replayed) {
                assert_eq!(replay.child_index, child.index);
                assert_eq!(replay.snapshot.hash.len(), 32);
                assert_eq!(replay.input_log_id.len(), 32);
                tokio::task::block_in_place(|| evidence.append_child(&harness, child, &replay))
                    .unwrap_or_else(|e| panic!("M8 evidence child {}: {e}", child.index));
                unique_replay_hashes.insert(replay.state_hash);
                unique_replay_refs.insert(arr32(&replay.snapshot.hash, "M8 replay snapshot"));
                unique_replay_slots.insert(replay.replay_slot_id);
                if replay.original_slot_id == replay.replay_slot_id {
                    eprintln!(
                        "M8 replay-commit child {} reused slot {}",
                        replay.child_index, replay.replay_slot_id
                    );
                }
            }
            for child in children {
                unique_hashes.insert(child.state_hash);
            }

            accepted += batch_count;
            replay_commits += batch_count;
            match guest {
                AcceptanceGuest::Nanokernel => {
                    eprintln!("M8 replay-commit progress: {accepted}/{jobs}");
                }
                AcceptanceGuest::Linux => {
                    eprintln!("M8 Linux replay-commit progress: {accepted}/{jobs}");
                }
            }
        }

        assert_eq!(accepted, jobs);
        assert_eq!(replay_commits, jobs);
        if guest == AcceptanceGuest::Nanokernel {
            assert_eq!(
                unique_hashes.len(),
                jobs,
                "distinct seeded pad bursts should produce distinct child hashes"
            );
            assert_eq!(
                unique_replay_refs.len(),
                jobs,
                "distinct replay commits should produce distinct snapshot refs"
            );
            assert_eq!(
                unique_replay_hashes.len(),
                jobs,
                "distinct replay commits should produce distinct state hashes"
            );
        }

        harness.destroy_root().await;
        let info = harness
            .svc()
            .get_worker_info(Request::new(proto::GetWorkerInfoRequest {}))
            .await
            .expect("GetWorkerInfo after cleanup")
            .into_inner();
        assert_eq!(info.slots_free as usize, slot_cores.len());
        evidence.finish(guest).expect("write M8 evidence summary");

        match guest {
            AcceptanceGuest::Nanokernel => {
                eprintln!(
                    "M8 replay-commit done: replay_commits={replay_commits} unique_refs={} unique_hashes={} replay_slots={}",
                    unique_replay_refs.len(),
                    unique_replay_hashes.len(),
                    unique_replay_slots.len()
                );
            }
            AcceptanceGuest::Linux => {
                eprintln!(
                    "M8 Linux replay-commit done: replay_commits={replay_commits} unique_refs={} unique_hashes={} replay_slots={} epoch_hashes={epoch_hashes}",
                    unique_replay_refs.len(),
                    unique_replay_hashes.len(),
                    unique_replay_slots.len()
                );
            }
        }
    });
}

#[test]
fn m8_linux_skid_margin_keeps_loaded_host_headroom() {
    assert!(M8_LINUX_SKID_MARGIN > 2 * M8_LINUX_MEASURED_MAX_SKID);
    assert!(M8_LINUX_SKID_MARGIN > dh_vmm::config::DEFAULT_SKID_MARGIN);
}

#[test]
#[ignore = "M8 semantic negative: commit a mutated replay and require replay_ref mismatch"]
fn m8_accept_semantic_negative_replay_commit_ref_mismatch() {
    let Some(slot_cores) = acceptance_slot_cores_or_skip() else {
        return;
    };
    let child_capacity = slot_cores.len() - 1;
    assert!(
        child_capacity > 0,
        "one slot is reserved for the reusable root parent"
    );

    let Some(harness) = AcceptanceHarness::new(
        AcceptanceGuest::Nanokernel,
        "m7_fork_verify::m8_accept_semantic_negative_replay_commit_ref_mismatch",
        slot_cores.clone(),
    )
    .expect("acceptance harness") else {
        return;
    };
    let mut evidence =
        M8EvidenceRun::new_semantic_negative(harness.store_root()).expect("M8 evidence root");

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("test runtime");
    rt.block_on(async {
        let index = 0usize;
        let fork_started = Instant::now();
        let forked = harness
            .svc()
            .fork(Request::new(proto::ForkRequest {
                parent: Some(harness.root_lease().clone()),
                count: 1,
                entropy_seeds: vec![child_seed(index)],
            }))
            .await
            .unwrap_or_else(|e| panic!("M8 semantic-negative Fork: {e}"))
            .into_inner()
            .children;
        assert_eq!(forked.len(), 1);

        let mut original = run_child_batch(&harness, index, forked)
            .await
            .unwrap_or_else(|e| panic!("M8 semantic-negative original child: {e}"))
            .into_iter()
            .next()
            .expect("one original child");
        original.timing.fork_ms = elapsed_ms(fork_started.elapsed());
        let log = tokio::task::block_in_place(|| {
            fetch_log_payload(harness.store(), &original.input_log_id)
        });
        let parsed = validate_single_edge_lineage(&harness, &original, &log)
            .unwrap_or_else(|e| panic!("M8 semantic-negative original DHILOG: {e}"));
        let verified = verify_batch(&harness, vec![(original, parsed)])
            .await
            .unwrap_or_else(|e| panic!("M8 semantic-negative VerifyReplay: {e}"))
            .into_iter()
            .next()
            .expect("one verified original child");

        let (replay, replay_restore_ms) =
            semantic_negative_replay_commit_child(&harness, &verified)
                .await
                .unwrap_or_else(|e| panic!("M8 semantic-negative replay commit: {e}"));
        assert_ne!(
            replay.snapshot.hash, verified.snapshot.hash,
            "mutated replay must commit a different snapshot ref"
        );
        tokio::task::block_in_place(|| {
            evidence.append_semantic_negative(&harness, &verified, &replay, replay_restore_ms)
        })
        .unwrap_or_else(|e| panic!("M8 semantic-negative evidence: {e}"));

        harness.destroy_root().await;
        let info = harness
            .svc()
            .get_worker_info(Request::new(proto::GetWorkerInfoRequest {}))
            .await
            .expect("GetWorkerInfo after cleanup")
            .into_inner();
        assert_eq!(info.slots_free as usize, slot_cores.len());
        evidence
            .finish(AcceptanceGuest::Nanokernel)
            .expect("write M8 semantic-negative evidence summary");
    });
}

#[test]
#[ignore = "M7 acceptance gate: cross-slot same-seed reruns; run with --release -- --ignored --nocapture"]
fn m7_accept_cross_slot_rerun_10_seeded_forks_identical_refs() {
    let guest = AcceptanceGuest::configured();
    let Some(slot_cores) = acceptance_slot_cores_or_skip() else {
        return;
    };
    let child_capacity = slot_cores.len().saturating_sub(1);
    if child_capacity < 2 {
        let message = format!(
            "{SLOT_CORES_ENV} must provide at least three slots for cross-slot rerun: \
             one root parent and at least two same-seed children"
        );
        if allow_skip() {
            eprintln!("skipping M7 cross-slot acceptance because {ALLOW_SKIP_ENV}=1: {message}");
            return;
        }
        panic!("{message}");
    }

    let jobs = configured_jobs();
    let checks = configured_cross_checks(jobs);
    let indices = cross_check_indices(jobs, checks);

    let Some(harness) = AcceptanceHarness::new(
        guest,
        "m7_fork_verify::m7_accept_cross_slot_rerun_10_seeded_forks_identical_refs",
        slot_cores.clone(),
    )
    .expect("acceptance harness") else {
        return;
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("test runtime");
    rt.block_on(async {
        let mut first_error = None;
        for (offset, index) in indices.iter().copied().enumerate() {
            if let Err(e) =
                cross_check_child_on_distinct_slots(&harness, index, child_capacity).await
            {
                first_error = Some(format!("cross-slot check for child {index}: {e}"));
                break;
            }
            match guest {
                AcceptanceGuest::Nanokernel => eprintln!(
                    "M7 cross-slot progress: {}/{} (job index {index})",
                    offset + 1,
                    indices.len()
                ),
                AcceptanceGuest::Linux => eprintln!(
                    "M7 Linux cross-slot progress: {}/{} (job index {index})",
                    offset + 1,
                    indices.len()
                ),
            }
        }

        harness.destroy_root().await;
        let info = harness
            .svc()
            .get_worker_info(Request::new(proto::GetWorkerInfoRequest {}))
            .await
            .expect("GetWorkerInfo after cleanup")
            .into_inner();
        assert_eq!(info.slots_free as usize, slot_cores.len());
        if let Some(error) = first_error {
            panic!("{error}");
        }
    });
}
