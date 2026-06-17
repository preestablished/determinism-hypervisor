//! M6 ACCEPT (bead bik): drive the public worker API over UDS with 64
//! concurrently occupied slots. Every slot restores the same base snapshot,
//! injects the same input, runs the same segment, snapshots with a
//! `CaptureSpec`, and destroys the slot; the per-slot public hashes must
//! match the single-slot baseline.
//!
//! HARDWARE-GATED: requires KVM and 64 dedicated slot cores. By default this
//! uses cores 2-65 (housekeeping cores 0-1 reserved); override with
//! `DH_M6_ACCEPT_SLOT_CORES`, which must still resolve to exactly 64 cores.
//!
//!   cargo test -p dh-worker --test m6_full_api_uds --release -- --ignored --nocapture
//!
//! Developer smoke on under-provisioned hosts may set
//! `DH_M6_ACCEPT_ALLOW_SKIP=1`; the acceptance command above must fail when
//! prerequisites are not met.

#![cfg(target_arch = "x86_64")]

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use dh_proto::v1 as proto;
use dh_proto::v1::hypervisor_worker_client::HypervisorWorkerClient;
use dh_proto::v1::hypervisor_worker_server::HypervisorWorkerServer;
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_worker::proto_map::machine_config_to_proto;
use dh_worker::service::{PreflightHealth, WorkerConfig, WorkerService};
use dh_worker::slot_manager::{parse_core_list, LeasePolicy};
use hyper_util::rt::TokioIo;
use tokio::net::{UnixListener, UnixStream};
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::{Channel, Endpoint, Server};
use tower::service_fn;

const ACCEPT_SLOTS: usize = 64;
const MEM: u64 = 8 << 20;
const BASE_SLOT_CORE: u32 = 2;
const RUN_BUDGET: u64 = 10_000_000;
const INPUT_ICOUNT: u64 = 1;
const CAPTURE_OFFSET: u64 = 8;
const CAPTURE_LEN: u32 = 24;
const SLOT_CORES_ENV: &str = "DH_M6_ACCEPT_SLOT_CORES";
const ALLOW_SKIP_ENV: &str = "DH_M6_ACCEPT_ALLOW_SKIP";

type TestResult<T> = Result<T, String>;
type WorkerClient = HypervisorWorkerClient<Channel>;

struct WorkerUdsServer {
    _dir: tempfile::TempDir,
    uds_path: PathBuf,
    handle: tokio::task::JoinHandle<()>,
}

impl Drop for WorkerUdsServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LegDigest {
    hash: [u8; 32],
}

struct RestoredSlot {
    index: usize,
    lease: proto::Lease,
    restore: proto::RestoreSnapshotResponse,
}

fn write_cache_blob(root: &Path, bytes: &[u8]) -> [u8; 32] {
    let hash = *blake3::hash(bytes).as_bytes();
    std::fs::write(
        root.join(dh_worker::image_resolver::cache_key(&hash)),
        bytes,
    )
    .expect("write image-cache blob");
    hash
}

fn worker_config(
    slot_cores: Vec<u32>,
    image_cache_dir: PathBuf,
    snapstore: snapstore_client::Transport,
) -> WorkerConfig {
    WorkerConfig {
        worker_id: "m6-uds-accept-worker".into(),
        slot_cores,
        lease_policy: LeasePolicy::default(),
        class: proto::DeterminismClass {
            cpu_model: "m6-test-cpu".into(),
            microcode: "m6-test-ucode".into(),
            host_kernel: "m6-test-kernel".into(),
            vmm_version: "m6-test-vmm".into(),
        },
        preflight: PreflightHealth::skipped("m6 acceptance harness"),
        image_cache_dir,
        snapstore: Some(snapstore),
        bisection_checkpoints: dh_worker::service::BisectionCheckpointConfig::default(),
    }
}

