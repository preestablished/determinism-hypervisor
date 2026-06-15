//! dh-workerd gRPC service shell (bead rfv).
//!
//! This module is the daemon-owned API seam: tonic transport, worker
//! identity, slot table visibility, status-code mapping, and runtime-table
//! ownership. Guest mutating RPCs stay `UNIMPLEMENTED` until each path owns
//! real per-slot KVM, device, counter, DHILOG, and snapshot-store state;
//! returning success before that would fake the M6 acceptance surface.

use crate::proto_map::slot_info_to_proto;
#[cfg(target_arch = "x86_64")]
use crate::runtime::{RuntimeError, SlotRuntime, WorkerRuntimeTable};
use crate::slot_manager::{parse_core_list, Lease, LeasePolicy, SlotError, SlotManager};
use dh_proto::v1 as proto;
use dh_proto::v1::hypervisor_worker_server::{HypervisorWorker, HypervisorWorkerServer};
use prost::Message;
use std::convert::TryFrom;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use tonic::transport::Server;
use tonic::{Code, Request, Response, Status};

pub const DEFAULT_TCP_ADDR: &str = "0.0.0.0:7400";
pub const DEFAULT_UDS_PATH: &str = "/run/dh/grpc.sock";

type ResponseStream<T> =
    Pin<Box<dyn tonic::codegen::tokio_stream::Stream<Item = Result<T, Status>> + Send + 'static>>;

#[derive(Clone, Debug)]
pub struct WorkerConfig {
    pub worker_id: String,
    pub slot_cores: Vec<u32>,
    pub lease_policy: LeasePolicy,
    pub class: proto::DeterminismClass,
}

impl WorkerConfig {
    pub fn from_host_defaults() -> Result<Self, ConfigError> {
        let slot_cores = parse_core_list(crate::preflight::SLOT_CORES)
            .ok_or_else(|| ConfigError::InvalidCoreList(crate::preflight::SLOT_CORES.into()))?;
        Ok(Self {
            worker_id: read_trim("/etc/machine-id").unwrap_or_else(|| "unknown-worker".into()),
            slot_cores,
            lease_policy: LeasePolicy::default(),
            class: detect_determinism_class(),
        })
    }
}

