//! M7 ACCEPT (bead cw2): root snapshot, 1000 seeded fork children,
//! one guest-second random pad burst per child, then VerifyReplay every
//! child log with zero Divergence and matching end_state_hash.
//!
//! DHILOG v1 does not persist a byte-concatenated fork-tree artifact in
//! this repo. The splice contract is a validated sequence of independently
//! replayable edges, so this harness validates each child segment as the
//! single-edge lineage `(root_snapshot -> child_snapshot)` and then verifies
//! that edge through the worker VerifyReplay RPC path.
//!
//! HARDWARE-GATED: ignored by default; the acceptance command is:
//!
//!   cargo test -p dh-worker --test m7_fork_verify --release \
//!     -- --ignored --nocapture
//!
//! Defaults are 1000 jobs and slot cores 2-65 (64 slots: one root parent
//! plus 63 children per batch). Developer smoke on small machines may set:
//!
//!   DH_M7_ACCEPT_JOBS=2 DH_M7_ACCEPT_SLOT_CORES=0-1 \
//!     cargo test -p dh-worker --test m7_fork_verify -- --ignored --nocapture
//!
//! The cross-slot acceptance gate samples the same 1000-job universe, forking
//! same-seed children across every available child slot and requiring identical
//! refs:
//!
//!   DH_M7_ACCEPT_SLOT_CORES=2-5 cargo test -p dh-worker \
//!     --test m7_fork_verify --release \
//!     m7_accept_cross_slot_rerun_10_seeded_forks_identical_refs \
//!     -- --ignored --nocapture
//!
//! By default it samples 10 indices from the 1000-job universe. Override
//! `DH_M7_ACCEPT_JOBS` to change the universe size and `DH_M7_CROSS_CHECKS`
//! to change the number of sampled indices.

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
const JOBS_ENV: &str = "DH_M7_ACCEPT_JOBS";
const SLOT_CORES_ENV: &str = "DH_M7_ACCEPT_SLOT_CORES";
const ALLOW_SKIP_ENV: &str = "DH_M7_ACCEPT_ALLOW_SKIP";
const CROSS_CHECKS_ENV: &str = "DH_M7_CROSS_CHECKS";

type TestResult<T> = Result<T, String>;

#[derive(Clone, Debug)]
struct ChildRecord {
    index: usize,
    slot_id: u64,
    snapshot: proto::SnapshotRef,
    state_hash: [u8; 32],
    input_log_id: Vec<u8>,
}