fn capture_fixture_machine_config(
    base_hash: [u8; 32],
    kernel_hash: [u8; 32],
) -> proto::MachineConfig {
    let mut config = MachineConfig::new(
        MEM,
        base_hash,
        BootSpec::Elf {
            kernel_hash,
            cmdline: Vec::new(),
        },
    );
    config.device_set = vec![
        dh_devices::detchannel::DEVICE_ID_DETCHANNEL,
        dh_devices::clock::DEVICE_ID_PV_CLOCK,
        dh_devices::pad::DEVICE_ID_PV_PAD,
        dh_devices::entropy::DEVICE_ID_PV_ENTROPY,
        dh_devices::serial::DEVICE_ID_DEBUG_SERIAL,
    ];
    machine_config_to_proto(&config)
}

fn capture_spec() -> proto::CaptureSpec {
    proto::CaptureSpec {
        ranges: vec![proto::ExtractRange {
            region: "framebuffer".into(),
            layout_version: nanokernel::CAPTURE_FIXTURE_DEFAULT_LAYOUT_VERSION,
            offset: CAPTURE_OFFSET,
            len: CAPTURE_LEN,
        }],
        framebuffer: false,
    }
}

fn expected_capture_bytes() -> Vec<u8> {
    let mut fb = Vec::with_capacity(nanokernel::CAPTURE_FIXTURE_FB_BYTES as usize);
    for j in 0..nanokernel::CAPTURE_FIXTURE_FB_BYTES / 8 {
        fb.extend_from_slice(&(nanokernel::CAPTURE_FIXTURE_FB_QWORD_BASE + j).to_le_bytes());
    }
    let start = CAPTURE_OFFSET as usize;
    let end = start + CAPTURE_LEN as usize;
    fb[start..end].to_vec()
}

fn default_slot_cores() -> Vec<u32> {
    (BASE_SLOT_CORE..BASE_SLOT_CORE + ACCEPT_SLOTS as u32).collect()
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
    assert_eq!(
        cores.len(),
        ACCEPT_SLOTS,
        "{SLOT_CORES_ENV} must list exactly {ACCEPT_SLOTS} dedicated slot cores"
    );

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
        Err(e) if std::env::var(ALLOW_SKIP_ENV).as_deref() == Ok("1") => {
            eprintln!("skipping M6 UDS acceptance because {ALLOW_SKIP_ENV}=1: {e}");
            None
        }
        Err(e) => panic!(
            "M6 acceptance prerequisites failed: {e}. \
             Set {ALLOW_SKIP_ENV}=1 only for non-acceptance local smoke."
        ),
    }
}

async fn start_worker_uds(config: WorkerConfig) -> WorkerUdsServer {
    let dir = tempfile::TempDir::new().expect("worker tempdir");
    let uds_path = dir.path().join("dh-workerd.sock");
    let listener = UnixListener::bind(&uds_path).expect("bind worker UDS");
    let incoming = UnixListenerStream::new(listener);
    let service = WorkerService::new(config).expect("worker service");
    let handle = tokio::spawn(async move {
        Server::builder()
            .add_service(HypervisorWorkerServer::new(service))
            .serve_with_incoming(incoming)
            .await
            .expect("worker UDS server");
    });
    WorkerUdsServer {
        _dir: dir,
        uds_path,
        handle,
    }
}

async fn connect_worker_uds(uds_path: PathBuf) -> WorkerClient {
    Endpoint::try_from("http://[::]:0")
        .expect("UDS endpoint URI")
        .connect_with_connector(service_fn(move |_uri: tonic::transport::Uri| {
            let path = uds_path.clone();
            async move {
                let stream = UnixStream::connect(path).await?;
                Ok::<_, std::io::Error>(TokioIo::new(stream))
            }
        }))
        .await
        .map(HypervisorWorkerClient::new)
        .expect("connect worker UDS")
}

fn slot_label(index: usize) -> String {
    if index == usize::MAX {
        "baseline slot".into()
    } else {
        format!("slot {index}")
    }
}

async fn destroy_with_client(
    client: &mut WorkerClient,
    label: &str,
    lease: proto::Lease,
) -> TestResult<()> {
    client
        .destroy_vm(proto::DestroyVmRequest { lease: Some(lease) })
        .await
        .map(|_| ())
        .map_err(|e| format!("{label} DestroyVm: {e}"))
}

