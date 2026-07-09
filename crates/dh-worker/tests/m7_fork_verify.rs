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
use std::path::{Path, PathBuf};

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
use snapstore_types::LogId;
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
const M9_LINUX_META_IO_MAGIC_OFF: u64 = 32;
const M9_LINUX_META_IO_PROOF_LEN: u64 = 24;
const JOBS_ENV: &str = "DH_M7_ACCEPT_JOBS";
const SLOT_CORES_ENV: &str = "DH_M7_ACCEPT_SLOT_CORES";
const ALLOW_SKIP_ENV: &str = "DH_M7_ACCEPT_ALLOW_SKIP";
const CROSS_CHECKS_ENV: &str = "DH_M7_CROSS_CHECKS";
const GUEST_ENV: &str = "DH_M7_ACCEPT_GUEST";

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
    meta_pvblk_checksum: Option<u64>,
}

#[derive(Clone, Debug)]
struct ReplayCommitRecord {
    child_index: usize,
    original_slot_id: u64,
    replay_slot_id: u64,
    snapshot: proto::SnapshotRef,
    state_hash: [u8; 32],
    input_log_id: Vec<u8>,
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
        _store_dir: tempfile::TempDir,
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

        let store_dir = tempfile::TempDir::new().map_err(|e| format!("snapstore tempdir: {e}"))?;
        let store_sock = "snapstore.sock";
        let (_store_rt, _store_handle, store) =
            common::spawn_store_at(store_dir.path().to_path_buf(), store_sock);
        let snapstore = snapstore_client::Transport::Uds(store_dir.path().join(store_sock));
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
            _store_dir: store_dir,
            _image_cache: image_cache,
        })
    }

    fn new_linux(test_name: &str, slot_cores: Vec<u32>) -> TestResult<Option<Self>> {
        let Some(ready) = common::m9_linux_ready_snapshot_with_slot_cores_and_config(
            test_name,
            slot_cores,
            |config| {
                config.epoch_len = M9_LINUX_CHILD_EPOCH_LEN;
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
        meta_pvblk_checksum,
    })
}

async fn run_nanokernel_child(
    svc: WorkerService,
    index: usize,
    lease: proto::Lease,
) -> TestResult<ChildRecord> {
    let slot_id = lease.slot_id;
    let scheduled = match svc
        .inject_inputs(Request::new(proto::InjectInputsRequest {
            lease: Some(lease.clone()),
            events: pad_burst(index),
        }))
        .await
    {
        Ok(response) => response.into_inner().scheduled,
        Err(e) => {
            destroy_best_effort(&svc, Some(lease)).await;
            return Err(format!("child {index} InjectInputs: {e}"));
        }
    };
    if scheduled as usize != BURST_EVENTS {
        destroy_best_effort(&svc, Some(lease)).await;
        return Err(format!(
            "child {index} scheduled {scheduled}, expected {BURST_EVENTS}"
        ));
    }

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
            return Err(format!("child {index} Run: {e}"));
        }
    };
    if run.reason != i32::from(proto::StopReason::BudgetReached) {
        destroy_best_effort(&svc, Some(lease)).await;
        return Err(format!(
            "child {index} Run stopped with {}, expected BUDGET_REACHED",
            run.reason
        ));
    }
    if run.icount != RUN_BUDGET || run.vns != VNS_PER_SECOND {
        destroy_best_effort(&svc, Some(lease)).await;
        return Err(format!(
            "child {index} ended at icount={} vns={}, expected {RUN_BUDGET}/{VNS_PER_SECOND}",
            run.icount, run.vns
        ));
    }

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
            return Err(format!("child {index} TakeSnapshot: {e}"));
        }
    };
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
    if replay.meta_pvblk_checksum != original.meta_pvblk_checksum {
        return Err(format!(
            "replay-commit child {} Linux meta IO checksum mismatch",
            original.index
        ));
    }
    Ok(())
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
    let restored = svc
        .restore_snapshot(Request::new(proto::RestoreSnapshotRequest {
            snapshot: Some(root_snapshot),
            entropy_seed: child_seed(original.index),
        }))
        .await
        .map_err(|e| {
            format!(
                "child {} replay-commit RestoreSnapshot: {e}",
                original.index
            )
        })?
        .into_inner();
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
        meta_pvblk_checksum: Some(u64::from(tag)),
    }
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

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("test runtime");
    rt.block_on(async {
        let mut accepted = 0usize;
        let mut replay_commits = 0usize;
        let mut unique_hashes = BTreeSet::new();
        let mut unique_replay_hashes = BTreeSet::new();
        let mut unique_replay_refs = BTreeSet::new();
        let mut unique_replay_slots = BTreeSet::new();
        let mut epoch_hashes = 0usize;

        while accepted < jobs {
            let batch_count = child_capacity.min(jobs - accepted);
            let seeds: Vec<_> = (accepted..accepted + batch_count).map(child_seed).collect();
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
            assert_eq!(forked.len(), batch_count);

            let children = run_child_batch(&harness, accepted, forked)
                .await
                .unwrap_or_else(|e| panic!("M8 Run/Snapshot batch starting at {accepted}: {e}"));
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
            for (offset, replay) in replayed.into_iter().enumerate() {
                assert_eq!(replay.child_index, accepted + offset);
                assert_eq!(replay.snapshot.hash.len(), 32);
                assert_eq!(replay.input_log_id.len(), 32);
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