fn arr32(bytes: &[u8], what: &str) -> [u8; 32] {
    bytes
        .try_into()
        .unwrap_or_else(|_| panic!("{what} must be 32 bytes, got {}", bytes.len()))
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

fn pad_echo_config(base_hash: [u8; 32], kernel_hash: [u8; 32]) -> proto::MachineConfig {
    let mut config = MachineConfig::new(
        MEM,
        base_hash,
        BootSpec::Elf {
            kernel_hash,
            cmdline: Vec::new(),
        },
    );
    config.epoch_len = RUN_BUDGET;
    config.clock = ClockRatio::new(CLOCK_NUM, 1).expect("nonzero clock ratio");
    config.device_set = vec![
        dh_devices::clock::DEVICE_ID_PV_CLOCK,
        dh_devices::pad::DEVICE_ID_PV_PAD,
        dh_devices::entropy::DEVICE_ID_PV_ENTROPY,
        dh_devices::serial::DEVICE_ID_DEBUG_SERIAL,
    ];
    machine_config_to_proto(&config)
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
) -> TestResult<(proto::Lease, proto::SnapshotRef)> {
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
        .into_inner()
        .snapshot
        .ok_or_else(|| "TakeSnapshot root returned no snapshot".to_owned())?;
    Ok((lease, snapshot))
}

async fn destroy_best_effort(svc: &WorkerService, lease: Option<proto::Lease>) {
    if let Some(lease) = lease {
        let _ = svc
            .destroy_vm(Request::new(proto::DestroyVmRequest { lease: Some(lease) }))
            .await;
    }
}

async fn run_child(
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
    let snapshot_ref = snapshot
        .snapshot
        .ok_or_else(|| format!("child {index} TakeSnapshot returned no snapshot"))?;
    let state_hash = snapshot
        .state_hash
        .as_ref()
        .map(|hash| arr32(&hash.hash, "child state_hash"))
        .ok_or_else(|| format!("child {index} TakeSnapshot returned no state_hash"))?;
    if snapshot.input_log_id.len() != 32 {
        destroy_best_effort(&svc, Some(lease)).await;
        return Err(format!(
            "child {index} input_log_id length {}, expected 32",
            snapshot.input_log_id.len()
        ));
    }
    destroy_best_effort(&svc, Some(lease)).await;
    Ok(ChildRecord {
        index,
        slot_id,
        snapshot: snapshot_ref,
        state_hash,
        input_log_id: snapshot.input_log_id,
    })
}

async fn run_child_batch(
    svc: &WorkerService,
    start_index: usize,
    leases: Vec<proto::Lease>,
) -> TestResult<Vec<ChildRecord>> {
    let mut tasks = Vec::with_capacity(leases.len());
    for (offset, lease) in leases.into_iter().enumerate() {
        let svc = svc.clone();
        tasks.push(tokio::spawn(async move {
            run_child(svc, start_index + offset, lease).await
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
    svc: &WorkerService,
    index: usize,
    leases: Vec<proto::Lease>,
) -> TestResult<Vec<ChildRecord>> {
    let mut tasks = Vec::with_capacity(leases.len());
    for lease in leases {
        let svc = svc.clone();
        tasks.push(tokio::spawn(
            async move { run_child(svc, index, lease).await },
        ));
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

fn validate_single_edge_lineage(root: &proto::SnapshotRef, child: &ChildRecord, log: &[u8]) {
    let lineage = Lineage::new(&[log]).expect("child segment is a valid lineage edge");
    assert_eq!(lineage.len(), 1);
    assert_eq!(lineage.root_base(), arr32(&root.hash, "root snapshot"));
    let (end_snapshot, end_state_hash, end_icount) = lineage.end_identity();
    assert_eq!(end_snapshot, arr32(&child.snapshot.hash, "child snapshot"));
    assert_eq!(end_state_hash, child.state_hash);
    assert_eq!(end_icount, RUN_BUDGET);

    let reader = LogReader::parse(log).expect("child segment parses as DHILOG");
    let actual: Vec<_> = reader
        .canonical()
        .map(|rec| match rec.body() {
            RecordBody::PadSet { port, buttons, .. } => (rec.icount(), port, buttons),
            other => panic!(
                "child {} DHILOG contains unexpected canonical record {other:?}",
                child.index
            ),
        })
        .collect();
    assert_eq!(actual, expected_pad_records(child.index));
}

async fn verify_child(
    svc: WorkerService,
    root: proto::SnapshotRef,
    child: ChildRecord,
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
            Some(proto::verify_replay_progress::Msg::Done(msg)) => done = Some(msg),
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
    if done.total_icount != RUN_BUDGET {
        return Err(format!(
            "child {} VerifyReplay Done total_icount {}, expected {RUN_BUDGET}",
            child.index, done.total_icount
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
    Ok(child)
}

async fn verify_batch(
    svc: &WorkerService,
    root: &proto::SnapshotRef,
    children: Vec<ChildRecord>,
) -> TestResult<Vec<ChildRecord>> {
    let mut tasks = Vec::with_capacity(children.len());
    for child in children {
        let svc = svc.clone();
        let root = root.clone();
        tasks.push(tokio::spawn(
            async move { verify_child(svc, root, child).await },
        ));
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

async fn cross_check_child_on_distinct_slots(
    svc: &WorkerService,
    root_lease: &proto::Lease,
    root_snapshot: &proto::SnapshotRef,
    store: &snapstore_client::blocking::SnapstoreClient,
    index: usize,
    child_count: usize,
) -> TestResult<()> {
    if child_count < 2 {
        return Err(format!(
            "cross-slot child {index} requires at least two child slots, got {child_count}"
        ));
    }
    let seed = child_seed(index);
    let forked = svc
        .fork(Request::new(proto::ForkRequest {
            parent: Some(root_lease.clone()),
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

        let children = run_same_seed_children(svc, index, forked.clone()).await?;
        let mut logs = Vec::with_capacity(children.len());
        for child in &children {
            let log = tokio::task::block_in_place(|| fetch_log_payload(store, &child.input_log_id));
            validate_single_edge_lineage(root_snapshot, child, &log);
            logs.push((child.slot_id, log));
        }

        let mut verified = verify_batch(svc, root_snapshot, children).await?;
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
        }
        let first_log = logs
            .first()
            .ok_or_else(|| format!("cross-slot child {index} produced no input logs"))?;
        for (slot_id, log) in logs.iter().skip(1) {
            if first_log.1 != *log {
                return Err(format!(
                    "cross-slot child {index} input log payloads diverged between slots {} and {}",
                    first_log.0, slot_id
                ));
            }
        }
        Ok(())
    }
    .await;

    if result.is_err() {
        for lease in forked {
            destroy_best_effort(svc, Some(lease)).await;
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

#[test]
#[ignore = "M7 acceptance gate: 1000 forked children; run with --release -- --ignored --nocapture"]
fn m7_accept_1000_seeded_forks_verify_replay_all() {
    let Some(slot_cores) = acceptance_slot_cores_or_skip() else {
        return;
    };
    let jobs = configured_jobs();
    let child_capacity = slot_cores.len() - 1;
    assert!(
        child_capacity > 0,
        "one slot is reserved for the reusable root parent"
    );

    let image_cache = tempfile::TempDir::new().expect("image cache");
    let base_hash = write_cache_blob(image_cache.path(), &vec![0u8; 4096]);
    let kernel_hash = write_cache_blob(image_cache.path(), nanokernel::pad_echo_elf());
    let config = pad_echo_config(base_hash, kernel_hash);

    let store_dir = tempfile::TempDir::new().expect("snapstore data root");
    let store_sock = "snapstore.sock";
    let (_store_rt, _store_handle, store) =
        common::spawn_store_at(store_dir.path().to_path_buf(), store_sock);
    let snapstore = snapstore_client::Transport::Uds(store_dir.path().join(store_sock));

    let svc = WorkerService::new(worker_config(
        slot_cores,
        image_cache.path().to_path_buf(),
        snapstore,
    ))
    .expect("worker service");

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("test runtime");
    rt.block_on(async {
        let (root_lease, root_snapshot) = create_root(&svc, config).await.expect("root snapshot");
        let mut verified = 0usize;
        let mut unique_hashes = BTreeSet::new();

        while verified < jobs {
            let batch_count = child_capacity.min(jobs - verified);
            let seeds: Vec<_> = (verified..verified + batch_count).map(child_seed).collect();
            let forked = svc
                .fork(Request::new(proto::ForkRequest {
                    parent: Some(root_lease.clone()),
                    count: batch_count as u32,
                    entropy_seeds: seeds,
                }))
                .await
                .unwrap_or_else(|e| panic!("Fork batch starting at {verified}: {e}"))
                .into_inner()
                .children;
            assert_eq!(forked.len(), batch_count);

            let children = run_child_batch(&svc, verified, forked)
                .await
                .unwrap_or_else(|e| panic!("Run/Snapshot batch starting at {verified}: {e}"));
            for child in &children {
                let log =
                    tokio::task::block_in_place(|| fetch_log_payload(&store, &child.input_log_id));
                validate_single_edge_lineage(&root_snapshot, child, &log);
            }

            let children = verify_batch(&svc, &root_snapshot, children)
                .await
                .unwrap_or_else(|e| panic!("VerifyReplay batch starting at {verified}: {e}"));
            for child in children {
                unique_hashes.insert(child.state_hash);
            }
            verified += batch_count;
            eprintln!("M7 fork/verify progress: {verified}/{jobs}");
        }

        assert_eq!(verified, jobs);
        assert_eq!(
            unique_hashes.len(),
            jobs,
            "distinct seeded pad bursts should produce distinct child hashes"
        );

        destroy_best_effort(&svc, Some(root_lease)).await;
        let info = svc
            .get_worker_info(Request::new(proto::GetWorkerInfoRequest {}))
            .await
            .expect("GetWorkerInfo after cleanup")
            .into_inner();
        assert_eq!(info.slots_free as usize, child_capacity + 1);
    });
}

#[test]
#[ignore = "M7 acceptance gate: cross-slot same-seed reruns; run with --release -- --ignored --nocapture"]
fn m7_accept_cross_slot_rerun_10_seeded_forks_identical_refs() {
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

    let image_cache = tempfile::TempDir::new().expect("image cache");
    let base_hash = write_cache_blob(image_cache.path(), &vec![0u8; 4096]);
    let kernel_hash = write_cache_blob(image_cache.path(), nanokernel::pad_echo_elf());
    let config = pad_echo_config(base_hash, kernel_hash);

    let store_dir = tempfile::TempDir::new().expect("snapstore data root");
    let store_sock = "snapstore.sock";
    let (_store_rt, _store_handle, store) =
        common::spawn_store_at(store_dir.path().to_path_buf(), store_sock);
    let snapstore = snapstore_client::Transport::Uds(store_dir.path().join(store_sock));

    let svc = WorkerService::new(worker_config(
        slot_cores.clone(),
        image_cache.path().to_path_buf(),
        snapstore,
    ))
    .expect("worker service");

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("test runtime");
    rt.block_on(async {
        let (root_lease, root_snapshot) = create_root(&svc, config).await.expect("root snapshot");
        let mut first_error = None;
        for (offset, index) in indices.iter().copied().enumerate() {
            if let Err(e) = cross_check_child_on_distinct_slots(
                &svc,
                &root_lease,
                &root_snapshot,
                &store,
                index,
                child_capacity,
            )
            .await
            {
                first_error = Some(format!("cross-slot check for child {index}: {e}"));
                break;
            }
            eprintln!(
                "M7 cross-slot progress: {}/{} (job index {index})",
                offset + 1,
                indices.len()
            );
        }

        destroy_best_effort(&svc, Some(root_lease)).await;
        let info = svc
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