async fn error_after_destroy(
    client: &mut WorkerClient,
    label: &str,
    lease: proto::Lease,
    original: String,
) -> String {
    match destroy_with_client(client, label, lease).await {
        Ok(()) => original,
        Err(cleanup) => format!("{original}; cleanup failed: {cleanup}"),
    }
}

async fn destroy_slots_best_effort(
    uds_path: &Path,
    phase: &str,
    slots: &[RestoredSlot],
) -> Vec<String> {
    let mut tasks = Vec::with_capacity(slots.len());
    for slot in slots {
        let uds_path = uds_path.to_path_buf();
        let lease = slot.lease.clone();
        let label = slot_label(slot.index);
        tasks.push(tokio::spawn(async move {
            let mut client = connect_worker_uds(uds_path).await;
            destroy_with_client(&mut client, &label, lease).await.err()
        }));
    }

    let mut errors = Vec::new();
    for task in tasks {
        match task.await {
            Ok(Some(e)) => errors.push(format!("{phase} cleanup: {e}")),
            Ok(None) => {}
            Err(e) => errors.push(format!("{phase} cleanup task: {e}")),
        }
    }
    errors
}

fn merge_phase_errors(phase: &str, mut errors: Vec<String>, cleanup_errors: Vec<String>) -> String {
    errors.extend(cleanup_errors);
    format!("{phase} failed: {}", errors.join("; "))
}

async fn create_base_snapshot(
    client: &mut WorkerClient,
    config: proto::MachineConfig,
) -> TestResult<proto::SnapshotRef> {
    let created = client
        .create_vm(proto::CreateVmRequest {
            config: Some(config),
            entropy_seed: vec![0x4D; 32],
        })
        .await
        .map_err(|e| format!("CreateVm base: {e}"))?
        .into_inner();
    let lease = created
        .lease
        .ok_or_else(|| "CreateVm base returned no lease".to_owned())?;
    let snapshot = match client
        .take_snapshot(proto::TakeSnapshotRequest {
            lease: Some(lease.clone()),
            seal_input_log: Some(true),
            capture: None,
        })
        .await
    {
        Ok(response) => response
            .into_inner()
            .snapshot
            .ok_or_else(|| "TakeSnapshot base returned no snapshot ref".to_owned()),
        Err(e) => {
            return Err(error_after_destroy(
                client,
                "baseline slot",
                lease,
                format!("TakeSnapshot base: {e}"),
            )
            .await);
        }
    };
    let snapshot = match snapshot {
        Ok(snapshot) => snapshot,
        Err(e) => {
            return Err(error_after_destroy(client, "baseline slot", lease, e).await);
        }
    };
    destroy_with_client(client, "baseline slot", lease).await?;
    Ok(snapshot)
}

async fn restore_slot(
    mut client: WorkerClient,
    index: usize,
    snapshot: proto::SnapshotRef,
) -> TestResult<RestoredSlot> {
    let restore = client
        .restore_snapshot(proto::RestoreSnapshotRequest {
            snapshot: Some(snapshot),
            entropy_seed: Vec::new(),
        })
        .await
        .map_err(|e| format!("slot {index} RestoreSnapshot: {e}"))?
        .into_inner();
    let lease = restore
        .lease
        .clone()
        .ok_or_else(|| format!("slot {index} RestoreSnapshot returned no lease"))?;
    Ok(RestoredSlot {
        index,
        lease,
        restore,
    })
}