#[derive(Debug)]
pub enum ConfigError {
    InvalidCoreList(String),
    Slot(SlotError),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::InvalidCoreList(spec) => write!(f, "invalid slot core list: {spec}"),
            ConfigError::Slot(e) => write!(f, "slot manager config: {e:?}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<SlotError> for ConfigError {
    fn from(e: SlotError) -> Self {
        ConfigError::Slot(e)
    }
}

#[derive(Debug)]
pub enum ServeError {
    Config(ConfigError),
    Io(std::io::Error),
    Transport(tonic::transport::Error),
}

impl std::fmt::Display for ServeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServeError::Config(e) => write!(f, "{e}"),
            ServeError::Io(e) => write!(f, "{e}"),
            ServeError::Transport(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ServeError {}

impl From<ConfigError> for ServeError {
    fn from(e: ConfigError) -> Self {
        ServeError::Config(e)
    }
}

impl From<std::io::Error> for ServeError {
    fn from(e: std::io::Error) -> Self {
        ServeError::Io(e)
    }
}

impl From<tonic::transport::Error> for ServeError {
    fn from(e: tonic::transport::Error) -> Self {
        ServeError::Transport(e)
    }
}

#[derive(Clone)]
pub struct WorkerService {
    inner: Arc<WorkerInner>,
}

struct WorkerInner {
    manager: Arc<SlotManager>,
    #[cfg(target_arch = "x86_64")]
    runtimes: Arc<WorkerRuntimeTable>,
    worker_id: String,
    class: proto::DeterminismClass,
    version: String,
}

impl WorkerService {
    pub fn new(config: WorkerConfig) -> Result<Self, ConfigError> {
        let slot_count = config.slot_cores.len();
        let manager = Arc::new(SlotManager::new(
            slot_count,
            config.slot_cores,
            config.lease_policy,
        )?);
        Ok(Self {
            inner: Arc::new(WorkerInner {
                manager,
                #[cfg(target_arch = "x86_64")]
                runtimes: Arc::new(WorkerRuntimeTable::new(slot_count)),
                worker_id: config.worker_id,
                class: config.class,
                version: env!("CARGO_PKG_VERSION").into(),
            }),
        })
    }

    pub fn slot_manager(&self) -> Arc<SlotManager> {
        self.inner.manager.clone()
    }

    #[cfg(target_arch = "x86_64")]
    pub fn runtime_table(&self) -> Arc<WorkerRuntimeTable> {
        self.inner.runtimes.clone()
    }

    fn slots_total(&self) -> u32 {
        u32::try_from(self.inner.manager.slot_count()).expect("slot count fits u32")
    }

    fn slots_free(&self) -> u32 {
        let free = self
            .inner
            .manager
            .list()
            .iter()
            .filter(|slot| slot.state == dh_vmm::SlotState::Empty)
            .count();
        u32::try_from(free).expect("slot count fits u32")
    }
}

pub async fn serve(
    config: WorkerConfig,
    tcp_addr: std::net::SocketAddr,
    uds_path: Option<PathBuf>,
) -> Result<(), ServeError> {
    let service = WorkerService::new(config)?;
    let tcp_service = HypervisorWorkerServer::new(service.clone());
    let tcp = Server::builder().add_service(tcp_service).serve(tcp_addr);

    if let Some(uds_path) = uds_path {
        prepare_uds_path(&uds_path)?;
        let listener = tokio::net::UnixListener::bind(&uds_path)?;
        let incoming = tokio_stream::wrappers::UnixListenerStream::new(listener);
        let uds_service = HypervisorWorkerServer::new(service);
        let uds = Server::builder()
            .add_service(uds_service)
            .serve_with_incoming(incoming);
        tokio::try_join!(tcp, uds)?;
    } else {
        tcp.await?;
    }
    Ok(())
}

fn prepare_uds_path(path: &Path) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_socket() => std::fs::remove_file(path),
        Ok(_) => Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("refusing to remove non-socket UDS path {}", path.display()),
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

fn read_trim(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn detect_determinism_class() -> proto::DeterminismClass {
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let cpu = |key: &str| {
        cpuinfo
            .lines()
            .find(|line| line.starts_with(key))
            .and_then(|line| line.split(':').nth(1))
            .map(|value| value.trim().to_string())
            .unwrap_or_else(|| "unknown".into())
    };
    let family = cpu("cpu family");
    let model = cpu("model\t");
    let stepping = cpu("stepping");
    proto::DeterminismClass {
        cpu_model: format!("family={family} model={model} stepping={stepping}"),
        microcode: cpu("microcode"),
        host_kernel: std::process::Command::new("uname")
            .arg("-r")
            .output()
            .ok()
            .and_then(|out| String::from_utf8(out.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unknown".into()),
        vmm_version: env!("CARGO_PKG_VERSION").into(),
    }
}

pub fn lease_from_proto(lease: Option<proto::Lease>) -> Result<Lease, Status> {
    let lease = lease.ok_or_else(|| Status::invalid_argument("missing lease"))?;
    let token: [u8; 16] = lease
        .token
        .try_into()
        .map_err(|_| Status::invalid_argument("lease token must be 16 bytes"))?;
    Ok(Lease {
        slot_id: lease.slot_id,
        token,
    })
}

pub fn slot_error_to_status(e: SlotError) -> Status {
    let detail = proto::ErrorDetail {
        slot_id: slot_error_id(&e).unwrap_or_default(),
        icount: 0,
        code: slot_error_code(&e).into(),
    };
    let message = format!("{e:?}");
    let details = detail.encode_to_vec().into();
    match e {
        SlotError::NoFreeSlot | SlotError::NotEnoughCores { .. } => {
            Status::with_details(Code::ResourceExhausted, message, details)
        }
        SlotError::ZeroChildFork { .. } => {
            Status::with_details(Code::InvalidArgument, message, details)
        }
        SlotError::DuplicateCore { .. } => {
            Status::with_details(Code::InvalidArgument, message, details)
        }
        SlotError::NoSuchSlot(_)
        | SlotError::State(_)
        | SlotError::StaleLease { .. }
        | SlotError::LeaseExpired { .. }
        | SlotError::LiveChildren { .. }
        | SlotError::CowChildCannotFork { .. } => {
            Status::with_details(Code::FailedPrecondition, message, details)
        }
    }
}

fn slot_error_id(e: &SlotError) -> Option<u64> {
    match e {
        SlotError::NoSuchSlot(slot_id)
        | SlotError::StaleLease { slot_id }
        | SlotError::LeaseExpired { slot_id }
        | SlotError::LiveChildren { slot_id, .. }
        | SlotError::CowChildCannotFork { slot_id }
        | SlotError::ZeroChildFork { slot_id } => Some(*slot_id),
        SlotError::State(_)
        | SlotError::NoFreeSlot
        | SlotError::NotEnoughCores { .. }
        | SlotError::DuplicateCore { .. } => None,
    }
}

fn slot_error_code(e: &SlotError) -> &'static str {
    match e {
        SlotError::NoSuchSlot(_) => "no_such_slot",
        SlotError::State(_) => "slot_state",
        SlotError::StaleLease { .. } => "stale_lease",
        SlotError::LeaseExpired { .. } => "lease_expired",
        SlotError::LiveChildren { .. } => "live_children",
        SlotError::CowChildCannotFork { .. } => "cow_child_cannot_fork",
        SlotError::ZeroChildFork { .. } => "zero_child_fork",
        SlotError::NoFreeSlot => "no_free_slot",
        SlotError::NotEnoughCores { .. } => "not_enough_cores",
        SlotError::DuplicateCore { .. } => "duplicate_core",
    }
}

fn unimplemented_status(method: &'static str) -> Status {
    Status::unimplemented(format!(
        "{method} awaits real KVM/store runtime ownership in determinism-hypervisor-rfv"
    ))
}

fn lease_now_ms() -> u64 {
    let Ok(elapsed) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return 0;
    };
    u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(target_arch = "x86_64")]
fn runtime_error_to_status(e: RuntimeError) -> Status {
    let slot_id = match &e {
        RuntimeError::NoSuchSlot(slot_id)
        | RuntimeError::Empty { slot_id }
        | RuntimeError::Occupied { slot_id } => *slot_id,
    };
    let code = match &e {
        RuntimeError::NoSuchSlot(_) => "runtime_no_such_slot",
        RuntimeError::Empty { .. } => "runtime_missing",
        RuntimeError::Occupied { .. } => "runtime_occupied",
    };
    let detail = proto::ErrorDetail {
        slot_id,
        icount: 0,
        code: code.into(),
    };
    Status::with_details(
        Code::FailedPrecondition,
        e.to_string(),
        detail.encode_to_vec().into(),
    )
}

#[cfg(target_arch = "x86_64")]
#[allow(dead_code)] // Wired by determinism-hypervisor-rfv after this runtime-table bead.
fn runtime_position(runtime: &SlotRuntime) -> (u64, Option<[u8; 32]>) {
    (
        runtime.position.cumulative_icount,
        runtime
            .base_snapshot
            .as_ref()
            .map(snapstore_types::SnapshotRef::to_bytes),
    )
}

#[cfg(target_arch = "x86_64")]
#[allow(dead_code)] // Wired by determinism-hypervisor-rfv after this runtime-table bead.
fn rollback_lifecycle_leases(
    method: &'static str,
    manager: &SlotManager,
    runtimes: &WorkerRuntimeTable,
    leases: &[Lease],
    now_ms: u64,
) -> Result<(), Status> {
    let mut removed = Vec::new();
    for lease in leases {
        match runtimes.take(lease.slot_id) {
            Ok(runtime) => removed.push((lease.slot_id, Some(runtime))),
            Err(RuntimeError::Empty { .. }) => removed.push((lease.slot_id, None)),
            Err(e) => {
                return Err(Status::internal(format!(
                    "{method} rollback could not inspect runtime slot {}: {e}",
                    lease.slot_id
                )));
            }
        }
    }

    for (idx, lease) in leases.iter().enumerate() {
        if let Err(e) = manager.destroy(lease, now_ms) {
            let mut restore_errors = Vec::new();
            for (slot_id, runtime) in removed.into_iter().skip(idx) {
                let Some(runtime) = runtime else {
                    continue;
                };
                if let Err(reinsert) = runtimes.insert(slot_id, runtime) {
                    restore_errors.push(format!("slot {slot_id}: {reinsert}"));
                }
            }
            let restore = if restore_errors.is_empty() {
                String::new()
            } else {
                format!("; runtime restore failed: {}", restore_errors.join(", "))
            };
            return Err(Status::internal(format!(
                "{method} rollback could not release slot {}: {e:?}{restore}",
                lease.slot_id
            )));
        }
    }
    Ok(())
}

#[cfg(target_arch = "x86_64")]
async fn blocking_lifecycle<T>(
    method: &'static str,
    f: impl FnOnce() -> Result<T, Status> + Send + 'static,
) -> Result<T, Status>
where
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| Status::internal(format!("{method} blocking worker failed: {e}")))?
}

#[cfg(target_arch = "x86_64")]
impl WorkerService {
    #[allow(dead_code)] // Wired by determinism-hypervisor-rfv after this runtime-table bead.
    pub(crate) async fn install_allocated_runtime(
        &self,
        method: &'static str,
        build_runtime: impl FnOnce(Lease) -> Result<SlotRuntime, Status> + Send + 'static,
    ) -> Result<Lease, Status> {
        let manager = self.inner.manager.clone();
        let runtimes = self.inner.runtimes.clone();
        blocking_lifecycle(method, move || {
            let now_ms = lease_now_ms();
            let lease = manager.allocate(now_ms).map_err(slot_error_to_status)?;
            let runtime = match build_runtime(lease.clone()) {
                Ok(runtime) => runtime,
                Err(e) => {
                    rollback_lifecycle_leases(
                        method,
                        manager.as_ref(),
                        runtimes.as_ref(),
                        std::slice::from_ref(&lease),
                        now_ms,
                    )?;
                    return Err(e);
                }
            };

            let (icount, base_snapshot_id) = runtime_position(&runtime);
            if let Err(e) = runtimes.insert(lease.slot_id, runtime) {
                rollback_lifecycle_leases(
                    method,
                    manager.as_ref(),
                    runtimes.as_ref(),
                    std::slice::from_ref(&lease),
                    now_ms,
                )?;
                return Err(runtime_error_to_status(e));
            }
            if let Err(e) = manager.set_position(&lease, icount, base_snapshot_id, now_ms) {
                rollback_lifecycle_leases(
                    method,
                    manager.as_ref(),
                    runtimes.as_ref(),
                    std::slice::from_ref(&lease),
                    now_ms,
                )?;
                return Err(slot_error_to_status(e));
            }
            Ok(lease)
        })
        .await
    }

    #[allow(dead_code)] // Wired by determinism-hypervisor-rfv after this runtime-table bead.
    pub(crate) async fn install_forked_runtimes(
        &self,
        parent: Lease,
        count: usize,
        build_runtimes: impl FnOnce(&WorkerRuntimeTable, &[Lease]) -> Result<Vec<SlotRuntime>, Status>
            + Send
            + 'static,
    ) -> Result<Vec<Lease>, Status> {
        let manager = self.inner.manager.clone();
        let runtimes = self.inner.runtimes.clone();
        blocking_lifecycle("Fork", move || {
            let now_ms = lease_now_ms();
            runtimes
                .ensure_occupied(parent.slot_id)
                .map_err(runtime_error_to_status)?;
            let child_leases = manager
                .fork(&parent, count, now_ms)
                .map_err(slot_error_to_status)?;

            let child_runtimes = match build_runtimes(runtimes.as_ref(), &child_leases) {
                Ok(child_runtimes) if child_runtimes.len() == child_leases.len() => child_runtimes,
                Ok(child_runtimes) => {
                    rollback_lifecycle_leases(
                        "Fork",
                        manager.as_ref(),
                        runtimes.as_ref(),
                        &child_leases,
                        now_ms,
                    )?;
                    return Err(Status::internal(format!(
                        "Fork built {} child runtimes for {} leases",
                        child_runtimes.len(),
                        child_leases.len()
                    )));
                }
                Err(e) => {
                    rollback_lifecycle_leases(
                        "Fork",
                        manager.as_ref(),
                        runtimes.as_ref(),
                        &child_leases,
                        now_ms,
                    )?;
                    return Err(e);
                }
            };

            let positions: Vec<_> = child_runtimes.iter().map(runtime_position).collect();
            let entries = child_leases
                .iter()
                .map(|lease| lease.slot_id)
                .zip(child_runtimes)
                .collect();
            if let Err(e) = runtimes.insert_many(entries) {
                rollback_lifecycle_leases(
                    "Fork",
                    manager.as_ref(),
                    runtimes.as_ref(),
                    &child_leases,
                    now_ms,
                )?;
                return Err(runtime_error_to_status(e));
            }

            for (lease, (icount, base_snapshot_id)) in child_leases.iter().zip(positions) {
                if let Err(e) = manager.set_position(lease, icount, base_snapshot_id, now_ms) {
                    rollback_lifecycle_leases(
                        "Fork",
                        manager.as_ref(),
                        runtimes.as_ref(),
                        &child_leases,
                        now_ms,
                    )?;
                    return Err(slot_error_to_status(e));
                }
            }
            Ok(child_leases)
        })
        .await
    }