async fn inject_slot(mut client: WorkerClient, slot: RestoredSlot) -> TestResult<RestoredSlot> {
    let label = slot_label(slot.index);
    let lease = slot.lease.clone();
    let scheduled = match client
        .inject_inputs(proto::InjectInputsRequest {
            lease: Some(slot.lease.clone()),
            events: vec![proto::ScheduledEvent {
                at: Some(proto::scheduled_event::At::AtIcount(INPUT_ICOUNT)),
                event: Some(proto::scheduled_event::Event::PadSet(proto::PadSet {
                    port: 0,
                    buttons: 0x600D,
                })),
            }],
        })
        .await
    {
        Ok(response) => response.into_inner().scheduled,
        Err(e) => {
            return Err(error_after_destroy(
                &mut client,
                &label,
                lease,
                format!("{label} InjectInputs: {e}"),
            )
            .await);
        }
    };
    if scheduled != 1 {
        return Err(error_after_destroy(
            &mut client,
            &label,
            lease,
            format!("{label} InjectInputs scheduled {scheduled}, expected 1"),
        )
        .await);
    }
    Ok(slot)
}

async fn run_snapshot_destroy(
    mut client: WorkerClient,
    slot: RestoredSlot,
) -> TestResult<LegDigest> {
    let label = slot_label(slot.index);
    let lease = slot.lease.clone();
    let run = match client
        .run(proto::RunRequest {
            lease: Some(slot.lease.clone()),
            until: Some(proto::run_request::Until::IcountBudget(RUN_BUDGET)),
            hard_icount_cap: 0,
            capture: None,
        })
        .await
    {
        Ok(response) => response.into_inner(),
        Err(e) => {
            return Err(error_after_destroy(
                &mut client,
                &label,
                lease,
                format!("{label} Run: {e}"),
            )
            .await);
        }
    };
    if run.reason != i32::from(proto::StopReason::GuestHalted) {
        return Err(error_after_destroy(
            &mut client,
            &label,
            lease,
            format!(
                "{label} Run stopped with {}, expected GUEST_HALTED",
                run.reason
            ),
        )
        .await);
    }
    if run.icount < INPUT_ICOUNT {
        return Err(error_after_destroy(
            &mut client,
            &label,
            lease,
            format!(
                "{label} Run stopped before the injected input boundary: {}",
                run.icount
            ),
        )
        .await);
    }

    let snapshot = match client
        .take_snapshot(proto::TakeSnapshotRequest {
            lease: Some(slot.lease.clone()),
            seal_input_log: Some(true),
            capture: Some(capture_spec()),
        })
        .await
    {
        Ok(response) => response.into_inner(),
        Err(e) => {
            return Err(error_after_destroy(
                &mut client,
                &label,
                lease,
                format!("{label} TakeSnapshot: {e}"),
            )
            .await);
        }
    };
    if snapshot.feature_bytes != expected_capture_bytes() {
        return Err(error_after_destroy(
            &mut client,
            &label,
            lease,
            format!("{label} CaptureSpec feature bytes changed"),
        )
        .await);
    }
    if !snapshot.fb_lz4.is_empty() || snapshot.fb_info.is_some() {
        return Err(error_after_destroy(
            &mut client,
            &label,
            lease,
            format!("{label} CaptureSpec framebuffer output should be empty"),
        )
        .await);
    }

    destroy_with_client(&mut client, &label, slot.lease.clone()).await?;

    Ok(digest_leg(&slot.restore, &run, &snapshot))
}

fn digest_leg(
    restore: &proto::RestoreSnapshotResponse,
    run: &proto::RunResponse,
    snapshot: &proto::TakeSnapshotResponse,
) -> LegDigest {
    let mut hasher = blake3::Hasher::new();
    update_u64(&mut hasher, restore.frame_counter.into());
    update_bytes(
        &mut hasher,
        &restore
            .state_hash
            .as_ref()
            .expect("restore state hash")
            .hash,
    );
    update_u64(&mut hasher, run.icount);
    update_u64(&mut hasher, run.vns);
    update_i32(&mut hasher, run.reason);
    update_u64(&mut hasher, run.frames_elapsed);
    update_bytes(
        &mut hasher,
        &run.state_hash.as_ref().expect("run state hash").hash,
    );
    update_bytes(
        &mut hasher,
        &snapshot.snapshot.as_ref().expect("child snapshot ref").hash,
    );
    update_bytes(&mut hasher, &snapshot.input_log_id);
    update_u64(&mut hasher, snapshot.icount);
    update_u64(&mut hasher, snapshot.vns);
    update_bytes(
        &mut hasher,
        &snapshot
            .state_hash
            .as_ref()
            .expect("snapshot state hash")
            .hash,
    );
    update_u64(&mut hasher, snapshot.dirty_pages.into());
    update_bytes(&mut hasher, &snapshot.machine_config_hash);
    update_bytes(&mut hasher, &snapshot.feature_bytes);
    update_bytes(&mut hasher, &snapshot.fb_lz4);
    update_u64(&mut hasher, snapshot.frame_counter.into());
    LegDigest {
        hash: *hasher.finalize().as_bytes(),
    }
}