    async fn destroy_runtime_slot(&self, lease: Lease) -> Result<(), Status> {
        let manager = self.inner.manager.clone();
        let runtimes = self.inner.runtimes.clone();
        blocking_lifecycle("DestroyVm", move || {
            let now_ms = lease_now_ms();
            manager
                .check_destroy(&lease, now_ms)
                .map_err(slot_error_to_status)?;
            let runtime = runtimes
                .take(lease.slot_id)
                .map_err(runtime_error_to_status)?;
            if let Err(e) = manager.destroy(&lease, now_ms) {
                if let Err(reinsert) = runtimes.insert(lease.slot_id, runtime) {
                    return Err(Status::internal(format!(
                        "DestroyVm failed after runtime removal: {e:?}; runtime restore failed: {reinsert}"
                    )));
                }
                return Err(slot_error_to_status(e));
            }
            Ok(())
        })
        .await
    }
}

#[tonic::async_trait]
impl HypervisorWorker for WorkerService {
    type StreamGuestEventsStream = ResponseStream<proto::GuestEvent>;
    type VerifyReplayStream = ResponseStream<proto::VerifyReplayProgress>;
    type RunWithFrameCaptureStream = ResponseStream<proto::FrameCaptureEvent>;
    type WatchSlotsStream = ResponseStream<proto::SlotEvent>;

    async fn create_vm(
        &self,
        _request: Request<proto::CreateVmRequest>,
    ) -> Result<Response<proto::CreateVmResponse>, Status> {
        Err(unimplemented_status("CreateVm"))
    }

    async fn restore_snapshot(
        &self,
        _request: Request<proto::RestoreSnapshotRequest>,
    ) -> Result<Response<proto::RestoreSnapshotResponse>, Status> {
        Err(unimplemented_status("RestoreSnapshot"))
    }

    async fn fork(
        &self,
        _request: Request<proto::ForkRequest>,
    ) -> Result<Response<proto::ForkResponse>, Status> {
        Err(unimplemented_status("Fork"))
    }

    async fn destroy_vm(
        &self,
        request: Request<proto::DestroyVmRequest>,
    ) -> Result<Response<proto::DestroyVmResponse>, Status> {
        #[cfg(target_arch = "x86_64")]
        {
            let lease = lease_from_proto(request.into_inner().lease)?;
            self.destroy_runtime_slot(lease).await?;
            return Ok(Response::new(proto::DestroyVmResponse {}));
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let _ = request;
            Err(unimplemented_status("DestroyVm"))
        }
    }

    async fn inject_inputs(
        &self,
        _request: Request<proto::InjectInputsRequest>,
    ) -> Result<Response<proto::InjectInputsResponse>, Status> {
        Err(unimplemented_status("InjectInputs"))
    }

    async fn run(
        &self,
        _request: Request<proto::RunRequest>,
    ) -> Result<Response<proto::RunResponse>, Status> {
        Err(unimplemented_status("Run"))
    }

    async fn pause(
        &self,
        _request: Request<proto::PauseRequest>,
    ) -> Result<Response<proto::PauseResponse>, Status> {
        Err(unimplemented_status("Pause"))
    }

    async fn take_snapshot(
        &self,
        _request: Request<proto::TakeSnapshotRequest>,
    ) -> Result<Response<proto::TakeSnapshotResponse>, Status> {
        Err(unimplemented_status("TakeSnapshot"))
    }

    async fn quiesce(
        &self,
        _request: Request<proto::QuiesceRequest>,
    ) -> Result<Response<proto::QuiesceResponse>, Status> {
        Err(unimplemented_status("Quiesce"))
    }

    async fn read_guest_memory(
        &self,
        _request: Request<proto::ReadGuestMemoryRequest>,
    ) -> Result<Response<proto::ReadGuestMemoryResponse>, Status> {
        Err(unimplemented_status("ReadGuestMemory"))
    }

    async fn get_framebuffer(
        &self,
        _request: Request<proto::GetFramebufferRequest>,
    ) -> Result<Response<proto::GetFramebufferResponse>, Status> {
        Err(unimplemented_status("GetFramebuffer"))
    }

    async fn stream_guest_events(
        &self,
        _request: Request<proto::StreamGuestEventsRequest>,
    ) -> Result<Response<Self::StreamGuestEventsStream>, Status> {
        Err(unimplemented_status("StreamGuestEvents"))
    }

    async fn verify_replay(
        &self,
        _request: Request<proto::VerifyReplayRequest>,
    ) -> Result<Response<Self::VerifyReplayStream>, Status> {
        Err(unimplemented_status("VerifyReplay"))
    }

    async fn run_with_frame_capture(
        &self,
        _request: Request<proto::RunWithFrameCaptureRequest>,
    ) -> Result<Response<Self::RunWithFrameCaptureStream>, Status> {
        Err(unimplemented_status("RunWithFrameCapture"))
    }

    async fn get_worker_info(
        &self,
        _request: Request<proto::GetWorkerInfoRequest>,
    ) -> Result<Response<proto::GetWorkerInfoResponse>, Status> {
        Ok(Response::new(proto::GetWorkerInfoResponse {
            worker_id: self.inner.worker_id.clone(),
            slots_total: self.slots_total(),
            slots_free: self.slots_free(),
            class: Some(self.inner.class.clone()),
            version: self.inner.version.clone(),
        }))
    }

    async fn list_slots(
        &self,
        _request: Request<proto::ListSlotsRequest>,
    ) -> Result<Response<proto::ListSlotsResponse>, Status> {
        Ok(Response::new(proto::ListSlotsResponse {
            slots: self
                .inner
                .manager
                .list()
                .iter()
                .map(slot_info_to_proto)
                .collect(),
        }))
    }