fn update_bytes(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn update_u64(hasher: &mut blake3::Hasher, value: u64) {
    hasher.update(&value.to_le_bytes());
}

fn update_i32(hasher: &mut blake3::Hasher, value: i32) {
    hasher.update(&value.to_le_bytes());
}

async fn restore_all(
    uds_path: &Path,
    base_snapshot: &proto::SnapshotRef,
) -> TestResult<Vec<RestoredSlot>> {
    let mut tasks = Vec::with_capacity(ACCEPT_SLOTS);
    for index in 0..ACCEPT_SLOTS {
        let uds_path = uds_path.to_path_buf();
        let snapshot = base_snapshot.clone();
        tasks.push(tokio::spawn(async move {
            let client = connect_worker_uds(uds_path).await;
            restore_slot(client, index, snapshot).await
        }));
    }

    let mut slots = Vec::with_capacity(ACCEPT_SLOTS);
    let mut errors = Vec::new();
    for task in tasks {
        match task.await {
            Ok(Ok(slot)) => slots.push(slot),
            Ok(Err(e)) => errors.push(e),
            Err(e) => errors.push(format!("RestoreSnapshot task: {e}")),
        }
    }
    if !errors.is_empty() {
        let cleanup = destroy_slots_best_effort(uds_path, "RestoreSnapshot", &slots).await;
        return Err(merge_phase_errors("RestoreSnapshot", errors, cleanup));
    }
    slots.sort_by_key(|slot| slot.index);
    Ok(slots)
}

async fn inject_all(uds_path: &Path, slots: Vec<RestoredSlot>) -> TestResult<Vec<RestoredSlot>> {
    let mut tasks = Vec::with_capacity(slots.len());
    for slot in slots {
        let uds_path = uds_path.to_path_buf();
        tasks.push(tokio::spawn(async move {
            let client = connect_worker_uds(uds_path).await;
            inject_slot(client, slot).await
        }));
    }

    let mut injected = Vec::with_capacity(ACCEPT_SLOTS);
    let mut errors = Vec::new();
    for task in tasks {
        match task.await {
            Ok(Ok(slot)) => injected.push(slot),
            Ok(Err(e)) => errors.push(e),
            Err(e) => errors.push(format!("InjectInputs task: {e}")),
        }
    }
    if !errors.is_empty() {
        let cleanup = destroy_slots_best_effort(uds_path, "InjectInputs", &injected).await;
        return Err(merge_phase_errors("InjectInputs", errors, cleanup));
    }
    injected.sort_by_key(|slot| slot.index);
    Ok(injected)
}

async fn run_snapshot_destroy_all(
    uds_path: &Path,
    slots: Vec<RestoredSlot>,
) -> TestResult<Vec<LegDigest>> {
    let mut tasks = Vec::with_capacity(slots.len());
    for slot in slots {
        let uds_path = uds_path.to_path_buf();
        tasks.push(tokio::spawn(async move {
            let client = connect_worker_uds(uds_path).await;
            run_snapshot_destroy(client, slot).await
        }));
    }

    let mut digests = Vec::with_capacity(ACCEPT_SLOTS);
    let mut errors = Vec::new();
    for task in tasks {
        match task.await {
            Ok(Ok(digest)) => digests.push(digest),
            Ok(Err(e)) => errors.push(e),
            Err(e) => errors.push(format!("Run/Snapshot task: {e}")),
        }
    }
    if !errors.is_empty() {
        return Err(merge_phase_errors("Run/Snapshot", errors, Vec::new()));
    }
    Ok(digests)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "M6 acceptance gate: requires KVM and 64 dedicated slot cores; run with --release -- --ignored --nocapture"]
async fn m6_full_api_uds_64_concurrent_slots_match_single_slot_baseline() {
    let Some(slot_cores) = acceptance_slot_cores_or_skip() else {
        return;
    };

    let image_cache = tempfile::TempDir::new().expect("image cache");
    let base_hash = write_cache_blob(image_cache.path(), &vec![0u8; 4096]);
    let kernel_hash = write_cache_blob(image_cache.path(), nanokernel::capture_fixture_elf());
    let config = capture_fixture_machine_config(base_hash, kernel_hash);

    let store_dir = tempfile::TempDir::new().expect("snapstore data root");
    let store_sock = "snapstore.sock";
    let (_store_rt, _store_handle, _store_client) =
        common::spawn_store_at(store_dir.path().to_path_buf(), store_sock);
    let snapstore = snapstore_client::Transport::Uds(store_dir.path().join(store_sock));

    let worker = start_worker_uds(worker_config(
        slot_cores,
        image_cache.path().to_path_buf(),
        snapstore,
    ))
    .await;
    let mut control = connect_worker_uds(worker.uds_path.clone()).await;
    let info = control
        .get_worker_info(proto::GetWorkerInfoRequest {})
        .await
        .expect("GetWorkerInfo")
        .into_inner();
    assert_eq!(info.slots_total as usize, ACCEPT_SLOTS);
    assert_eq!(info.slots_free as usize, ACCEPT_SLOTS);

    let base_snapshot = create_base_snapshot(&mut control, config)
        .await
        .expect("base snapshot");

    let baseline_restore = restore_slot(control.clone(), usize::MAX, base_snapshot.clone())
        .await
        .expect("single-slot baseline restore");
    let baseline_slot = inject_slot(control.clone(), baseline_restore)
        .await
        .expect("single-slot baseline inject");
    let baseline = run_snapshot_destroy(control.clone(), baseline_slot)
        .await
        .expect("single-slot baseline leg");

    let slots = restore_all(&worker.uds_path, &base_snapshot)
        .await
        .expect("restore 64 slots");
    let listed = control
        .list_slots(proto::ListSlotsRequest {})
        .await
        .expect("ListSlots after restore")
        .into_inner()
        .slots;
    assert_eq!(listed.len(), ACCEPT_SLOTS);
    assert!(listed
        .iter()
        .all(|slot| slot.state == i32::from(proto::SlotState::PausedS)));
    let slot_ids: BTreeSet<_> = listed.iter().map(|slot| slot.slot_id).collect();
    assert_eq!(slot_ids.len(), ACCEPT_SLOTS);
    assert!(listed.iter().all(|slot| {
        slot.base.as_ref().map(|base| base.hash.as_slice()) == Some(base_snapshot.hash.as_slice())
    }));
    let info = control
        .get_worker_info(proto::GetWorkerInfoRequest {})
        .await
        .expect("GetWorkerInfo after restore")
        .into_inner();
    assert_eq!(info.slots_free, 0);

    let slots = inject_all(&worker.uds_path, slots)
        .await
        .expect("inject all slots");
    let digests = run_snapshot_destroy_all(&worker.uds_path, slots)
        .await
        .expect("run/snapshot/destroy all slots");
    assert_eq!(digests.len(), ACCEPT_SLOTS);
    for (index, digest) in digests.iter().enumerate() {
        assert_eq!(
            digest, &baseline,
            "slot {index} diverged from the single-slot baseline"
        );
    }

    let info = control
        .get_worker_info(proto::GetWorkerInfoRequest {})
        .await
        .expect("GetWorkerInfo after cleanup")
        .into_inner();
    assert_eq!(info.slots_free as usize, ACCEPT_SLOTS);
}