    async fn watch_slots(
        &self,
        _request: Request<proto::WatchSlotsRequest>,
    ) -> Result<Response<Self::WatchSlotsStream>, Status> {
        Err(unimplemented_status("WatchSlots"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_arch = "x86_64")]
    use crate::runtime::{SlotPosition, SlotRuntime};
    use dh_proto::v1::hypervisor_worker_server::HypervisorWorker;

    fn test_config(slots: usize) -> WorkerConfig {
        WorkerConfig {
            worker_id: "test-worker".into(),
            slot_cores: (0..slots)
                .map(|slot| u32::try_from(slot).unwrap())
                .collect(),
            lease_policy: LeasePolicy::default(),
            class: proto::DeterminismClass {
                cpu_model: "test-cpu".into(),
                microcode: "test-ucode".into(),
                host_kernel: "test-kernel".into(),
                vmm_version: "test-vmm".into(),
            },
        }
    }

    #[tokio::test]
    async fn worker_info_reports_slot_capacity() {
        let svc = WorkerService::new(test_config(4)).unwrap();
        let info = svc
            .get_worker_info(Request::new(proto::GetWorkerInfoRequest {}))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(info.worker_id, "test-worker");
        assert_eq!(info.slots_total, 4);
        assert_eq!(info.slots_free, 4);
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.class.unwrap().cpu_model, "test-cpu");
    }

    #[tokio::test]
    async fn list_slots_reflects_slot_manager_state() {
        let svc = WorkerService::new(test_config(2)).unwrap();
        let lease = svc.slot_manager().allocate(0).unwrap();
        let slots = svc
            .list_slots(Request::new(proto::ListSlotsRequest {}))
            .await
            .unwrap()
            .into_inner()
            .slots;
        assert_eq!(slots.len(), 2);
        assert_eq!(
            slots[usize::try_from(lease.slot_id).unwrap()].state,
            i32::from(proto::SlotState::PausedS)
        );
        assert_eq!(
            slots
                .iter()
                .filter(|slot| slot.state == i32::from(proto::SlotState::Empty))
                .count(),
            1
        );
    }

    #[test]
    fn lease_wire_validation_is_strict() {
        assert_eq!(
            lease_from_proto(None).unwrap_err().code(),
            tonic::Code::InvalidArgument
        );
        assert_eq!(
            lease_from_proto(Some(proto::Lease {
                slot_id: 1,
                token: vec![0; 15],
            }))
            .unwrap_err()
            .code(),
            tonic::Code::InvalidArgument
        );
        let lease = lease_from_proto(Some(proto::Lease {
            slot_id: 7,
            token: vec![0xA5; 16],
        }))
        .unwrap();
        assert_eq!(lease.slot_id, 7);
        assert_eq!(lease.token, [0xA5; 16]);
    }

    #[test]
    fn slot_errors_map_to_api_status_classes() {
        assert_eq!(
            slot_error_to_status(SlotError::NoFreeSlot).code(),
            tonic::Code::ResourceExhausted
        );
        assert_eq!(
            slot_error_to_status(SlotError::ZeroChildFork { slot_id: 3 }).code(),
            tonic::Code::InvalidArgument
        );
        assert_eq!(
            slot_error_to_status(SlotError::DuplicateCore { core: 2 }).code(),
            tonic::Code::InvalidArgument
        );
        assert_eq!(
            slot_error_to_status(SlotError::StaleLease { slot_id: 3 }).code(),
            tonic::Code::FailedPrecondition
        );
    }

    #[test]
    fn uds_prepare_removes_only_stale_sockets() {
        let root = std::env::temp_dir().join(format!(
            "dh-worker-uds-test-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("anon")
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("worker.sock");
        std::fs::write(&path, b"not a socket").unwrap();
        let err = prepare_uds_path(&path).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        assert!(path.exists(), "regular file must not be removed");
        std::fs::remove_file(&path).unwrap();

        let target = root.join("target.sock");
        let target_listener = std::os::unix::net::UnixListener::bind(&target).unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        let err = prepare_uds_path(&path).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        assert!(
            std::fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink(),
            "symlink must not be followed and removed"
        );
        std::fs::remove_file(&path).unwrap();
        drop(target_listener);
        std::fs::remove_file(&target).unwrap();

        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        drop(listener);
        prepare_uds_path(&path).unwrap();
        assert!(!path.exists(), "stale socket should be removed");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn mutating_surface_does_not_fake_engine_success() {
        let svc = WorkerService::new(test_config(1)).unwrap();
        let err = svc
            .create_vm(Request::new(proto::CreateVmRequest::default()))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unimplemented);
    }

    #[cfg(target_arch = "x86_64")]
    #[tokio::test]
    async fn destroy_requires_runtime_before_releasing_slot() {
        let svc = WorkerService::new(test_config(1)).unwrap();
        let lease = svc.slot_manager().allocate(0).unwrap();
        let err = svc
            .destroy_vm(Request::new(proto::DestroyVmRequest {
                lease: Some(proto::Lease {
                    slot_id: lease.slot_id,
                    token: lease.token.to_vec(),
                }),
            }))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert_eq!(err.message(), "runtime slot 0 is empty");
        assert_eq!(
            svc.slot_manager().slot_info(lease.slot_id).unwrap().state,
            dh_vmm::SlotState::Paused
        );
        assert_eq!(svc.runtime_table().occupied_count(), 0);
    }

    #[cfg(target_arch = "x86_64")]
    fn runtime_tests_available() -> bool {
        match dh_vmm::kvm::KvmSystem::open() {
            Ok(sys) if sys.dirty_ring => true,
            Ok(_) => {
                eprintln!("skipping runtime service test: KVM dirty ring unavailable");
                false
            }
            Err(e) => {
                eprintln!("skipping runtime service test: KVM unavailable: {e:?}");
                false
            }
        }
    }

    #[cfg(target_arch = "x86_64")]
    fn runtime_test_bus() -> dh_devices::MmioBus {
        let mut bus = dh_devices::MmioBus::new();
        bus.register(
            dh_devices::clock::PV_CLOCK_BASE,
            Box::new(dh_devices::clock::PvClock::new(1, 1)),
        )
        .unwrap();
        bus.register(
            dh_devices::pad::PV_PAD_BASE,
            Box::new(dh_devices::pad::PvPad::new()),
        )
        .unwrap();
        bus.register(
            dh_devices::entropy::PV_ENTROPY_BASE,
            Box::new(dh_devices::entropy::PvEntropy::new()),
        )
        .unwrap();
        bus.register(0xD000_6000, Box::new(dh_devices::DebugSerial::new()))
            .unwrap();
        bus
    }

    #[cfg(target_arch = "x86_64")]
    fn make_runtime(
        seed: u8,
        position: SlotPosition,
        base_snapshot: Option<snapstore_types::SnapshotRef>,
    ) -> Result<SlotRuntime, Status> {
        let sys = dh_vmm::kvm::KvmSystem::open()
            .map_err(|e| Status::internal(format!("open KVM: {e:?}")))?;
        if !sys.dirty_ring {
            return Err(Status::failed_precondition("KVM dirty ring unavailable"));
        }
        let mut config = dh_vmm::config::MachineConfig::new(
            2 * 1024 * 1024,
            [seed; 32],
            dh_vmm::config::BootSpec::Elf {
                kernel_hash: [seed.wrapping_add(1); 32],
                cmdline: Vec::new(),
            },
        );
        config.device_set = vec![
            dh_devices::clock::DEVICE_ID_PV_CLOCK,
            dh_devices::pad::DEVICE_ID_PV_PAD,
            dh_devices::entropy::DEVICE_ID_PV_ENTROPY,
            dh_devices::serial::DEVICE_ID_DEBUG_SERIAL,
        ];
        let config_hash = config
            .config_hash()
            .map_err(|e| Status::internal(format!("config hash: {e:?}")))?;
        let base_ref = base_snapshot
            .as_ref()
            .map(snapstore_types::SnapshotRef::to_bytes)
            .unwrap_or([0; 32]);
        let slot = sys
            .create_slot_vm(config.mem_bytes)
            .map_err(|e| Status::internal(format!("create slot VM: {e:?}")))?;
        SlotRuntime::new(
            slot,
            runtime_test_bus(),
            dh_devices::entropy::DetEntropy::from_seed([seed; 32]),
            config,
            dh_vmm::hash::StateHashChain::new(&config_hash, &base_ref),
            None,
            base_snapshot,
            position,
        )
        .map_err(|e| Status::internal(format!("create slot runtime: {e:?}")))
    }

    #[cfg(target_arch = "x86_64")]
    #[tokio::test]
    async fn allocated_runtime_populates_manager_and_destroy_releases_both_tables() {
        if !runtime_tests_available() {
            return;
        }
        let svc = WorkerService::new(test_config(1)).unwrap();
        let base_snapshot = snapstore_types::SnapshotRef::from_bytes([0x42; 32]);
        let position = SlotPosition {
            cumulative_icount: 1234,
            segment_icount: 17,
            vns: 1234,
            epoch_index: 2,
            frame_counter: 3,
        };
        let lease = svc
            .install_allocated_runtime("CreateVm", move |_| {
                make_runtime(0x11, position, Some(base_snapshot))
            })
            .await
            .unwrap();

        assert_eq!(svc.runtime_table().occupied_count(), 1);
        let slot = svc.slot_manager().slot_info(lease.slot_id).unwrap();
        assert_eq!(slot.state, dh_vmm::SlotState::Paused);
        assert_eq!(slot.icount, position.cumulative_icount);
        assert_eq!(slot.base_snapshot_id, Some([0x42; 32]));

        svc.destroy_vm(Request::new(proto::DestroyVmRequest {
            lease: Some(proto::Lease {
                slot_id: lease.slot_id,
                token: lease.token.to_vec(),
            }),
        }))
        .await
        .unwrap();
        assert_eq!(svc.runtime_table().occupied_count(), 0);
        assert_eq!(
            svc.slot_manager().slot_info(lease.slot_id).unwrap().state,
            dh_vmm::SlotState::Empty
        );
    }

    #[cfg(target_arch = "x86_64")]
    #[tokio::test]
    async fn allocated_runtime_build_failure_rolls_back_manager_lease() {
        let svc = WorkerService::new(test_config(1)).unwrap();
        let err = svc
            .install_allocated_runtime("RestoreSnapshot", |_| {
                Err(Status::internal("restore engine failed"))
            })
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::Internal);
        assert_eq!(err.message(), "restore engine failed");
        assert_eq!(svc.runtime_table().occupied_count(), 0);
        assert_eq!(svc.slot_manager().list()[0].state, dh_vmm::SlotState::Empty);
    }

    #[cfg(target_arch = "x86_64")]
    #[tokio::test]
    async fn fork_runtime_build_failure_rolls_back_children_and_thaws_parent() {
        if !runtime_tests_available() {
            return;
        }
        let svc = WorkerService::new(test_config(3)).unwrap();
        let parent = svc
            .install_allocated_runtime("CreateVm", |_| {
                make_runtime(0x21, SlotPosition::default(), None)
            })
            .await
            .unwrap();

        let err = svc
            .install_forked_runtimes(parent.clone(), 2, |_table, _leases| {
                Err(Status::internal("fork engine failed"))
            })
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::Internal);
        assert_eq!(err.message(), "fork engine failed");
        assert_eq!(svc.runtime_table().occupied_count(), 1);
        let slots = svc.slot_manager().list();
        assert_eq!(
            slots[parent.slot_id as usize].state,
            dh_vmm::SlotState::Paused
        );
        assert_eq!(slots[parent.slot_id as usize].live_children, 0);
        assert_eq!(
            slots
                .iter()
                .filter(|slot| slot.state == dh_vmm::SlotState::Empty)
                .count(),
            2
        );
    }

    #[cfg(target_arch = "x86_64")]
    #[tokio::test]
    async fn fork_runtime_install_populates_children_until_destroy_thaws_parent() {
        if !runtime_tests_available() {
            return;
        }
        let svc = WorkerService::new(test_config(2)).unwrap();
        let parent = svc
            .install_allocated_runtime("CreateVm", |_| {
                make_runtime(0x31, SlotPosition::default(), None)
            })
            .await
            .unwrap();
        let fork_base = snapstore_types::SnapshotRef::from_bytes([0x55; 32]);
        let child_position = SlotPosition {
            cumulative_icount: 9001,
            segment_icount: 0,
            vns: 9001,
            epoch_index: 9,
            frame_counter: 44,
        };

        let children = svc
            .install_forked_runtimes(parent.clone(), 1, move |_table, leases| {
                assert_eq!(leases.len(), 1);
                Ok(vec![make_runtime(0x32, child_position, Some(fork_base))?])
            })
            .await
            .unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(svc.runtime_table().occupied_count(), 2);
        let slots = svc.slot_manager().list();
        assert_eq!(
            slots[parent.slot_id as usize].state,
            dh_vmm::SlotState::Frozen
        );
        assert_eq!(slots[parent.slot_id as usize].live_children, 1);
        let child_info = &slots[children[0].slot_id as usize];
        assert_eq!(child_info.state, dh_vmm::SlotState::Paused);
        assert_eq!(child_info.icount, child_position.cumulative_icount);
        assert_eq!(child_info.base_snapshot_id, Some([0x55; 32]));

        svc.destroy_vm(Request::new(proto::DestroyVmRequest {
            lease: Some(proto::Lease {
                slot_id: children[0].slot_id,
                token: children[0].token.to_vec(),
            }),
        }))
        .await
        .unwrap();
        assert_eq!(svc.runtime_table().occupied_count(), 1);
        assert_eq!(
            svc.slot_manager().slot_info(parent.slot_id).unwrap().state,
            dh_vmm::SlotState::Paused
        );
    }

    #[tokio::test]
    async fn generated_client_reaches_worker_info_and_slots() {
        let svc = WorkerService::new(test_config(2)).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
        let handle = tokio::spawn(async move {
            Server::builder()
                .add_service(HypervisorWorkerServer::new(svc))
                .serve_with_incoming(incoming)
                .await
                .unwrap();
        });

        let endpoint = format!("http://{addr}");
        let mut client = proto::hypervisor_worker_client::HypervisorWorkerClient::connect(endpoint)
            .await
            .unwrap();
        let info = client
            .get_worker_info(proto::GetWorkerInfoRequest {})
            .await
            .unwrap()
            .into_inner();
        assert_eq!(info.worker_id, "test-worker");
        assert_eq!(info.slots_total, 2);

        let slots = client
            .list_slots(proto::ListSlotsRequest {})
            .await
            .unwrap()
            .into_inner()
            .slots;
        assert_eq!(slots.len(), 2);
        assert!(slots
            .iter()
            .all(|slot| slot.state == i32::from(proto::SlotState::Empty)));

        handle.abort();
    }
}
