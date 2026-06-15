//! dh-workerd gRPC service (bead rfv).
//!
//! This module is the daemon-owned API seam: tonic transport, worker
//! identity, slot table visibility, status-code mapping, runtime-table
//! ownership, and the daemon-side resource seams for image-cache and
//! snapshot-store backed lifecycle operations.

#[cfg(target_arch = "x86_64")]
use crate::image_resolver::{ImageResolver, ImageResolverError, ResolvedBoot};
use crate::proto_map::slot_info_to_proto;
#[cfg(target_arch = "x86_64")]
use crate::proto_map::{
    fork_entropy_seeds_from_proto, lease_to_proto, machine_config_from_proto,
    machine_config_to_proto, stop_reason_to_proto,
};
#[cfg(target_arch = "x86_64")]
use crate::runtime::{
    QueuedInput, QueuedInputAt, QueuedInputKind, RuntimeActorError, RuntimeError,
    RuntimeThreadState, SlotActor, SlotRuntime, WorkerRuntimeTable,
};
use crate::slot_manager::{parse_core_list, Lease, LeasePolicy, SlotError, SlotManager};
use dh_proto::v1 as proto;
use dh_proto::v1::hypervisor_worker_server::{HypervisorWorker, HypervisorWorkerServer};
#[cfg(target_arch = "x86_64")]
use dh_verify::verify::VerifyProgress;
use prost::Message;
use std::convert::TryFrom;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
#[cfg(target_arch = "x86_64")]
use std::sync::Mutex;
use tonic::transport::Server;
use tonic::{Code, Request, Response, Status};

#[cfg(target_arch = "x86_64")]
#[derive(Clone)]
struct RuntimeVmMem(vm_memory::GuestMemoryMmap<()>);

#[cfg(target_arch = "x86_64")]
impl dh_devices::ctx::GuestMem for RuntimeVmMem {
    fn read(&self, gpa: u64, out: &mut [u8]) -> Result<(), dh_devices::ctx::MemError> {
        use vm_memory::Bytes;
        self.0
            .read_slice(out, vm_memory::GuestAddress(gpa))
            .map_err(|_| dh_devices::ctx::MemError)
    }

    fn write(&mut self, gpa: u64, data: &[u8]) -> Result<(), dh_devices::ctx::MemError> {
        use vm_memory::Bytes;
        self.0
            .write_slice(data, vm_memory::GuestAddress(gpa))
            .map_err(|_| dh_devices::ctx::MemError)
    }
}

#[cfg(target_arch = "x86_64")]
impl detguest_host::GuestMem for RuntimeVmMem {
    fn read(&self, gpa: u64, out: &mut [u8]) -> Result<(), detguest_host::MemError> {
        if gpa.checked_add(out.len() as u64).is_none() {
            return Err(detguest_host::MemError::Overflow);
        }
        use vm_memory::Bytes;
        self.0
            .read_slice(out, vm_memory::GuestAddress(gpa))
            .map_err(|_| detguest_host::MemError::Unmapped {
                gpa,
                len: out.len(),
            })
    }

    fn write(&mut self, gpa: u64, data: &[u8]) -> Result<(), detguest_host::MemError> {
        if gpa.checked_add(data.len() as u64).is_none() {
            return Err(detguest_host::MemError::Overflow);
        }
        use vm_memory::Bytes;
        self.0
            .write_slice(data, vm_memory::GuestAddress(gpa))
            .map_err(|_| detguest_host::MemError::Unmapped {
                gpa,
                len: data.len(),
            })
    }
}

#[cfg(target_arch = "x86_64")]
type RuntimeDetChannel = dh_devices::detchannel::DetChannelDevice<
    RuntimeVmMem,
    detguest_host::LogFaultPlan,
    fn() -> detguest_host::LogFaultPlan,
>;

#[cfg(target_arch = "x86_64")]
const DETCHANNEL_MMIO_BASE: u64 = 0xD000_3000;
#[cfg(target_arch = "x86_64")]
const MAX_CAPTURE_FEATURE_BYTES: usize = 16 * 1024 * 1024;
#[cfg(target_arch = "x86_64")]
const MAX_CAPTURE_FRAMEBUFFER_BYTES: usize = 64 * 1024 * 1024;

pub const DEFAULT_TCP_ADDR: &str = "0.0.0.0:7400";
pub const DEFAULT_UDS_PATH: &str = "/run/dh/grpc.sock";
#[cfg(target_arch = "x86_64")]
pub const DEFAULT_SNAPSTORE_TCP: &str = "http://127.0.0.1:7410";
#[cfg(target_arch = "x86_64")]
const VERIFY_REPLAY_INLINE_LOG_MAX_BYTES: usize = 4 * 1024 * 1024;

type ResponseStream<T> =
    Pin<Box<dyn tonic::codegen::tokio_stream::Stream<Item = Result<T, Status>> + Send + 'static>>;

#[derive(Clone, Debug)]
pub struct WorkerConfig {
    pub worker_id: String,
    pub slot_cores: Vec<u32>,
    pub lease_policy: LeasePolicy,
    pub class: proto::DeterminismClass,
    #[cfg(target_arch = "x86_64")]
    pub image_cache_dir: PathBuf,
    #[cfg(target_arch = "x86_64")]
    pub snapstore: Option<snapstore_client::Transport>,
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
            #[cfg(target_arch = "x86_64")]
            image_cache_dir: crate::image_resolver::DEFAULT_IMAGE_CACHE_DIR.into(),
            #[cfg(target_arch = "x86_64")]
            snapstore: Some(snapstore_client::Transport::Tcp(
                DEFAULT_SNAPSTORE_TCP.into(),
            )),
        })
    }
}

#[derive(Debug)]
pub enum ConfigError {
    InvalidCoreList(String),
    Slot(SlotError),
    #[cfg(target_arch = "x86_64")]
    Store(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::InvalidCoreList(spec) => write!(f, "invalid slot core list: {spec}"),
            ConfigError::Slot(e) => write!(f, "slot manager config: {e:?}"),
            #[cfg(target_arch = "x86_64")]
            ConfigError::Store(e) => write!(f, "snapstore config: {e}"),
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
    #[cfg(target_arch = "x86_64")]
    image_resolver: ImageResolver,
    #[cfg(target_arch = "x86_64")]
    store: Option<Arc<Mutex<snapstore_client::blocking::SnapstoreClient>>>,
    #[cfg(target_arch = "x86_64")]
    snapstore_transport: Option<snapstore_client::Transport>,
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
        #[cfg(target_arch = "x86_64")]
        let store = config
            .snapstore
            .clone()
            .map(snapstore_client::blocking::SnapstoreClient::connect)
            .transpose()
            .map_err(|e| ConfigError::Store(e.to_string()))?
            .map(|client| Arc::new(Mutex::new(client)));
        Ok(Self {
            inner: Arc::new(WorkerInner {
                manager,
                #[cfg(target_arch = "x86_64")]
                runtimes: Arc::new(WorkerRuntimeTable::new(slot_count)),
                #[cfg(target_arch = "x86_64")]
                image_resolver: ImageResolver::new(config.image_cache_dir),
                #[cfg(target_arch = "x86_64")]
                store,
                #[cfg(target_arch = "x86_64")]
                snapstore_transport: config.snapstore,
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

    #[cfg(target_arch = "x86_64")]
    fn store(&self) -> Result<Arc<Mutex<snapstore_client::blocking::SnapstoreClient>>, Status> {
        self.inner
            .store
            .clone()
            .ok_or_else(|| unavailable_status("snapshot-store"))
    }

    #[cfg(target_arch = "x86_64")]
    fn snapstore_transport(&self) -> Result<snapstore_client::Transport, Status> {
        self.inner
            .snapstore_transport
            .clone()
            .ok_or_else(|| unavailable_status("snapshot-store"))
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

#[cfg(target_arch = "x86_64")]
fn unavailable_status(resource: &'static str) -> Status {
    Status::failed_precondition(format!(
        "{resource} is not configured for this WorkerService"
    ))
}

#[cfg(target_arch = "x86_64")]
fn store_error_to_status(context: &'static str, e: impl std::fmt::Display) -> Status {
    Status::unavailable(format!("{context}: {e}"))
}

#[cfg(target_arch = "x86_64")]
fn image_error_to_status(e: ImageResolverError) -> Status {
    match e {
        ImageResolverError::InvalidConfig(_) => Status::invalid_argument(e.to_string()),
        ImageResolverError::NotFound { .. } | ImageResolverError::NotFile { .. } => {
            Status::failed_precondition(e.to_string())
        }
        ImageResolverError::HashMismatch { .. } => Status::data_loss(e.to_string()),
        ImageResolverError::TooLarge { .. } => Status::invalid_argument(e.to_string()),
        ImageResolverError::Io { .. } => Status::unavailable(e.to_string()),
    }
}

#[cfg(target_arch = "x86_64")]
fn machine_config_error_to_status(e: crate::proto_map::MachineConfigWireError) -> Status {
    Status::invalid_argument(e.to_string())
}

#[cfg(target_arch = "x86_64")]
fn fork_wire_error_to_status(e: crate::proto_map::ForkRequestWireError) -> Status {
    Status::invalid_argument(e.to_string())
}

#[cfg(target_arch = "x86_64")]
fn kvm_error_to_status(context: &'static str, e: dh_vmm::kvm::KvmError) -> Status {
    Status::failed_precondition(format!("{context}: {e:?}"))
}

#[cfg(target_arch = "x86_64")]
fn snapshot_engine_error_to_status(e: crate::snapshot_engine::EngineError) -> Status {
    use crate::snapshot_engine::EngineError;
    match e {
        EngineError::AgendaNotEmpty | EngineError::NotPaused { .. } => {
            Status::failed_precondition(format!("{e:?}"))
        }
        EngineError::Kvm(m) => Status::failed_precondition(m),
        EngineError::Codec(m) => Status::data_loss(m),
        EngineError::Store(m) => Status::unavailable(m),
    }
}

#[cfg(target_arch = "x86_64")]
fn restore_engine_error_to_status(e: crate::restore_engine::RestoreError) -> Status {
    use crate::restore_engine::RestoreError;
    match e {
        RestoreError::NotPaused { .. } | RestoreError::ConfigMismatch(_) => {
            Status::failed_precondition(format!("{e:?}"))
        }
        RestoreError::Kvm(m) => Status::failed_precondition(m),
        RestoreError::Codec(m) => Status::data_loss(m),
        RestoreError::Store(m) => Status::unavailable(m),
    }
}

#[cfg(target_arch = "x86_64")]
fn replay_error_to_status(e: crate::replay_engine::ReplayError) -> Status {
    use crate::replay_engine::ReplayError;
    match e {
        ReplayError::Restore(e) => restore_engine_error_to_status(e),
        ReplayError::Log(e) => Status::data_loss(format!("DHILOG parse: {e:?}")),
        ReplayError::HeaderMismatch(what) => {
            Status::failed_precondition(format!("DHILOG header mismatch: {what}"))
        }
        ReplayError::Divergence { .. } => {
            Status::internal("VerifyReplay divergence escaped report translation")
        }
        ReplayError::NotYetWired(what) => Status::unimplemented(what),
        ReplayError::Apply(m) | ReplayError::Run(m) => Status::data_loss(m),
    }
}

#[cfg(target_arch = "x86_64")]
fn fork_engine_error_to_status(e: crate::fork_engine::ForkError) -> Status {
    use crate::fork_engine::ForkError;
    match e {
        ForkError::AgendaNotEmpty | ForkError::ParentNotFrozen { .. } => {
            Status::failed_precondition(format!("{e:?}"))
        }
        ForkError::Capture(m) | ForkError::Apply(m) => Status::data_loss(m),
        ForkError::Kvm(m) | ForkError::BuildBus(m) => Status::failed_precondition(m),
    }
}

#[cfg(target_arch = "x86_64")]
fn snapshot_ref_from_proto(
    snapshot: Option<proto::SnapshotRef>,
) -> Result<snapstore_types::SnapshotRef, Status> {
    let snapshot = snapshot.ok_or_else(|| Status::invalid_argument("missing snapshot"))?;
    let hash: [u8; 32] = snapshot
        .hash
        .try_into()
        .map_err(|_| Status::invalid_argument("snapshot hash must be 32 bytes"))?;
    Ok(snapstore_types::SnapshotRef::from_bytes(hash))
}

#[cfg(target_arch = "x86_64")]
fn log_id_from_bytes(bytes: Vec<u8>) -> Result<snapstore_types::LogId, Status> {
    let id: [u8; 32] = bytes
        .try_into()
        .map_err(|_| Status::invalid_argument("input_log_id must be 32 bytes"))?;
    Ok(snapstore_types::LogId::from_bytes(id))
}

#[cfg(target_arch = "x86_64")]
enum VerifyReplayLogInput {
    Inline(Vec<u8>),
    Stored(snapstore_types::LogId),
}

#[cfg(target_arch = "x86_64")]
fn verify_replay_log_input(
    log: Option<proto::verify_replay_request::Log>,
) -> Result<VerifyReplayLogInput, Status> {
    use proto::verify_replay_request::Log as WireLog;
    match log.ok_or_else(|| Status::invalid_argument("VerifyReplay.log is required"))? {
        WireLog::InputLog(bytes) => {
            if bytes.len() > VERIFY_REPLAY_INLINE_LOG_MAX_BYTES {
                return Err(Status::invalid_argument(format!(
                    "VerifyReplay.input_log exceeds {} bytes",
                    VERIFY_REPLAY_INLINE_LOG_MAX_BYTES
                )));
            }
            Ok(VerifyReplayLogInput::Inline(bytes))
        }
        WireLog::InputLogId(id) => log_id_from_bytes(id).map(VerifyReplayLogInput::Stored),
    }
}

#[cfg(target_arch = "x86_64")]
fn input_log_payload_from_container(container: &[u8]) -> Result<Vec<u8>, Status> {
    let container = snapstore_manifest::input_log::InputLogContainer::decode(container)
        .map_err(|e| Status::data_loss(format!("input log container decode failed: {e}")))?;
    if container.inner_version() != dh_inputlog::DHILOG_FORMAT_VERSION {
        return Err(Status::failed_precondition(format!(
            "input log inner format version {} != DHILOG {}",
            container.inner_version(),
            dh_inputlog::DHILOG_FORMAT_VERSION
        )));
    }
    Ok(container.payload().to_vec())
}

#[cfg(target_arch = "x86_64")]
fn verify_replay_log_bytes(
    input: VerifyReplayLogInput,
    store: &snapstore_client::blocking::SnapstoreClient,
) -> Result<Vec<u8>, Status> {
    match input {
        VerifyReplayLogInput::Inline(bytes) => Ok(bytes),
        VerifyReplayLogInput::Stored(log_id) => {
            let container = store
                .get_input_log(log_id)
                .map_err(|e| store_error_to_status("get_input_log", e))?;
            input_log_payload_from_container(&container)
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn log_writer_from_reader_header(
    header: &dh_inputlog::reader::Header,
) -> dh_inputlog::dhilog::LogWriter {
    dh_inputlog::dhilog::LogWriter::new(dh_inputlog::dhilog::SegmentHeader {
        base_snapshot_id: header.base_snapshot_id,
        entropy_seed: header.entropy_seed,
        machine_config_hash: header.machine_config_hash,
        clock_num: header.clock_num,
        clock_den: header.clock_den,
        encoder_fingerprint: header.encoder_fingerprint,
    })
}

#[cfg(target_arch = "x86_64")]
fn entropy_seed_from_proto(
    field: &'static str,
    bytes: &[u8],
    allow_empty_continue: bool,
) -> Result<Option<[u8; 32]>, Status> {
    if bytes.is_empty() && allow_empty_continue {
        return Ok(None);
    }
    let seed: [u8; 32] = bytes
        .try_into()
        .map_err(|_| Status::invalid_argument(format!("{field} must be 32 bytes")))?;
    Ok(Some(seed))
}

#[cfg(target_arch = "x86_64")]
fn segment_vns_from_icount(
    config: &dh_vmm::config::MachineConfig,
    segment_icount: u64,
) -> Result<u64, Status> {
    config
        .clock
        .vns_from_icount(segment_icount)
        .ok_or_else(|| Status::failed_precondition("segment vns conversion overflow"))
}

#[cfg(target_arch = "x86_64")]
fn hard_icount_cap(raw: u64) -> u64 {
    if raw == 0 {
        10_000_000_000
    } else {
        raw
    }
}

#[cfg(target_arch = "x86_64")]
fn until_from_run_request(req: &proto::RunRequest) -> Result<dh_vmm::runctl::Until, Status> {
    use proto::run_request::Until as WireUntil;
    match req
        .until
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("RunRequest.until is required"))?
    {
        WireUntil::IcountBudget(budget) => Ok(dh_vmm::runctl::Until::IcountBudget(*budget)),
        WireUntil::VnsBudget(budget) => Ok(dh_vmm::runctl::Until::VnsBudget(*budget)),
        WireUntil::FrameBudget(frames) => Ok(dh_vmm::runctl::Until::FrameBudget {
            frames: u64::from(*frames),
            hard_cap: hard_icount_cap(req.hard_icount_cap),
        }),
        WireUntil::NextSdkEvent(_) => Err(unimplemented_status("Run next_sdk_event")),
        WireUntil::Goal(_) => Err(unimplemented_status("Run goal")),
    }
}

#[cfg(target_arch = "x86_64")]
fn proto_stop_reason(reason: dh_vmm::runctl::StopReason) -> i32 {
    i32::from(stop_reason_to_proto(reason))
}

#[cfg(target_arch = "x86_64")]
fn proto_pixel_format(format: proto::PixelFormat) -> i32 {
    match format {
        proto::PixelFormat::PfUnspecified => 0,
        proto::PixelFormat::Xrgb8888 => 1,
        proto::PixelFormat::Rgb565 => 2,
    }
}

#[cfg(target_arch = "x86_64")]
fn hex32(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(64);
    for byte in bytes {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

#[cfg(target_arch = "x86_64")]
fn verify_log_header_matches_request(
    header: &dh_inputlog::reader::Header,
    base_snapshot: &snapstore_types::SnapshotRef,
    config: &dh_vmm::config::MachineConfig,
) -> Result<(), Status> {
    if header.base_snapshot_id != base_snapshot.to_bytes() {
        return Err(Status::failed_precondition(
            "DHILOG header base_snapshot_id does not match VerifyReplay.base",
        ));
    }
    let config_hash = config
        .config_hash()
        .map_err(|e| Status::invalid_argument(format!("MachineConfig hash: {e:?}")))?;
    if header.machine_config_hash != config_hash {
        return Err(Status::failed_precondition(
            "DHILOG header machine_config_hash does not match base snapshot config",
        ));
    }
    if header.clock_num != config.clock.num() || header.clock_den != config.clock.den() {
        return Err(Status::failed_precondition(
            "DHILOG header clock ratio does not match base snapshot config",
        ));
    }
    Ok(())
}

#[cfg(target_arch = "x86_64")]
fn verify_progress_to_proto(
    progress: VerifyProgress,
    bisect_on_divergence: bool,
) -> Result<proto::VerifyReplayProgress, Status> {
    use proto::verify_replay_progress::Msg;
    let msg = match progress {
        VerifyProgress::EpochOk {
            epoch_index,
            icount,
        } => Msg::EpochOk(proto::EpochOk {
            epoch_index,
            icount,
        }),
        VerifyProgress::Done {
            total_icount,
            end_state_hash,
        } => Msg::Done(proto::VerifyDone {
            total_icount,
            end_state_hash: Some(proto::StateHash {
                hash: end_state_hash.to_vec(),
            }),
        }),
        VerifyProgress::Divergence {
            first_bad_epoch,
            at_icount,
            what,
            expected,
            got,
        } => {
            if bisect_on_divergence {
                return Err(Status::unimplemented(
                    "VerifyReplay divergence bisection is M8 and is not implemented yet; retry without bisection",
                ));
            }
            let first_bad_epoch_value = first_bad_epoch.unwrap_or(0);
            let first_bad_epoch_note = first_bad_epoch
                .map(|epoch| epoch.to_string())
                .unwrap_or_else(|| "none".into());
            Msg::Divergence(proto::Divergence {
                first_bad_epoch: first_bad_epoch_value,
                icount_lo: at_icount,
                icount_hi: at_icount,
                rip_expected: 0,
                rip_actual: 0,
                reg_diff: Vec::new(),
                diff_page_idx: Vec::new(),
                suspected_cause: format!(
                    "coarse:{what}; first_bad_epoch={first_bad_epoch_note}; expected_hash={}; got_hash={}",
                    hex32(&expected),
                    hex32(&got)
                ),
            })
        }
    };
    Ok(proto::VerifyReplayProgress { msg: Some(msg) })
}

#[cfg(target_arch = "x86_64")]
fn run_verify_replay_on_current_thread(
    core: u32,
    base_snapshot: snapstore_types::SnapshotRef,
    log_input: VerifyReplayLogInput,
    transport: snapstore_client::Transport,
    image_resolver: ImageResolver,
    bisect_on_divergence: bool,
) -> Result<Vec<proto::VerifyReplayProgress>, Status> {
    let store = snapstore_client::blocking::SnapstoreClient::connect(transport)
        .map_err(|e| store_error_to_status("connect snapstore", e))?;
    let log_bytes = verify_replay_log_bytes(log_input, &store)?;
    let reader = dh_inputlog::reader::LogReader::parse(&log_bytes)
        .map_err(|e| Status::data_loss(format!("DHILOG parse: {e:?}")))?;
    let header = reader.header().clone();
    let log_writer = log_writer_from_reader_header(&header);
    drop(reader);

    let config = crate::restore_engine::recover_machine_config(base_snapshot.clone(), &store)
        .map_err(restore_engine_error_to_status)?;
    verify_log_header_matches_request(&header, &base_snapshot, &config)?;
    let assets = image_resolver
        .resolve_create_vm(&config)
        .map_err(image_error_to_status)?;
    let sys = dh_vmm::kvm::KvmSystem::open().map_err(|e| kvm_error_to_status("open KVM", e))?;
    if !sys.dirty_ring {
        return Err(Status::failed_precondition("KVM dirty ring unavailable"));
    }
    dh_vmm::run::install_kick_handler()
        .map_err(|e| Status::failed_precondition(format!("install kick handler: {e}")))?;
    dh_vmm::run::pin_current_thread(core).map_err(|e| {
        Status::failed_precondition(format!("pin VerifyReplay to core {core}: {e:?}"))
    })?;
    let _ = dh_vmm::run::set_current_thread_fifo();
    let mut slot = sys
        .create_slot_vm(config.mem_bytes)
        .map_err(|e| kvm_error_to_status("create slot VM", e))?;
    let bus = build_bus(
        &config,
        assets.base_image,
        RuntimeVmMem(slot.guest_mem.clone()),
    )?;
    let rail = dh_vmm::recording::DeviceRail::new(
        bus,
        dh_devices::entropy::DetEntropy::from_seed([0; 32]),
        log_writer,
        RuntimeVmMem(slot.guest_mem.clone()),
    );
    let counter = dh_detclock::counter::InstRetired::open_for_current_thread()
        .map_err(|e| Status::failed_precondition(format!("open InstRetired: {e:?}")))?;
    counter
        .route_overflow_to_thread(dh_vmm::run::current_tid(), dh_vmm::run::kick_signal())
        .map_err(|e| Status::failed_precondition(format!("route InstRetired overflow: {e:?}")))?;
    counter
        .reset()
        .map_err(|e| Status::failed_precondition(format!("reset InstRetired: {e:?}")))?;
    counter
        .arm_period(dh_detclock::counter::NEVER_FIRES_PERIOD)
        .map_err(|e| Status::failed_precondition(format!("arm InstRetired: {e:?}")))?;
    counter
        .enable()
        .map_err(|e| Status::failed_precondition(format!("enable InstRetired: {e:?}")))?;

    let report = crate::verify_replay::verify_replay(
        &mut slot,
        rail,
        &config,
        base_snapshot,
        &counter,
        &store,
        &log_bytes,
    )
    .map_err(replay_error_to_status)?;
    report
        .events
        .into_iter()
        .map(|event| verify_progress_to_proto(event, bisect_on_divergence))
        .collect()
}

#[cfg(target_arch = "x86_64")]
fn queued_input_from_proto(
    index: usize,
    event: &proto::ScheduledEvent,
    current_icount: u64,
    current_frame_counter: u32,
    config: &dh_vmm::config::MachineConfig,
) -> Result<QueuedInput, Status> {
    use proto::scheduled_event::{At as WireAt, Event as WireEvent};

    let (at, frame_hint) = match event
        .at
        .as_ref()
        .ok_or_else(|| Status::invalid_argument(format!("events[{index}].at is required")))?
    {
        WireAt::AtIcount(icount) => (
            QueuedInputAt::Icount(*icount),
            dh_inputlog::dhilog::FRAME_HINT_NONE,
        ),
        WireAt::AtVns(vns) => (
            QueuedInputAt::Icount(config.clock.icount_for_vns_target(*vns).ok_or_else(|| {
                Status::invalid_argument(format!("events[{index}].at_vns overflows"))
            })?),
            dh_inputlog::dhilog::FRAME_HINT_NONE,
        ),
        WireAt::AtFrame(frame) => {
            if *frame == dh_inputlog::dhilog::FRAME_HINT_NONE {
                return Err(Status::invalid_argument(format!(
                    "events[{index}].at_frame value {frame} is reserved"
                )));
            }
            if !machine_has_pv_pad(config) {
                return Err(Status::failed_precondition(format!(
                    "events[{index}].at_frame requires pv-pad in machine_config.device_set"
                )));
            }
            if *frame <= current_frame_counter {
                return Err(Status::invalid_argument(format!(
                    "events[{index}].at_frame must be greater than current frame_counter {current_frame_counter}, got {frame}"
                )));
            }
            (QueuedInputAt::Frame(*frame), *frame)
        }
    };
    if let QueuedInputAt::Icount(icount) = at {
        if icount <= current_icount {
            return Err(Status::invalid_argument(format!(
                "events[{index}] must land after current segment icount {current_icount}, got {icount}"
            )));
        }
    }

    let kind = match event
        .event
        .as_ref()
        .ok_or_else(|| Status::invalid_argument(format!("events[{index}].event is required")))?
    {
        WireEvent::PadSet(pad) => {
            let port = u8::try_from(pad.port).map_err(|_| {
                Status::invalid_argument(format!("events[{index}].pad_set.port must be 0..3"))
            })?;
            if usize::from(port) >= dh_devices::pad::NUM_PORTS {
                return Err(Status::invalid_argument(format!(
                    "events[{index}].pad_set.port must be 0..3"
                )));
            }
            QueuedInputKind::PadSet {
                port,
                buttons: pad.buttons,
                frame_hint,
            }
        }
        WireEvent::NetRx(net) => {
            if net.frame.is_empty() {
                return Err(Status::invalid_argument(format!(
                    "events[{index}].net_rx.frame must not be empty"
                )));
            }
            if net.frame.len() > dh_devices::net::MAX_FRAME as usize {
                return Err(Status::invalid_argument(format!(
                    "events[{index}].net_rx.frame exceeds {} bytes",
                    dh_devices::net::MAX_FRAME
                )));
            }
            QueuedInputKind::NetRx {
                frame: net.frame.clone(),
            }
        }
        WireEvent::DevEvent(dev) => {
            let device_id = u16::try_from(dev.device_id).map_err(|_| {
                Status::invalid_argument(format!(
                    "events[{index}].dev_event.device_id must fit u16"
                ))
            })?;
            let event_type = u16::try_from(dev.event_type).map_err(|_| {
                Status::invalid_argument(format!(
                    "events[{index}].dev_event.event_type must fit u16"
                ))
            })?;
            if dev.payload.len() > dh_inputlog::dhilog::MAX_DEV_EVENT_DATA {
                return Err(Status::invalid_argument(format!(
                    "events[{index}].dev_event.payload exceeds {} bytes",
                    dh_inputlog::dhilog::MAX_DEV_EVENT_DATA
                )));
            }
            QueuedInputKind::DevEvent {
                device_id,
                event_type,
                payload: dev.payload.clone(),
            }
        }
    };

    Ok(QueuedInput { at, order: 0, kind })
}

#[cfg(target_arch = "x86_64")]
fn machine_has_pv_pad(config: &dh_vmm::config::MachineConfig) -> bool {
    config
        .device_set
        .contains(&dh_devices::pad::DEVICE_ID_PV_PAD)
}

#[cfg(target_arch = "x86_64")]
fn frame_scheduled_irq_precondition(
    bus: &mut dh_devices::MmioBus,
    kind: &QueuedInputKind,
) -> Option<&'static str> {
    match kind {
        QueuedInputKind::PadSet { .. } => {
            for (_base, dev) in bus.devices_mut() {
                if dev.device_id() != dh_devices::pad::DEVICE_ID_PV_PAD {
                    continue;
                }
                let pad = dev
                    .as_any_mut()
                    .and_then(|a| a.downcast_mut::<dh_devices::pad::PvPad>())?;
                if pad.irq_vector() != 0 {
                    return Some(
                        "pv-pad IRQ vector is enabled; frame-scheduled PAD_SET IRQ delivery is not wired",
                    );
                }
                return None;
            }
            None
        }
        QueuedInputKind::NetRx { .. } => {
            for (_base, dev) in bus.devices_mut() {
                if dev.device_id() != dh_devices::net::DEVICE_ID_PV_NET {
                    continue;
                }
                let net = dev
                    .as_any_mut()
                    .and_then(|a| a.downcast_mut::<dh_devices::net::PvNet>())?;
                if net.rx_vector() != 0 {
                    return Some(
                        "pv-net RX vector is enabled; frame-scheduled NET_RX IRQ delivery is not wired",
                    );
                }
                return None;
            }
            None
        }
        QueuedInputKind::DevEvent { .. } => None,
    }
}

#[cfg(target_arch = "x86_64")]
fn queue_inputs_from_proto(
    runtime: &mut SlotRuntime,
    events: Vec<proto::ScheduledEvent>,
) -> Result<u32, Status> {
    let scheduled = u32::try_from(events.len())
        .map_err(|_| Status::invalid_argument("too many scheduled events"))?;
    let current_icount = runtime.position.segment_icount;
    let current_frame_counter = runtime.position.frame_counter;
    let mut queued = Vec::with_capacity(events.len());
    for (index, event) in events.iter().enumerate() {
        let mut input = queued_input_from_proto(
            index,
            event,
            current_icount,
            current_frame_counter,
            &runtime.machine_config,
        )?;
        if matches!(input.at, QueuedInputAt::Frame(_)) {
            if let Some(reason) = frame_scheduled_irq_precondition(&mut runtime.bus, &input.kind) {
                return Err(Status::failed_precondition(format!(
                    "events[{index}].at_frame cannot queue an IRQ: {reason}"
                )));
            }
        }
        input.order = runtime.next_input_order;
        runtime.next_input_order = runtime
            .next_input_order
            .checked_add(1)
            .ok_or_else(|| Status::resource_exhausted("scheduled input order exhausted"))?;
        queued.push(input);
    }
    runtime.queued_inputs.extend(queued);
    runtime.queued_inputs.sort_by_key(|input| {
        let (kind, value) = match input.at {
            QueuedInputAt::Icount(icount) => (0u8, icount),
            QueuedInputAt::Frame(frame) => (1u8, u64::from(frame)),
        };
        (kind, value, input.order)
    });
    Ok(scheduled)
}

#[cfg(target_arch = "x86_64")]
fn record_error_to_boundary(e: dh_vmm::recording::RecordError) -> dh_vmm::boundary::BoundaryError {
    dh_vmm::boundary::BoundaryError::Exit(format!("device rail: {e:?}"))
}

#[cfg(target_arch = "x86_64")]
fn apply_queued_input<M: dh_devices::ctx::GuestMem>(
    rail: &mut dh_vmm::recording::DeviceRail<M>,
    input: &QueuedInput,
    boundary: dh_vmm::boundary::Boundary,
) -> Result<Vec<u8>, dh_vmm::boundary::BoundaryError> {
    let vector = match &input.kind {
        QueuedInputKind::PadSet {
            port,
            buttons,
            frame_hint,
        } => rail
            .apply_pad_set(boundary.icount, boundary.rip, *port, *buttons, *frame_hint)
            .map_err(record_error_to_boundary)?,
        QueuedInputKind::NetRx { frame } => rail
            .apply_net_rx(boundary.icount, boundary.rip, frame)
            .map_err(record_error_to_boundary)?,
        QueuedInputKind::DevEvent {
            device_id,
            event_type,
            payload,
        } => rail
            .apply_dev_event(
                boundary.icount,
                boundary.rip,
                *device_id,
                *event_type,
                payload,
            )
            .map_err(record_error_to_boundary)?,
    };
    Ok(vector.into_iter().collect())
}

#[cfg(target_arch = "x86_64")]
fn run_error_to_status(e: dh_vmm::runctl::RunError) -> Status {
    use dh_vmm::runctl::RunError;
    match e {
        RunError::Agenda(_) | RunError::ClockOverflow | RunError::MissingSdkEventFeed => {
            Status::failed_precondition(e.to_string())
        }
        RunError::Boundary(_) | RunError::Inject(_) | RunError::Kvm(_) => {
            Status::data_loss(e.to_string())
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn fault_runtime_after_snapshot_loss(
    manager: &SlotManager,
    runtime: &mut SlotRuntime,
    slot_id: u64,
    context: &'static str,
    status: Status,
) -> Status {
    runtime.thread = RuntimeThreadState::Faulted(format!(
        "{context}: {}: {}",
        status.code(),
        status.message()
    ));
    if let Err(fault) = manager.mark_faulted(slot_id) {
        Status::internal(format!(
            "{context} failed with {}: {}; also failed to mark slot faulted: {fault:?}",
            status.code(),
            status.message()
        ))
    } else {
        status
    }
}

#[cfg(target_arch = "x86_64")]
fn base_snapshot_bytes(base: Option<&snapstore_types::SnapshotRef>) -> [u8; 32] {
    base.map(snapstore_types::SnapshotRef::to_bytes)
        .unwrap_or([0; 32])
}

#[cfg(target_arch = "x86_64")]
fn new_segment_log(
    config: &dh_vmm::config::MachineConfig,
    base_snapshot: Option<&snapstore_types::SnapshotRef>,
    entropy_seed: [u8; 32],
) -> Result<dh_inputlog::dhilog::LogWriter, Status> {
    let machine_config_hash = config
        .config_hash()
        .map_err(|e| Status::invalid_argument(format!("MachineConfig hash: {e:?}")))?;
    Ok(dh_inputlog::dhilog::LogWriter::new(
        dh_inputlog::dhilog::SegmentHeader {
            base_snapshot_id: base_snapshot_bytes(base_snapshot),
            entropy_seed,
            machine_config_hash,
            clock_num: config.clock.num(),
            clock_den: config.clock.den(),
            encoder_fingerprint: 0,
        },
    ))
}

#[cfg(target_arch = "x86_64")]
fn runtime_with_log(
    slot: dh_vmm::kvm::SlotVm,
    bus: dh_devices::MmioBus,
    entropy: dh_devices::entropy::DetEntropy,
    config: dh_vmm::config::MachineConfig,
    chain: dh_vmm::hash::StateHashChain,
    base_snapshot: Option<snapstore_types::SnapshotRef>,
    position: crate::runtime::SlotPosition,
    entropy_seed: [u8; 32],
) -> Result<SlotRuntime, Status> {
    let log = new_segment_log(&config, base_snapshot.as_ref(), entropy_seed)?;
    let mut runtime = SlotRuntime::new(
        slot,
        bus,
        entropy,
        config,
        chain,
        None,
        base_snapshot,
        position,
    )
    .map_err(|e| kvm_error_to_status("create runtime", e))?;
    runtime.log = Some(log);
    Ok(runtime)
}

#[cfg(target_arch = "x86_64")]
fn build_bus(
    config: &dh_vmm::config::MachineConfig,
    base_image: dh_vmm::blkfile::FileBase,
    mem: RuntimeVmMem,
) -> Result<dh_devices::MmioBus, Status> {
    let mut bus = dh_devices::MmioBus::new();
    let mut base_image = Some(base_image);
    for id in &config.device_set {
        match *id {
            dh_devices::clock::DEVICE_ID_PV_CLOCK => bus
                .register(
                    dh_devices::clock::PV_CLOCK_BASE,
                    Box::new(dh_devices::clock::PvClock::new(
                        config.clock.num(),
                        config.clock.den(),
                    )),
                )
                .map_err(|e| Status::internal(format!("register pv-clock: {e:?}")))?,
            dh_devices::pad::DEVICE_ID_PV_PAD => bus
                .register(
                    dh_devices::pad::PV_PAD_BASE,
                    Box::new(dh_devices::pad::PvPad::new()),
                )
                .map_err(|e| Status::internal(format!("register pv-pad: {e:?}")))?,
            dh_devices::entropy::DEVICE_ID_PV_ENTROPY => bus
                .register(
                    dh_devices::entropy::PV_ENTROPY_BASE,
                    Box::new(dh_devices::entropy::PvEntropy::new()),
                )
                .map_err(|e| Status::internal(format!("register pv-entropy: {e:?}")))?,
            dh_devices::blk::DEVICE_ID_PV_BLK => {
                let base = base_image.take().ok_or_else(|| {
                    Status::invalid_argument("device_set contains duplicate pv-blk")
                })?;
                bus.register(
                    0xD000_4000,
                    Box::new(dh_devices::blk::PvBlk::new(Box::new(base))),
                )
                .map_err(|e| Status::internal(format!("register pv-blk: {e:?}")))?;
            }
            dh_devices::serial::DEVICE_ID_DEBUG_SERIAL => bus
                .register(0xD000_6000, Box::new(dh_devices::DebugSerial::new()))
                .map_err(|e| Status::internal(format!("register debug-serial: {e:?}")))?,
            dh_devices::net::DEVICE_ID_PV_NET => bus
                .register(
                    dh_devices::net::PV_NET_BASE,
                    Box::new(dh_devices::net::PvNet::new()),
                )
                .map_err(|e| Status::internal(format!("register pv-net: {e:?}")))?,
            dh_devices::detchannel::DEVICE_ID_DETCHANNEL => bus
                .register(
                    DETCHANNEL_MMIO_BASE,
                    Box::new(RuntimeDetChannel::new(
                        mem.clone(),
                        detguest_host::LogFaultPlan::default(),
                        detguest_host::LogFaultPlan::default,
                    )),
                )
                .map_err(|e| Status::internal(format!("register detchannel: {e:?}")))?,
            other => {
                return Err(Status::failed_precondition(format!(
                    "device id {other:#06x} is not supported by dh-workerd bus builder"
                )));
            }
        }
    }
    Ok(bus)
}

#[cfg(target_arch = "x86_64")]
fn boot_slot(slot: &dh_vmm::kvm::SlotVm, boot: ResolvedBoot) -> Result<(), Status> {
    match boot {
        ResolvedBoot::Elf { kernel, cmdline } => {
            dh_vmm::boot::load_and_enter(slot, &kernel, &cmdline)
                .map(|_| ())
                .map_err(|e| Status::failed_precondition(format!("ELF boot: {e}")))
        }
        ResolvedBoot::BzImage { .. } => Err(Status::unimplemented(
            "BzImage boot is in the wire schema, but dh-vmm currently only boots ELF images",
        )),
    }
}

#[cfg(target_arch = "x86_64")]
fn frame_counter_from_bus(bus: &mut dh_devices::MmioBus) -> u32 {
    for (_base, dev) in bus.devices_mut() {
        if dev.device_id() == dh_devices::pad::DEVICE_ID_PV_PAD {
            if let Some(pad) = dev
                .as_any_mut()
                .and_then(|a| a.downcast_mut::<dh_devices::pad::PvPad>())
            {
                return pad.frame_counter();
            }
        }
    }
    0
}

#[cfg(target_arch = "x86_64")]
fn runtime_detchannel_mut(bus: &mut dh_devices::MmioBus) -> Option<&mut RuntimeDetChannel> {
    bus.devices_mut().find_map(|(_base, dev)| {
        if dev.device_id() != dh_devices::detchannel::DEVICE_ID_DETCHANNEL {
            return None;
        }
        dev.as_any_mut()?.downcast_mut::<RuntimeDetChannel>()
    })
}

#[cfg(target_arch = "x86_64")]
fn service_exit_with_detchannel(
    rail: &mut dh_vmm::recording::DeviceRail<RuntimeVmMem>,
    icount: u64,
    exit: kvm_ioctls::VcpuExit<'_>,
) -> Result<(), dh_vmm::boundary::BoundaryError> {
    let serial_end = dh_vmm::kvm::PIO_SERIAL_BASE + dh_vmm::kvm::PIO_SERIAL_LEN;
    let detcall_end = dh_vmm::kvm::PIO_DETCALL_BASE + dh_vmm::kvm::PIO_DETCALL_LEN;
    let mut ctx = dh_devices::DevCtx::new(
        icount,
        0,
        &mut rail.log,
        &mut rail.mem,
        &mut rail.entropy,
        &mut rail.irqs,
    );

    match exit {
        kvm_ioctls::VcpuExit::IoOut(port, data)
            if (dh_vmm::kvm::PIO_SERIAL_BASE..serial_end).contains(&port) =>
        {
            rail.serial.pio_write(port, data);
        }
        kvm_ioctls::VcpuExit::IoIn(port, data)
            if (dh_vmm::kvm::PIO_SERIAL_BASE..serial_end).contains(&port) =>
        {
            rail.serial.pio_read(port, data);
        }
        kvm_ioctls::VcpuExit::IoOut(port, data)
            if (dh_vmm::kvm::PIO_DETCALL_BASE..detcall_end).contains(&port) =>
        {
            let host = runtime_detchannel_mut(&mut rail.bus).ok_or_else(|| {
                dh_vmm::boundary::BoundaryError::Exit(
                    "detchannel PIO without DetChannelDevice".into(),
                )
            })?;
            let mut word = [0u8; 4];
            let n = data.len().min(4);
            word[..n].copy_from_slice(&data[..n]);
            let _events = host
                .host_mut()
                .pio_out(port, u32::from_le_bytes(word), &mut ctx);
            if host.host().metrics.any_anomaly() {
                return Err(dh_vmm::boundary::BoundaryError::Exit(
                    "detchannel drain anomaly".into(),
                ));
            }
        }
        kvm_ioctls::VcpuExit::IoIn(port, data)
            if (dh_vmm::kvm::PIO_DETCALL_BASE..detcall_end).contains(&port) =>
        {
            let host = runtime_detchannel_mut(&mut rail.bus).ok_or_else(|| {
                dh_vmm::boundary::BoundaryError::Exit(
                    "detchannel PIO without DetChannelDevice".into(),
                )
            })?;
            let value = host.host_mut().pio_in(port, &mut ctx);
            data.fill(0);
            let bytes = value.to_le_bytes();
            let n = data.len().min(4);
            data[..n].copy_from_slice(&bytes[..n]);
            if host.host().metrics.any_anomaly() {
                return Err(dh_vmm::boundary::BoundaryError::Exit(
                    "detchannel drain anomaly".into(),
                ));
            }
        }
        kvm_ioctls::VcpuExit::MmioRead(gpa, data) => {
            rail.bus.read(gpa, data, &mut ctx).map_err(|e| {
                dh_vmm::boundary::BoundaryError::Exit(format!("bus read {gpa:#x}: {e:?}"))
            })?;
        }
        kvm_ioctls::VcpuExit::MmioWrite(gpa, data) => {
            rail.bus.write(gpa, data, &mut ctx).map_err(|e| {
                dh_vmm::boundary::BoundaryError::Exit(format!("bus write {gpa:#x}: {e:?}"))
            })?;
        }
        other => {
            return Err(dh_vmm::boundary::BoundaryError::Exit(format!(
                "unexpected exit: {other:?}"
            )));
        }
    }
    if let Some(e) = ctx.log_fault() {
        return Err(dh_vmm::boundary::BoundaryError::Exit(format!(
            "log fault: {e:?}"
        )));
    }
    Ok(())
}

#[cfg(target_arch = "x86_64")]
#[derive(Default)]
struct CaptureOutput {
    feature_bytes: Vec<u8>,
    fb_lz4: Vec<u8>,
    fb_info: Option<proto::FbInfo>,
}

#[cfg(target_arch = "x86_64")]
fn capture_region_error(region: &str, e: detguest_host::RegionReadError) -> Status {
    match e {
        detguest_host::RegionReadError::NameNotFound => {
            Status::failed_precondition(format!("capture region {region:?} is not published"))
        }
        detguest_host::RegionReadError::OutOfBounds => {
            Status::invalid_argument(format!("capture region {region:?} range is out of bounds"))
        }
        detguest_host::RegionReadError::Wire(e) => {
            Status::failed_precondition(format!("read capture manifest: {e:?}"))
        }
        detguest_host::RegionReadError::Mem(e) => {
            Status::failed_precondition(format!("read capture region {region:?}: {e:?}"))
        }
        _ => Status::failed_precondition(format!("read capture region {region:?}: {e:?}")),
    }
}

#[cfg(target_arch = "x86_64")]
fn checked_capture_len(what: &str, len: u64, max: usize) -> Result<usize, Status> {
    let len = usize::try_from(len)
        .map_err(|_| Status::invalid_argument(format!("{what} is too large")))?;
    if len > max {
        return Err(Status::invalid_argument(format!(
            "{what} is {len} bytes, max {max}"
        )));
    }
    Ok(len)
}

#[cfg(target_arch = "x86_64")]
fn capture_at_boundary(
    bus: &mut dh_devices::MmioBus,
    capture: Option<&proto::CaptureSpec>,
    frame_counter: u32,
) -> Result<CaptureOutput, Status> {
    let Some(capture) = capture else {
        return Ok(CaptureOutput::default());
    };
    if capture.ranges.is_empty() && !capture.framebuffer {
        return Ok(CaptureOutput::default());
    }

    let detchannel = runtime_detchannel_mut(bus).ok_or_else(|| {
        Status::failed_precondition("CaptureSpec requires DetChannelDevice in machine_config")
    })?;
    let channel = detchannel.host().channel().ok_or_else(|| {
        Status::failed_precondition("CaptureSpec requires an attached detchannel")
    })?;
    let manifest = channel
        .read_manifest()
        .map_err(|e| Status::failed_precondition(format!("read capture manifest: {e:?}")))?;
    let feature_len = capture
        .ranges
        .iter()
        .try_fold(0u64, |acc, range| acc.checked_add(u64::from(range.len)))
        .ok_or_else(|| Status::invalid_argument("CaptureSpec ranges are too large"))
        .and_then(|len| {
            checked_capture_len("CaptureSpec feature_bytes", len, MAX_CAPTURE_FEATURE_BYTES)
        })?;
    let mut out = CaptureOutput {
        feature_bytes: Vec::with_capacity(feature_len),
        fb_lz4: Vec::new(),
        fb_info: None,
    };

    for (index, range) in capture.ranges.iter().enumerate() {
        if range.region.is_empty() {
            return Err(Status::invalid_argument(format!(
                "capture.ranges[{index}].region must not be empty"
            )));
        }
        let region = manifest.resolve(&range.region).ok_or_else(|| {
            Status::failed_precondition(format!(
                "capture.ranges[{index}].region {:?} is not published",
                range.region
            ))
        })?;
        if region.layout_version != range.layout_version {
            return Err(Status::failed_precondition(format!(
                "capture.ranges[{index}] layout_version {} != manifest {} for region {:?}",
                range.layout_version, region.layout_version, range.region
            )));
        }
        let end = range
            .offset
            .checked_add(u64::from(range.len))
            .ok_or_else(|| {
                Status::invalid_argument(format!("capture.ranges[{index}] overflows"))
            })?;
        if end > region.len {
            return Err(Status::invalid_argument(format!(
                "capture.ranges[{index}] exceeds region {:?} length {}",
                range.region, region.len
            )));
        }
        let start = out.feature_bytes.len();
        out.feature_bytes.resize(start + range.len as usize, 0);
        channel
            .read_region(&range.region, range.offset, &mut out.feature_bytes[start..])
            .map_err(|e| capture_region_error(&range.region, e))?;
    }

    if capture.framebuffer {
        let entry = manifest
            .entries
            .iter()
            .find(|entry| {
                entry.is_live()
                    && entry.flags & detguest_wire::manifest::REGION_FLAG_FRAMEBUFFER != 0
            })
            .ok_or_else(|| {
                Status::failed_precondition(
                    "CaptureSpec.framebuffer requested but no framebuffer region is published",
                )
            })?;
        let name = std::str::from_utf8(entry.name_bytes()).map_err(|_| {
            Status::failed_precondition("framebuffer region name is not valid UTF-8")
        })?;
        let region = manifest.resolve(name).ok_or_else(|| {
            Status::failed_precondition("framebuffer region could not be resolved")
        })?;
        let fb_len = checked_capture_len(
            "framebuffer region",
            region.len,
            MAX_CAPTURE_FRAMEBUFFER_BYTES,
        )?;
        let mut pixels = vec![0u8; fb_len];
        channel
            .read_region(name, 0, &mut pixels)
            .map_err(|e| capture_region_error(name, e))?;
        out.fb_lz4 = lz4_flex::compress_prepend_size(&pixels);
        out.fb_info = Some(proto::FbInfo {
            width: 0,
            height: 0,
            stride: 0,
            format: proto_pixel_format(proto::PixelFormat::PfUnspecified),
            frame_counter,
        });
    }

    Ok(out)
}

#[cfg(target_arch = "x86_64")]
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
fn runtime_actor_error_to_status(e: RuntimeActorError) -> Status {
    Status::failed_precondition(e.to_string())
}

#[cfg(target_arch = "x86_64")]
fn runtime_core(manager: &SlotManager, slot_id: u64) -> Result<u32, Status> {
    manager
        .core_for(slot_id)
        .ok_or_else(|| Status::failed_precondition(format!("slot {slot_id} has no dedicated core")))
}

#[cfg(target_arch = "x86_64")]
fn start_slot_actor(
    method: &'static str,
    manager: &SlotManager,
    slot_id: u64,
    runtime: SlotRuntime,
) -> Result<Arc<SlotActor>, Status> {
    let core = runtime_core(manager, slot_id)?;
    SlotActor::start(slot_id, core, runtime)
        .map(Arc::new)
        .map_err(|e| Status::failed_precondition(format!("{method}: {e}")))
}

#[cfg(target_arch = "x86_64")]
fn with_runtime<R>(
    runtimes: &WorkerRuntimeTable,
    slot_id: u64,
    f: impl FnOnce(&SlotRuntime) -> R + Send + 'static,
) -> Result<R, Status>
where
    R: Send + 'static,
{
    let actor = runtimes
        .with(slot_id, Arc::clone)
        .map_err(runtime_error_to_status)?;
    actor.with_runtime(f).map_err(runtime_actor_error_to_status)
}

#[cfg(target_arch = "x86_64")]
fn with_runtime_mut<R>(
    runtimes: &WorkerRuntimeTable,
    slot_id: u64,
    f: impl FnOnce(&mut SlotRuntime) -> R + Send + 'static,
) -> Result<R, Status>
where
    R: Send + 'static,
{
    let actor = runtimes
        .with(slot_id, Arc::clone)
        .map_err(runtime_error_to_status)?;
    actor
        .with_runtime_mut(f)
        .map_err(runtime_actor_error_to_status)
}

#[cfg(target_arch = "x86_64")]
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
fn rollback_manager_leases(
    method: &'static str,
    manager: &SlotManager,
    leases: &[Lease],
    now_ms: u64,
) -> Result<(), Status> {
    let mut errors = Vec::new();
    for lease in leases {
        if let Err(e) = manager.destroy(lease, now_ms) {
            errors.push(format!("slot {}: {e:?}", lease.slot_id));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(Status::internal(format!(
            "{method} rollback could not release manager leases: {}",
            errors.join(", ")
        )))
    }
}

#[cfg(target_arch = "x86_64")]
#[allow(dead_code)] // Wired by determinism-hypervisor-rfv after this runtime-table bead.
fn rollback_inserted_lifecycle_leases(
    method: &'static str,
    manager: &SlotManager,
    runtimes: &WorkerRuntimeTable,
    leases: &[Lease],
    inserted_runtime_slots: &[u64],
    now_ms: u64,
) -> Result<(), Status> {
    let mut removed = Vec::new();
    for &slot_id in inserted_runtime_slots {
        match runtimes.take(slot_id) {
            Ok(runtime) => removed.push((slot_id, Some(runtime))),
            Err(RuntimeError::Empty { .. }) => removed.push((slot_id, None)),
            Err(e) => {
                return Err(Status::internal(format!(
                    "{method} rollback could not remove inserted runtime slot {slot_id}: {e}"
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
#[allow(dead_code)] // Wired by determinism-hypervisor-rfv after this runtime-table bead.
fn original_or_rollback(
    method: &'static str,
    original: Status,
    rollback: Result<(), Status>,
) -> Status {
    match rollback {
        Ok(()) => original,
        Err(rollback) => Status::internal(format!(
            "{method} failed with {}: {}; rollback also failed with {}: {}",
            original.code(),
            original.message(),
            rollback.code(),
            rollback.message()
        )),
    }
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
            let allocated_at_ms = lease_now_ms();
            let lease = manager
                .allocate(allocated_at_ms)
                .map_err(slot_error_to_status)?;
            let runtime = match build_runtime(lease.clone()) {
                Ok(runtime) => runtime,
                Err(e) => {
                    let rollback = rollback_manager_leases(
                        method,
                        manager.as_ref(),
                        std::slice::from_ref(&lease),
                        allocated_at_ms,
                    );
                    return Err(original_or_rollback(method, e, rollback));
                }
            };

            let publish_ms = lease_now_ms();
            if let Err(e) = manager.renew(&lease, publish_ms) {
                let rollback = rollback_manager_leases(
                    method,
                    manager.as_ref(),
                    std::slice::from_ref(&lease),
                    allocated_at_ms,
                );
                return Err(original_or_rollback(
                    method,
                    slot_error_to_status(e),
                    rollback,
                ));
            }

            let (icount, base_snapshot_id) = runtime_position(&runtime);
            let actor = match start_slot_actor(method, manager.as_ref(), lease.slot_id, runtime) {
                Ok(actor) => actor,
                Err(e) => {
                    let rollback = rollback_manager_leases(
                        method,
                        manager.as_ref(),
                        std::slice::from_ref(&lease),
                        allocated_at_ms,
                    );
                    return Err(original_or_rollback(method, e, rollback));
                }
            };
            if let Err(e) = runtimes.insert(lease.slot_id, actor) {
                let rollback = rollback_manager_leases(
                    method,
                    manager.as_ref(),
                    std::slice::from_ref(&lease),
                    allocated_at_ms,
                );
                return Err(original_or_rollback(
                    method,
                    runtime_error_to_status(e),
                    rollback,
                ));
            }
            if let Err(e) = manager.set_position(&lease, icount, base_snapshot_id, publish_ms) {
                let rollback = rollback_inserted_lifecycle_leases(
                    method,
                    manager.as_ref(),
                    runtimes.as_ref(),
                    std::slice::from_ref(&lease),
                    &[lease.slot_id],
                    allocated_at_ms,
                );
                return Err(original_or_rollback(
                    method,
                    slot_error_to_status(e),
                    rollback,
                ));
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
        // Contract: the builder may inspect existing runtime state and
        // construct child runtimes, but this helper owns runtime-table
        // publication/removal so SlotManager and WorkerRuntimeTable stay
        // transactionally aligned.
        build_runtimes: impl FnOnce(&WorkerRuntimeTable, &[Lease]) -> Result<Vec<SlotRuntime>, Status>
            + Send
            + 'static,
    ) -> Result<Vec<Lease>, Status> {
        let manager = self.inner.manager.clone();
        let runtimes = self.inner.runtimes.clone();
        blocking_lifecycle("Fork", move || {
            let forked_at_ms = lease_now_ms();
            manager
                .check_fork(&parent, count, forked_at_ms)
                .map_err(slot_error_to_status)?;
            runtimes
                .ensure_occupied(parent.slot_id)
                .map_err(runtime_error_to_status)?;
            let child_leases = manager
                .fork(&parent, count, forked_at_ms)
                .map_err(slot_error_to_status)?;

            let child_runtimes = match build_runtimes(runtimes.as_ref(), &child_leases) {
                Ok(child_runtimes) if child_runtimes.len() == child_leases.len() => child_runtimes,
                Ok(child_runtimes) => {
                    let rollback = rollback_manager_leases(
                        "Fork",
                        manager.as_ref(),
                        &child_leases,
                        forked_at_ms,
                    );
                    let original = Status::internal(format!(
                        "Fork built {} child runtimes for {} leases",
                        child_runtimes.len(),
                        child_leases.len()
                    ));
                    return Err(original_or_rollback("Fork", original, rollback));
                }
                Err(e) => {
                    let rollback = rollback_manager_leases(
                        "Fork",
                        manager.as_ref(),
                        &child_leases,
                        forked_at_ms,
                    );
                    return Err(original_or_rollback("Fork", e, rollback));
                }
            };

            let publish_ms = lease_now_ms();
            if let Err(e) = manager.validate(&parent, publish_ms) {
                let rollback =
                    rollback_manager_leases("Fork", manager.as_ref(), &child_leases, forked_at_ms);
                return Err(original_or_rollback(
                    "Fork",
                    slot_error_to_status(e),
                    rollback,
                ));
            }
            for child in &child_leases {
                if let Err(e) = manager.renew(child, publish_ms) {
                    let rollback = rollback_manager_leases(
                        "Fork",
                        manager.as_ref(),
                        &child_leases,
                        forked_at_ms,
                    );
                    return Err(original_or_rollback(
                        "Fork",
                        slot_error_to_status(e),
                        rollback,
                    ));
                }
            }

            let positions: Vec<_> = child_runtimes.iter().map(runtime_position).collect();
            let mut entries = Vec::with_capacity(child_runtimes.len());
            for (lease, runtime) in child_leases.iter().zip(child_runtimes) {
                let actor = match start_slot_actor("Fork", manager.as_ref(), lease.slot_id, runtime)
                {
                    Ok(actor) => actor,
                    Err(e) => {
                        let rollback = rollback_manager_leases(
                            "Fork",
                            manager.as_ref(),
                            &child_leases,
                            forked_at_ms,
                        );
                        return Err(original_or_rollback("Fork", e, rollback));
                    }
                };
                entries.push((lease.slot_id, actor));
            }
            if let Err(e) = runtimes.insert_many(entries) {
                let rollback =
                    rollback_manager_leases("Fork", manager.as_ref(), &child_leases, forked_at_ms);
                return Err(original_or_rollback(
                    "Fork",
                    runtime_error_to_status(e),
                    rollback,
                ));
            }

            let inserted_slots: Vec<u64> = child_leases.iter().map(|lease| lease.slot_id).collect();
            for (lease, (icount, base_snapshot_id)) in child_leases.iter().zip(positions) {
                if let Err(e) = manager.set_position(lease, icount, base_snapshot_id, publish_ms) {
                    let rollback = rollback_inserted_lifecycle_leases(
                        "Fork",
                        manager.as_ref(),
                        runtimes.as_ref(),
                        &child_leases,
                        &inserted_slots,
                        forked_at_ms,
                    );
                    return Err(original_or_rollback(
                        "Fork",
                        slot_error_to_status(e),
                        rollback,
                    ));
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
            let actor = runtimes
                .take(lease.slot_id)
                .map_err(runtime_error_to_status)?;
            let outstanding = Arc::strong_count(&actor);
            if outstanding != 1 {
                if let Err(reinsert) = runtimes.insert(lease.slot_id, actor) {
                    return Err(Status::internal(format!(
                        "DestroyVm found slot {} actor busy ({outstanding} references); runtime restore failed: {reinsert}",
                        lease.slot_id
                    )));
                }
                return Err(Status::failed_precondition(format!(
                    "DestroyVm cannot stop slot {} actor while {outstanding} references exist",
                    lease.slot_id
                )));
            }
            if let Err(e) = manager.destroy(&lease, now_ms) {
                if let Err(reinsert) = runtimes.insert(lease.slot_id, actor) {
                    return Err(Status::internal(format!(
                        "DestroyVm failed after runtime removal: {e:?}; runtime restore failed: {reinsert}"
                    )));
                }
                return Err(slot_error_to_status(e));
            }
            let actor = Arc::try_unwrap(actor)
                .map_err(|_| Status::internal("DestroyVm actor reference count changed"))?;
            actor.shutdown().map_err(runtime_actor_error_to_status)?;
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
        request: Request<proto::CreateVmRequest>,
    ) -> Result<Response<proto::CreateVmResponse>, Status> {
        #[cfg(target_arch = "x86_64")]
        {
            let request = request.into_inner();
            let config = machine_config_from_proto(
                request
                    .config
                    .as_ref()
                    .ok_or_else(|| Status::invalid_argument("missing config"))?,
            )
            .map_err(machine_config_error_to_status)?;
            let entropy_seed =
                entropy_seed_from_proto("entropy_seed", &request.entropy_seed, false)?
                    .expect("allow_empty_continue=false");
            let image_resolver = self.inner.image_resolver.clone();
            let lease = self
                .install_allocated_runtime("CreateVm", move |_| {
                    let assets = image_resolver
                        .resolve_create_vm(&config)
                        .map_err(image_error_to_status)?;
                    let sys = dh_vmm::kvm::KvmSystem::open()
                        .map_err(|e| kvm_error_to_status("open KVM", e))?;
                    if !sys.dirty_ring {
                        return Err(Status::failed_precondition("KVM dirty ring unavailable"));
                    }
                    let slot = sys
                        .create_slot_vm(config.mem_bytes)
                        .map_err(|e| kvm_error_to_status("create slot VM", e))?;
                    boot_slot(&slot, assets.boot)?;
                    let bus = build_bus(
                        &config,
                        assets.base_image,
                        RuntimeVmMem(slot.guest_mem.clone()),
                    )?;
                    let config_hash = config.config_hash().map_err(|e| {
                        Status::invalid_argument(format!("MachineConfig hash: {e:?}"))
                    })?;
                    runtime_with_log(
                        slot,
                        bus,
                        dh_devices::entropy::DetEntropy::from_seed(entropy_seed),
                        config,
                        dh_vmm::hash::StateHashChain::new(&config_hash, &[0; 32]),
                        None,
                        crate::runtime::SlotPosition::default(),
                        entropy_seed,
                    )
                })
                .await?;
            Ok(Response::new(proto::CreateVmResponse {
                lease: Some(lease_to_proto(&lease)),
                icount: 0,
            }))
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let _ = request;
            Err(unimplemented_status("CreateVm"))
        }
    }

    async fn restore_snapshot(
        &self,
        request: Request<proto::RestoreSnapshotRequest>,
    ) -> Result<Response<proto::RestoreSnapshotResponse>, Status> {
        #[cfg(target_arch = "x86_64")]
        {
            let request = request.into_inner();
            let snapshot_ref = snapshot_ref_from_proto(request.snapshot)?;
            let requested_seed =
                entropy_seed_from_proto("entropy_seed", &request.entropy_seed, true)?;
            if requested_seed == Some([0; 32]) {
                return Err(Status::invalid_argument(
                    "entropy_seed must be non-zero when present; omit it to continue snapshot PRNG",
                ));
            }
            let store = self.store()?;
            let image_resolver = self.inner.image_resolver.clone();
            let lease = self
                .install_allocated_runtime("RestoreSnapshot", move |_| {
                    let config = {
                        let store = store.lock().map_err(|_| {
                            Status::internal("snapshot-store client mutex poisoned")
                        })?;
                        crate::restore_engine::recover_machine_config(snapshot_ref.clone(), &store)
                            .map_err(restore_engine_error_to_status)?
                    };
                    let assets = image_resolver
                        .resolve_create_vm(&config)
                        .map_err(image_error_to_status)?;
                    let sys = dh_vmm::kvm::KvmSystem::open()
                        .map_err(|e| kvm_error_to_status("open KVM", e))?;
                    if !sys.dirty_ring {
                        return Err(Status::failed_precondition("KVM dirty ring unavailable"));
                    }
                    let slot = sys
                        .create_slot_vm(config.mem_bytes)
                        .map_err(|e| kvm_error_to_status("create slot VM", e))?;
                    let mut bus = build_bus(
                        &config,
                        assets.base_image,
                        RuntimeVmMem(slot.guest_mem.clone()),
                    )?;
                    let mut dirty = dh_vmm::dirty::DirtyPageSet::new(slot.mem_bytes);
                    let outcome = {
                        let store = store.lock().map_err(|_| {
                            Status::internal("snapshot-store client mutex poisoned")
                        })?;
                        crate::restore_engine::restore_snapshot(
                            &slot,
                            dh_vmm::SlotState::Paused,
                            &mut bus,
                            &config,
                            snapshot_ref.clone(),
                            None,
                            Some(&mut dirty),
                            &store,
                        )
                        .map_err(restore_engine_error_to_status)?
                    };
                    let entropy = requested_seed
                        .map(dh_devices::entropy::DetEntropy::from_seed)
                        .unwrap_or(outcome.entropy);
                    let frame_counter = frame_counter_from_bus(&mut bus);
                    runtime_with_log(
                        slot,
                        bus,
                        entropy,
                        config,
                        outcome.chain,
                        Some(snapshot_ref),
                        crate::runtime::SlotPosition {
                            cumulative_icount: outcome.cumulative_icount,
                            segment_icount: 0,
                            vns: outcome.vns,
                            epoch_index: outcome.epoch_index,
                            frame_counter,
                        },
                        requested_seed.unwrap_or([0; 32]),
                    )
                })
                .await?;
            let (config, state_hash, frame_counter) =
                with_runtime(self.inner.runtimes.as_ref(), lease.slot_id, |runtime| {
                    (
                        machine_config_to_proto(&runtime.machine_config),
                        runtime.state_hash(),
                        runtime.position.frame_counter,
                    )
                })?;
            Ok(Response::new(proto::RestoreSnapshotResponse {
                lease: Some(lease_to_proto(&lease)),
                config: Some(config),
                state_hash: Some(proto::StateHash {
                    hash: state_hash.to_vec(),
                }),
                frame_counter,
            }))
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let _ = request;
            Err(unimplemented_status("RestoreSnapshot"))
        }
    }

    async fn fork(
        &self,
        request: Request<proto::ForkRequest>,
    ) -> Result<Response<proto::ForkResponse>, Status> {
        #[cfg(target_arch = "x86_64")]
        {
            let request = request.into_inner();
            let parent = lease_from_proto(request.parent)?;
            let count = usize::try_from(request.count)
                .map_err(|_| Status::invalid_argument("count does not fit usize"))?;
            let entropy_seeds =
                fork_entropy_seeds_from_proto(request.count, &request.entropy_seeds)
                    .map_err(fork_wire_error_to_status)?;
            let image_resolver = self.inner.image_resolver.clone();
            let child_leases = self
                .install_forked_runtimes(parent.clone(), count, move |table, _leases| {
                    with_runtime_mut(table, parent.slot_id, move |parent_runtime| {
                            parent_runtime
                                .slot
                                .freeze_ram()
                                .map_err(|e| kvm_error_to_status("freeze parent RAM", e))?;
                            let sys = dh_vmm::kvm::KvmSystem::open()
                                .map_err(|e| kvm_error_to_status("open KVM", e))?;
                            if parent_runtime.position.segment_icount != 0 {
                                return Err(Status::failed_precondition(
                                    "Fork requires the parent at its segment base; take a snapshot before forking a dirty segment",
                                ));
                            }
                            let parent_base = parent_runtime.base_snapshot.clone();
                            let parent_boundary =
                                parent_runtime.boundary_state(parent_runtime.queued_inputs.is_empty());
                            let mut out = Vec::with_capacity(entropy_seeds.len());
                            for seed in entropy_seeds {
                                let assets = image_resolver
                                    .resolve_create_vm(&parent_runtime.machine_config)
                                    .map_err(image_error_to_status)?;
                                let (forked, child_bus) = crate::fork_engine::fork_slot_with_child_bus(
                                    &sys,
                                    &parent_runtime.slot,
                                    dh_vmm::SlotState::Frozen,
                                    &parent_runtime.bus,
                                    &parent_runtime.entropy,
                                    &parent_runtime.machine_config,
                                    parent_boundary,
                                    seed,
                                    None,
                                    |child| {
                                        build_bus(
                                            &parent_runtime.machine_config,
                                            assets.base_image,
                                            RuntimeVmMem(child.guest_mem.clone()),
                                        )
                                        .map_err(|e| format!("{}: {}", e.code(), e.message()))
                                    },
                                )
                                .map_err(fork_engine_error_to_status)?;
                                out.push(runtime_with_log(
                                    forked.child,
                                    child_bus,
                                    forked.entropy,
                                    parent_runtime.machine_config.clone(),
                                    forked.chain,
                                    parent_base.clone(),
                                    crate::runtime::SlotPosition {
                                        cumulative_icount: forked.cumulative_icount,
                                        segment_icount: 0,
                                        vns: forked.vns,
                                        epoch_index: forked.epoch_index,
                                        frame_counter: parent_runtime.position.frame_counter,
                                    },
                                    seed.unwrap_or([0; 32]),
                                )?);
                            }
                            Ok(out)
                        })?
                })
                .await?;
            Ok(Response::new(proto::ForkResponse {
                children: child_leases.iter().map(lease_to_proto).collect(),
            }))
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let _ = request;
            Err(unimplemented_status("Fork"))
        }
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
        request: Request<proto::InjectInputsRequest>,
    ) -> Result<Response<proto::InjectInputsResponse>, Status> {
        #[cfg(target_arch = "x86_64")]
        {
            let request = request.into_inner();
            let lease = lease_from_proto(request.lease)?;
            let manager = self.inner.manager.clone();
            let runtimes = self.inner.runtimes.clone();
            let scheduled = blocking_lifecycle("InjectInputs", move || {
                manager
                    .checkout_write(&lease, "InjectInputs", lease_now_ms())
                    .map_err(slot_error_to_status)?;
                with_runtime_mut(runtimes.as_ref(), lease.slot_id, move |runtime| {
                    queue_inputs_from_proto(runtime, request.events)
                })?
            })
            .await?;
            Ok(Response::new(proto::InjectInputsResponse { scheduled }))
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let _ = request;
            Err(unimplemented_status("InjectInputs"))
        }
    }

    async fn run(
        &self,
        request: Request<proto::RunRequest>,
    ) -> Result<Response<proto::RunResponse>, Status> {
        #[cfg(target_arch = "x86_64")]
        {
            let request = request.into_inner();
            let capture = request.capture.clone();
            let lease = lease_from_proto(request.lease.clone())?;
            let until = until_from_run_request(&request)?;
            let manager = self.inner.manager.clone();
            let runtimes = self.inner.runtimes.clone();
            let response = blocking_lifecycle("Run", move || {
                manager
                    .checkout_write(&lease, "Run", lease_now_ms())
                    .map_err(slot_error_to_status)?;
                with_runtime_mut(runtimes.as_ref(), lease.slot_id, move |runtime| {
                    let tid = dh_vmm::run::current_tid();
                    let start_segment_icount = runtime.position.segment_icount;
                    let start_cumulative_icount = runtime.position.cumulative_icount;
                    let start_vns = runtime.position.vns;
                    let start_segment_vns =
                        segment_vns_from_icount(&runtime.machine_config, start_segment_icount)?;
                    let epoch_len = runtime.machine_config.epoch_len.max(1);
                    let start_segment_epoch = start_segment_icount / epoch_len;
                    manager
                        .mark_running(&lease, lease_now_ms())
                        .map_err(slot_error_to_status)?;
                    runtime.thread = RuntimeThreadState::Running { tid };
                    runtime.clear_pause_request();
                    let pause = runtime.pause_flag();
                    let counter = runtime.counter.as_ref().ok_or_else(|| {
                        Status::failed_precondition("slot actor has no InstRetired counter")
                    })?;

                    let mut goal = || false;
                    let log = runtime.log.take().ok_or_else(|| {
                        Status::failed_precondition("slot has no active DHILOG segment")
                    })?;
                    let bus = std::mem::take(&mut runtime.bus);
                    let entropy = std::mem::replace(
                        &mut runtime.entropy,
                        dh_devices::entropy::DetEntropy::from_seed([0; 32]),
                    );
                    let pending_inputs = runtime.queued_inputs.clone();
                    let scheduled_input_icounts: Vec<u64> = pending_inputs
                        .iter()
                        .map(|input| match input.at {
                            QueuedInputAt::Icount(icount) => icount,
                            QueuedInputAt::Frame(_) => start_segment_icount,
                        })
                        .collect();
                    let scheduled_frame_inputs: Vec<_> = pending_inputs
                        .iter()
                        .enumerate()
                        .filter_map(|(index, input)| match input.at {
                            QueuedInputAt::Frame(frame) => {
                                Some(dh_vmm::runctl::ScheduledFrameInput { frame, index })
                            }
                            QueuedInputAt::Icount(_) => None,
                        })
                        .collect();
                    let (run_result, consumed_input_orders, rail) = {
                        let rail = std::cell::RefCell::new(dh_vmm::recording::DeviceRail::new(
                            bus,
                            entropy,
                            log,
                            RuntimeVmMem(runtime.slot.guest_mem.clone()),
                        ));
                        let mut consumed_input_orders = Vec::new();
                        let counter_ref = counter;
                        let mut on_exit = |exit: kvm_ioctls::VcpuExit<'_>| {
                            let icount = counter_ref.read().map_err(|e| {
                                dh_vmm::boundary::BoundaryError::Exit(format!(
                                    "counter read: {e:?}"
                                ))
                            })?;
                            service_exit_with_detchannel(&mut rail.borrow_mut(), icount, exit)
                        };
                        let mut input_sink = |idx: usize, boundary| {
                            let input = pending_inputs.get(idx).ok_or_else(|| {
                                dh_vmm::boundary::BoundaryError::Exit(format!(
                                    "scheduled input index {idx} out of range"
                                ))
                            })?;
                            let vectors =
                                apply_queued_input(&mut *rail.borrow_mut(), input, boundary)?;
                            consumed_input_orders.push(input.order);
                            Ok(vectors)
                        };
                        let run_result = {
                            let mut segment = dh_vmm::runctl::Segment {
                                slot: &mut runtime.slot,
                                counter,
                                chain: &mut runtime.chain,
                                config: &runtime.machine_config,
                                start_icount: start_segment_icount,
                                injections: &[],
                                timer: None,
                                pause: pause.as_ref(),
                                sdk_events: None,
                            };
                            dh_vmm::runctl::run_segment_with_scheduled_inputs_and_frames(
                                &mut segment,
                                until,
                                &scheduled_input_icounts,
                                &scheduled_frame_inputs,
                                runtime.position.frame_counter,
                                &mut goal,
                                &mut on_exit,
                                &mut input_sink,
                            )
                        };
                        (run_result, consumed_input_orders, rail.into_inner())
                    };
                    runtime.bus = rail.bus;
                    runtime.entropy = rail.entropy;
                    runtime.log = Some(rail.log);

                    match run_result {
                        Ok(outcome) => {
                            runtime.thread = RuntimeThreadState::Parked;
                            runtime.clear_pause_request();
                            let segment_delta =
                                outcome.boundary.icount.saturating_sub(start_segment_icount);
                            let vns_delta = outcome.vns.saturating_sub(start_segment_vns);
                            let segment_epoch = outcome.boundary.icount / epoch_len;
                            let epoch_delta = segment_epoch.saturating_sub(start_segment_epoch);
                            let cumulative_icount =
                                start_cumulative_icount.saturating_add(segment_delta);
                            let cumulative_vns = start_vns.saturating_add(vns_delta);
                            let cumulative_epoch =
                                runtime.position.epoch_index.saturating_add(epoch_delta);
                            runtime.set_boundary(
                                cumulative_icount,
                                outcome.boundary.icount,
                                cumulative_vns,
                                cumulative_epoch,
                                runtime.chain.clone(),
                            );
                            runtime.position.frame_counter =
                                frame_counter_from_bus(&mut runtime.bus);
                            if !consumed_input_orders.is_empty() {
                                runtime
                                    .queued_inputs
                                    .retain(|input| !consumed_input_orders.contains(&input.order));
                            }
                            manager
                                .mark_paused(&lease, lease_now_ms())
                                .map_err(slot_error_to_status)?;
                            manager
                                .set_position(
                                    &lease,
                                    cumulative_icount,
                                    runtime
                                        .base_snapshot
                                        .as_ref()
                                        .map(snapstore_types::SnapshotRef::to_bytes),
                                    lease_now_ms(),
                                )
                                .map_err(slot_error_to_status)?;
                            let capture = capture_at_boundary(
                                &mut runtime.bus,
                                capture.as_ref(),
                                runtime.position.frame_counter,
                            )?;
                            Ok(proto::RunResponse {
                                reason: proto_stop_reason(outcome.reason),
                                icount: cumulative_icount,
                                vns: cumulative_vns,
                                state_hash: Some(proto::StateHash {
                                    hash: outcome.state_hash.to_vec(),
                                }),
                                frames_elapsed: outcome.frames_elapsed,
                                sdk_event: None,
                                feature_bytes: capture.feature_bytes,
                                fb_lz4: capture.fb_lz4,
                                fb_info: capture.fb_info,
                            })
                        }
                        Err(e) => {
                            runtime.thread = RuntimeThreadState::Faulted(e.to_string());
                            let _ = manager.mark_faulted(lease.slot_id);
                            Err(run_error_to_status(e))
                        }
                    }
                })?
            })
            .await?;
            Ok(Response::new(response))
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let _ = request;
            Err(unimplemented_status("Run"))
        }
    }

    async fn pause(
        &self,
        request: Request<proto::PauseRequest>,
    ) -> Result<Response<proto::PauseResponse>, Status> {
        #[cfg(target_arch = "x86_64")]
        {
            let lease = lease_from_proto(request.into_inner().lease)?;
            let manager = self.inner.manager.clone();
            let runtimes = self.inner.runtimes.clone();
            let response = blocking_lifecycle("Pause", move || {
                manager
                    .validate(&lease, lease_now_ms())
                    .map_err(slot_error_to_status)?;
                let state = manager
                    .slot_info(lease.slot_id)
                    .map_err(slot_error_to_status)?
                    .state;
                if !matches!(
                    state,
                    dh_vmm::SlotState::Paused | dh_vmm::SlotState::Running
                ) {
                    return Err(Status::failed_precondition(format!(
                        "Pause requires Paused or Running slot, got {state:?}"
                    )));
                }
                let actor = runtimes
                    .with(lease.slot_id, Arc::clone)
                    .map_err(runtime_error_to_status)?;
                actor.request_pause();
                actor
                    .with_runtime_mut(|runtime| {
                        runtime.clear_pause_request();
                        if matches!(runtime.thread, RuntimeThreadState::PauseRequested { .. }) {
                            runtime.thread = RuntimeThreadState::Parked;
                        }
                        proto::PauseResponse {
                            icount: runtime.position.cumulative_icount,
                            vns: runtime.position.vns,
                            state_hash: Some(proto::StateHash {
                                hash: runtime.state_hash().to_vec(),
                            }),
                        }
                    })
                    .map_err(runtime_actor_error_to_status)
            })
            .await?;
            Ok(Response::new(response))
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let _ = request;
            Err(unimplemented_status("Pause"))
        }
    }

    async fn take_snapshot(
        &self,
        request: Request<proto::TakeSnapshotRequest>,
    ) -> Result<Response<proto::TakeSnapshotResponse>, Status> {
        #[cfg(target_arch = "x86_64")]
        {
            let request = request.into_inner();
            let capture = request.capture.clone();
            let lease = lease_from_proto(request.lease)?;
            let seal_input_log = request.seal_input_log.unwrap_or(true);
            let store = self.store()?;
            let manager = self.inner.manager.clone();
            let runtimes = self.inner.runtimes.clone();
            let class = self.inner.class.clone();
            let snapshot = blocking_lifecycle("TakeSnapshot", move || {
                let now_ms = lease_now_ms();
                manager
                    .validate(&lease, now_ms)
                    .map_err(slot_error_to_status)?;
                let slot_state = manager
                    .slot_info(lease.slot_id)
                    .map_err(slot_error_to_status)?
                    .state;
                with_runtime_mut(runtimes.as_ref(), lease.slot_id, move |runtime| {
                    let boundary = runtime.boundary_state(runtime.queued_inputs.is_empty());
                    let segment_icount = runtime.position.segment_icount;
                    let segment_vns =
                        segment_vns_from_icount(&runtime.machine_config, segment_icount)?;
                    let frame_counter = frame_counter_from_bus(&mut runtime.bus);
                    runtime.position.frame_counter = frame_counter;
                    let capture =
                        capture_at_boundary(&mut runtime.bus, capture.as_ref(), frame_counter)?;
                    let store = store
                        .lock()
                        .map_err(|_| Status::internal("snapshot-store client mutex poisoned"))?;
                    let machine_config_hash = runtime
                        .machine_config
                        .config_hash()
                        .map_err(|e| Status::internal(format!("MachineConfig hash: {e:?}")))?;
                    let source = match runtime.base_snapshot.clone() {
                        Some(parent) => crate::snapshot_engine::PageSource::Incremental {
                            parent,
                            ring: &mut runtime.dirty_ring,
                            dirty: &mut runtime.dirty,
                        },
                        None => crate::snapshot_engine::PageSource::Full,
                    };
                    let out = crate::snapshot_engine::take_snapshot(
                        &runtime.slot,
                        slot_state,
                        &runtime.bus,
                        &runtime.entropy,
                        &runtime.machine_config,
                        boundary,
                        source,
                        &store,
                    )
                    .map_err(snapshot_engine_error_to_status)?;
                    let input_log_id = if seal_input_log {
                        match (|| {
                            let log = runtime.log.take().ok_or_else(|| {
                                Status::failed_precondition("no active DHILOG segment to seal")
                            })?;
                            let log_bytes = log
                                .seal(dh_inputlog::dhilog::SealParams {
                                    end_snapshot_id: out.snapshot_ref.to_bytes(),
                                    end_icount: segment_icount,
                                    end_vns: segment_vns,
                                    end_state_hash: out.hash_chain,
                                    stop_reason: dh_vmm::recording::stop_reason_u8(
                                        dh_vmm::runctl::StopReason::BudgetReached,
                                    ),
                                })
                                .map_err(|e| Status::data_loss(format!("seal DHILOG: {e:?}")))?;
                            let log_container =
                                snapstore_client::helpers::build_input_log_container(
                                    dh_inputlog::DHILOG_FORMAT_VERSION,
                                    &log_bytes,
                                );
                            let (log_id, _deduped) = store
                                .put_input_log(log_container)
                                .map_err(|e| store_error_to_status("put_input_log", e))?;
                            Ok::<_, Status>(log_id.to_bytes().to_vec())
                        })() {
                            Ok(log_id) => log_id,
                            Err(e) => {
                                return Err(fault_runtime_after_snapshot_loss(
                                    manager.as_ref(),
                                    runtime,
                                    lease.slot_id,
                                    "TakeSnapshot lost active DHILOG",
                                    e,
                                ));
                            }
                        }
                    } else {
                        Vec::new()
                    };
                    let next_log =
                        new_segment_log(&runtime.machine_config, Some(&out.snapshot_ref), [0; 32])
                            .map_err(|e| {
                                fault_runtime_after_snapshot_loss(
                                    manager.as_ref(),
                                    runtime,
                                    lease.slot_id,
                                    "TakeSnapshot could not open next DHILOG segment",
                                    e,
                                )
                            })?;
                    if let Err(e) = manager
                        .set_position(
                            &lease,
                            boundary.icount,
                            Some(out.snapshot_ref.to_bytes()),
                            lease_now_ms(),
                        )
                        .map_err(slot_error_to_status)
                    {
                        return Err(fault_runtime_after_snapshot_loss(
                            manager.as_ref(),
                            runtime,
                            lease.slot_id,
                            "TakeSnapshot could not publish snapshot position",
                            e,
                        ));
                    }
                    runtime.base_snapshot = Some(out.snapshot_ref.clone());
                    runtime.log = Some(next_log);
                    runtime.position.segment_icount = 0;
                    Ok((
                        out,
                        machine_config_hash,
                        input_log_id,
                        frame_counter,
                        boundary.icount,
                        boundary.vns,
                        capture,
                    ))
                })
            })
            .await??;
            let (out, machine_config_hash, input_log_id, frame_counter, icount, vns, capture) =
                snapshot;
            Ok(Response::new(proto::TakeSnapshotResponse {
                snapshot: Some(proto::SnapshotRef {
                    hash: out.snapshot_ref.to_bytes().to_vec(),
                }),
                input_log_id,
                icount,
                vns,
                state_hash: Some(proto::StateHash {
                    hash: out.hash_chain.to_vec(),
                }),
                dirty_pages: u32::try_from(out.pages_shipped).unwrap_or(u32::MAX),
                machine_config_hash: machine_config_hash.to_vec(),
                determinism_class: Some(class),
                feature_bytes: capture.feature_bytes,
                fb_lz4: capture.fb_lz4,
                fb_info: capture.fb_info,
                frame_counter,
            }))
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let _ = request;
            Err(unimplemented_status("TakeSnapshot"))
        }
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
        request: Request<proto::VerifyReplayRequest>,
    ) -> Result<Response<Self::VerifyReplayStream>, Status> {
        #[cfg(target_arch = "x86_64")]
        {
            let request = request.into_inner();
            let base_snapshot = snapshot_ref_from_proto(request.base)?;
            let log_input = verify_replay_log_input(request.log)?;
            let bisect_on_divergence = request.bisect_on_divergence;
            let transport = self.snapstore_transport()?;
            let image_resolver = self.inner.image_resolver.clone();
            let manager = self.inner.manager.clone();
            let reserved_at_ms = lease_now_ms();
            let verify_lease = manager
                .allocate(reserved_at_ms)
                .map_err(slot_error_to_status)?;
            let core = match runtime_core(manager.as_ref(), verify_lease.slot_id) {
                Ok(core) => core,
                Err(e) => {
                    let cleanup = manager
                        .destroy(&verify_lease, lease_now_ms())
                        .map_err(slot_error_to_status);
                    return Err(original_or_rollback("VerifyReplay", e, cleanup));
                }
            };

            let (tx, rx) = tokio::sync::oneshot::channel();
            let thread_manager = manager.clone();
            let thread_lease = verify_lease.clone();
            let cleanup_manager = manager.clone();
            let cleanup_lease = verify_lease.clone();
            let spawn = std::thread::Builder::new()
                .name(format!("dh-verify-{}", verify_lease.slot_id))
                .spawn(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        run_verify_replay_on_current_thread(
                            core,
                            base_snapshot,
                            log_input,
                            transport,
                            image_resolver,
                            bisect_on_divergence,
                        )
                    }))
                    .unwrap_or_else(|_| Err(Status::internal("VerifyReplay thread panicked")));
                    let cleanup = thread_manager
                        .destroy(&thread_lease, lease_now_ms())
                        .map_err(slot_error_to_status);
                    let result = match result {
                        Ok(events) => match cleanup {
                            Ok(()) => Ok(events),
                            Err(cleanup) => Err(Status::internal(format!(
                                "VerifyReplay succeeded but slot cleanup failed with {}: {}",
                                cleanup.code(),
                                cleanup.message()
                            ))),
                        },
                        Err(e) => Err(original_or_rollback("VerifyReplay", e, cleanup)),
                    };
                    let _ = tx.send(result);
                });
            if let Err(e) = spawn {
                let cleanup = cleanup_manager
                    .destroy(&cleanup_lease, lease_now_ms())
                    .map_err(slot_error_to_status);
                return Err(original_or_rollback(
                    "VerifyReplay",
                    Status::internal(format!("start VerifyReplay thread: {e}")),
                    cleanup,
                ));
            }

            let events = rx
                .await
                .map_err(|_| Status::internal("VerifyReplay thread ended without response"))??;
            let stream = tokio_stream::iter(events.into_iter().map(Ok));
            Ok(Response::new(Box::pin(stream) as Self::VerifyReplayStream))
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let _ = request;
            Err(unimplemented_status("VerifyReplay"))
        }
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
    use crate::runtime::{SlotActor, SlotPosition, SlotRuntime};
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
            #[cfg(target_arch = "x86_64")]
            image_cache_dir: std::env::temp_dir(),
            #[cfg(target_arch = "x86_64")]
            snapstore: None,
        }
    }

    #[cfg(target_arch = "x86_64")]
    fn test_config_with_resources(
        slots: usize,
        image_cache_dir: PathBuf,
        snapstore: Option<snapstore_client::Transport>,
    ) -> WorkerConfig {
        WorkerConfig {
            image_cache_dir,
            snapstore,
            ..test_config(slots)
        }
    }

    #[cfg(target_arch = "x86_64")]
    fn write_cache_blob(root: &Path, bytes: &[u8]) -> [u8; 32] {
        let hash = *blake3::hash(bytes).as_bytes();
        std::fs::write(root.join(crate::image_resolver::cache_key(&hash)), bytes).unwrap();
        hash
    }

    #[cfg(target_arch = "x86_64")]
    fn service_machine_config(base_hash: [u8; 32], kernel_hash: [u8; 32]) -> proto::MachineConfig {
        let mut config = dh_vmm::config::MachineConfig::new(
            2 * 1024 * 1024,
            base_hash,
            dh_vmm::config::BootSpec::Elf {
                kernel_hash,
                cmdline: b"1000000".to_vec(),
            },
        );
        config.device_set = vec![
            dh_devices::clock::DEVICE_ID_PV_CLOCK,
            dh_devices::pad::DEVICE_ID_PV_PAD,
            dh_devices::entropy::DEVICE_ID_PV_ENTROPY,
            dh_devices::serial::DEVICE_ID_DEBUG_SERIAL,
        ];
        machine_config_to_proto(&config)
    }

    #[cfg(target_arch = "x86_64")]
    fn capture_fixture_machine_config(
        base_hash: [u8; 32],
        kernel_hash: [u8; 32],
    ) -> proto::MachineConfig {
        capture_fixture_machine_config_with_epoch_len(
            base_hash,
            kernel_hash,
            dh_vmm::config::DEFAULT_EPOCH_LEN,
        )
    }

    #[cfg(target_arch = "x86_64")]
    fn capture_fixture_machine_config_with_epoch_len(
        base_hash: [u8; 32],
        kernel_hash: [u8; 32],
        epoch_len: u64,
    ) -> proto::MachineConfig {
        let mut config = dh_vmm::config::MachineConfig::new(
            8 * 1024 * 1024,
            base_hash,
            dh_vmm::config::BootSpec::Elf {
                kernel_hash,
                cmdline: Vec::new(),
            },
        );
        config.epoch_len = epoch_len;
        config.device_set = vec![
            dh_devices::detchannel::DEVICE_ID_DETCHANNEL,
            dh_devices::clock::DEVICE_ID_PV_CLOCK,
            dh_devices::pad::DEVICE_ID_PV_PAD,
            dh_devices::entropy::DEVICE_ID_PV_ENTROPY,
            dh_devices::serial::DEVICE_ID_DEBUG_SERIAL,
        ];
        machine_config_to_proto(&config)
    }

    #[cfg(target_arch = "x86_64")]
    fn capture_fixture_bytes(offset: usize, len: usize) -> Vec<u8> {
        let mut fb = Vec::with_capacity(nanokernel::CAPTURE_FIXTURE_FB_BYTES as usize);
        for j in 0..nanokernel::CAPTURE_FIXTURE_FB_BYTES / 8 {
            fb.extend_from_slice(&(nanokernel::CAPTURE_FIXTURE_FB_QWORD_BASE + j).to_le_bytes());
        }
        fb[offset..offset + len].to_vec()
    }

    #[cfg(target_arch = "x86_64")]
    fn capture_fixture_spec(layout_version: u32) -> proto::CaptureSpec {
        proto::CaptureSpec {
            ranges: vec![proto::ExtractRange {
                region: "framebuffer".into(),
                layout_version,
                offset: 8,
                len: 24,
            }],
            framebuffer: true,
        }
    }

    #[cfg(target_arch = "x86_64")]
    fn stored_input_log_payload(svc: &WorkerService, input_log_id: Vec<u8>) -> Vec<u8> {
        let log_id = log_id_from_bytes(input_log_id).unwrap();
        let store = svc.store().unwrap();
        let store = store.lock().unwrap();
        let container = store.get_input_log(log_id).unwrap();
        input_log_payload_from_container(&container).unwrap()
    }

    #[cfg(target_arch = "x86_64")]
    fn epoch_hashes(log_bytes: &[u8]) -> Vec<(u64, u64, [u8; 32])> {
        dh_inputlog::reader::LogReader::parse(log_bytes)
            .unwrap()
            .aux()
            .filter_map(|rec| match rec.body() {
                dh_inputlog::reader::RecordBody::EpochHash {
                    epoch_index,
                    chain_value,
                } => Some((epoch_index, rec.icount(), chain_value)),
                _ => None,
            })
            .collect()
    }

    #[cfg(target_arch = "x86_64")]
    fn capture_epoch_leg(
        capture: bool,
    ) -> (dh_vmm::runctl::SegmentOutcome, Vec<(u64, u64, [u8; 32])>) {
        dh_vmm::run::install_kick_handler().unwrap();
        let sys = dh_vmm::kvm::KvmSystem::open().unwrap();
        let mut slot = sys.create_slot_vm(8 * 1024 * 1024).unwrap();
        dh_vmm::boot::load_and_enter(&slot, nanokernel::capture_fixture_elf(), b"").unwrap();
        let counter = dh_detclock::counter::InstRetired::open_for_current_thread().unwrap();
        counter
            .route_overflow_to_thread(dh_vmm::run::current_tid(), dh_vmm::run::kick_signal())
            .unwrap();
        counter
            .arm_period(dh_detclock::counter::NEVER_FIRES_PERIOD)
            .unwrap();
        counter.reset().unwrap();
        counter.enable().unwrap();

        let mut config = dh_vmm::config::MachineConfig::new(
            8 * 1024 * 1024,
            [0xCE; 32],
            dh_vmm::config::BootSpec::Elf {
                kernel_hash: [0xCF; 32],
                cmdline: Vec::new(),
            },
        );
        config.epoch_len = 64;
        config.device_set = vec![
            dh_devices::detchannel::DEVICE_ID_DETCHANNEL,
            dh_devices::clock::DEVICE_ID_PV_CLOCK,
            dh_devices::pad::DEVICE_ID_PV_PAD,
            dh_devices::entropy::DEVICE_ID_PV_ENTROPY,
            dh_devices::serial::DEVICE_ID_DEBUG_SERIAL,
        ];
        let config_hash = config.config_hash().unwrap();

        let mem = RuntimeVmMem(slot.guest_mem.clone());
        let mut bus = dh_devices::MmioBus::new();
        bus.register(
            DETCHANNEL_MMIO_BASE,
            Box::new(RuntimeDetChannel::new(
                mem.clone(),
                detguest_host::LogFaultPlan::default(),
                detguest_host::LogFaultPlan::default,
            )),
        )
        .unwrap();
        bus.register(
            dh_devices::clock::PV_CLOCK_BASE,
            Box::new(dh_devices::clock::PvClock::new(
                config.clock.num(),
                config.clock.den(),
            )),
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

        let header = dh_inputlog::dhilog::SegmentHeader {
            base_snapshot_id: [0; 32],
            entropy_seed: [0xC5; 32],
            machine_config_hash: config_hash,
            clock_num: config.clock.num(),
            clock_den: config.clock.den(),
            encoder_fingerprint: 0,
        };
        let rail = std::cell::RefCell::new(dh_vmm::recording::DeviceRail::new(
            bus,
            dh_devices::entropy::DetEntropy::from_seed([0xC5; 32]),
            dh_inputlog::dhilog::LogWriter::new(header),
            mem,
        ));
        let pause = std::sync::atomic::AtomicBool::new(false);
        let mut chain = dh_vmm::hash::StateHashChain::new(&config_hash, &[0; 32]);
        let mut epochs = Vec::new();
        let outcome = {
            let mut segment = dh_vmm::runctl::Segment {
                slot: &mut slot,
                counter: &counter,
                chain: &mut chain,
                config: &config,
                start_icount: 0,
                injections: &[],
                timer: None,
                pause: &pause,
                sdk_events: None,
            };
            dh_vmm::runctl::run_segment_with_epochs(
                &mut segment,
                dh_vmm::runctl::Until::IcountBudget(100_000),
                &mut || false,
                &mut |exit| {
                    let icount = counter.read().map_err(|e| {
                        dh_vmm::boundary::BoundaryError::Exit(format!("counter read: {e:?}"))
                    })?;
                    service_exit_with_detchannel(&mut rail.borrow_mut(), icount, exit)
                },
                &mut |epoch_index, icount, chain_value| {
                    epochs.push((epoch_index, icount, chain_value));
                    rail.borrow_mut()
                        .log_epoch_hash(epoch_index, icount, chain_value)
                        .map_err(|e| {
                            dh_vmm::boundary::BoundaryError::Exit(format!("epoch log: {e:?}"))
                        })
                },
            )
            .unwrap()
        };
        assert!(matches!(
            outcome.reason,
            dh_vmm::runctl::StopReason::BudgetReached | dh_vmm::runctl::StopReason::GuestHalted
        ));
        let mut rail = rail.into_inner();
        if capture {
            let out = capture_at_boundary(
                &mut rail.bus,
                Some(&capture_fixture_spec(
                    nanokernel::CAPTURE_FIXTURE_DEFAULT_LAYOUT_VERSION,
                )),
                0,
            )
            .unwrap();
            assert_eq!(out.feature_bytes, capture_fixture_bytes(8, 24));
            assert!(!out.fb_lz4.is_empty());
        }
        (outcome, epochs)
    }

    #[cfg(target_arch = "x86_64")]
    struct CaptureNeutralityLeg {
        run: proto::RunResponse,
        snap: proto::TakeSnapshotResponse,
        log_bytes: Vec<u8>,
        epoch_hashes: Vec<(u64, u64, [u8; 32])>,
    }

    #[cfg(target_arch = "x86_64")]
    async fn capture_neutrality_leg(
        svc: &WorkerService,
        base_snapshot: proto::SnapshotRef,
        capture: Option<proto::CaptureSpec>,
    ) -> CaptureNeutralityLeg {
        let restored = svc
            .restore_snapshot(Request::new(proto::RestoreSnapshotRequest {
                snapshot: Some(base_snapshot),
                entropy_seed: Vec::new(),
            }))
            .await
            .unwrap()
            .into_inner();
        let lease = restored.lease.unwrap();
        let had_capture = capture.is_some();
        let run = svc
            .run(Request::new(proto::RunRequest {
                lease: Some(lease.clone()),
                until: Some(proto::run_request::Until::IcountBudget(10_000_000)),
                hard_icount_cap: 0,
                capture,
            }))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(
            run.reason,
            proto_stop_reason(dh_vmm::runctl::StopReason::GuestHalted)
        );
        if had_capture {
            assert_eq!(run.feature_bytes, capture_fixture_bytes(8, 24));
            assert!(!run.fb_lz4.is_empty());
            assert!(run.fb_info.is_some());
        } else {
            assert!(run.feature_bytes.is_empty());
            assert!(run.fb_lz4.is_empty());
            assert!(run.fb_info.is_none());
        }

        let snap = svc
            .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                lease: Some(lease.clone()),
                seal_input_log: Some(true),
                capture: None,
            }))
            .await
            .unwrap()
            .into_inner();
        let svc_for_log = svc.clone();
        let input_log_id = snap.input_log_id.clone();
        let log_bytes = tokio::task::spawn_blocking(move || {
            stored_input_log_payload(&svc_for_log, input_log_id)
        })
        .await
        .unwrap();
        let epoch_hashes = epoch_hashes(&log_bytes);
        svc.destroy_vm(Request::new(proto::DestroyVmRequest { lease: Some(lease) }))
            .await
            .unwrap();
        CaptureNeutralityLeg {
            run,
            snap,
            log_bytes,
            epoch_hashes,
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn capture_size_limits_reject_oversized_lengths() {
        assert_eq!(
            checked_capture_len(
                "CaptureSpec feature_bytes",
                MAX_CAPTURE_FEATURE_BYTES as u64,
                MAX_CAPTURE_FEATURE_BYTES
            )
            .unwrap(),
            MAX_CAPTURE_FEATURE_BYTES
        );
        let over = checked_capture_len(
            "CaptureSpec feature_bytes",
            MAX_CAPTURE_FEATURE_BYTES as u64 + 1,
            MAX_CAPTURE_FEATURE_BYTES,
        )
        .unwrap_err();
        assert_eq!(over.code(), tonic::Code::InvalidArgument);
        assert!(over.message().contains("max"));

        let huge = checked_capture_len(
            "framebuffer region",
            u64::MAX,
            MAX_CAPTURE_FRAMEBUFFER_BYTES,
        )
        .unwrap_err();
        assert_eq!(huge.code(), tonic::Code::InvalidArgument);
    }

    #[cfg(target_arch = "x86_64")]
    fn mapper_config() -> dh_vmm::config::MachineConfig {
        let mut config = dh_vmm::config::MachineConfig::new(
            2 * 1024 * 1024,
            [0xAA; 32],
            dh_vmm::config::BootSpec::Elf {
                kernel_hash: [0xBB; 32],
                cmdline: Vec::new(),
            },
        );
        config.device_set = vec![dh_devices::pad::DEVICE_ID_PV_PAD];
        config
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn inject_mapper_accepts_at_frame_pad_set_with_frame_hint() {
        let input = queued_input_from_proto(
            0,
            &proto::ScheduledEvent {
                at: Some(proto::scheduled_event::At::AtFrame(12)),
                event: Some(proto::scheduled_event::Event::PadSet(proto::PadSet {
                    port: 0,
                    buttons: 0xA5A5,
                })),
            },
            100,
            10,
            &mapper_config(),
        )
        .unwrap();
        assert_eq!(input.at, QueuedInputAt::Frame(12));
        assert_eq!(
            input.kind,
            QueuedInputKind::PadSet {
                port: 0,
                buttons: 0xA5A5,
                frame_hint: 12
            }
        );
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn inject_mapper_accepts_generic_device_event() {
        let input = queued_input_from_proto(
            1,
            &proto::ScheduledEvent {
                at: Some(proto::scheduled_event::At::AtIcount(150)),
                event: Some(proto::scheduled_event::Event::DevEvent(
                    proto::DeviceEvent {
                        device_id: u32::from(dh_inputlog::dhilog::DEVICE_ID_DETCHANNEL),
                        event_type: u32::from(dh_inputlog::dhilog::EVENT_RING_PUSH),
                        payload: vec![1, 2, 3, 4],
                    },
                )),
            },
            100,
            10,
            &mapper_config(),
        )
        .unwrap();
        assert_eq!(input.at, QueuedInputAt::Icount(150));
        assert_eq!(
            input.kind,
            QueuedInputKind::DevEvent {
                device_id: dh_inputlog::dhilog::DEVICE_ID_DETCHANNEL,
                event_type: dh_inputlog::dhilog::EVENT_RING_PUSH,
                payload: vec![1, 2, 3, 4]
            }
        );
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn inject_mapper_rejects_stale_frame_and_oversized_device_event() {
        let stale = queued_input_from_proto(
            0,
            &proto::ScheduledEvent {
                at: Some(proto::scheduled_event::At::AtFrame(10)),
                event: Some(proto::scheduled_event::Event::PadSet(proto::PadSet {
                    port: 0,
                    buttons: 1,
                })),
            },
            100,
            10,
            &mapper_config(),
        )
        .unwrap_err();
        assert_eq!(stale.code(), tonic::Code::InvalidArgument);
        assert!(stale.message().contains("current frame_counter 10"));

        let oversized = queued_input_from_proto(
            1,
            &proto::ScheduledEvent {
                at: Some(proto::scheduled_event::At::AtIcount(150)),
                event: Some(proto::scheduled_event::Event::DevEvent(
                    proto::DeviceEvent {
                        device_id: 1,
                        event_type: 1,
                        payload: vec![0; dh_inputlog::dhilog::MAX_DEV_EVENT_DATA + 1],
                    },
                )),
            },
            100,
            10,
            &mapper_config(),
        )
        .unwrap_err();
        assert_eq!(oversized.code(), tonic::Code::InvalidArgument);
        assert!(oversized.message().contains("dev_event.payload exceeds"));
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn inject_mapper_rejects_reserved_frame_and_missing_pv_pad() {
        let reserved = queued_input_from_proto(
            0,
            &proto::ScheduledEvent {
                at: Some(proto::scheduled_event::At::AtFrame(
                    dh_inputlog::dhilog::FRAME_HINT_NONE,
                )),
                event: Some(proto::scheduled_event::Event::PadSet(proto::PadSet {
                    port: 0,
                    buttons: 1,
                })),
            },
            100,
            10,
            &mapper_config(),
        )
        .unwrap_err();
        assert_eq!(reserved.code(), tonic::Code::InvalidArgument);
        assert!(reserved.message().contains("reserved"));

        let mut no_pad = mapper_config();
        no_pad.device_set.clear();
        let missing = queued_input_from_proto(
            1,
            &proto::ScheduledEvent {
                at: Some(proto::scheduled_event::At::AtFrame(11)),
                event: Some(proto::scheduled_event::Event::PadSet(proto::PadSet {
                    port: 0,
                    buttons: 1,
                })),
            },
            100,
            10,
            &no_pad,
        )
        .unwrap_err();
        assert_eq!(missing.code(), tonic::Code::FailedPrecondition);
        assert!(missing.message().contains("requires pv-pad"));
    }

    #[cfg(target_arch = "x86_64")]
    fn set_pad_irq_vector(pad: &mut dh_devices::pad::PvPad, vector: u32) {
        let mut log = dh_inputlog::dhilog::LogWriter::new(dh_inputlog::dhilog::SegmentHeader {
            base_snapshot_id: [0; 32],
            entropy_seed: [0; 32],
            machine_config_hash: [0; 32],
            clock_num: 1,
            clock_den: 1,
            encoder_fingerprint: 0,
        });
        let mut mem = dh_devices::ctx::VecGuestMem(vec![0; 8]);
        let mut entropy = dh_devices::entropy::DetEntropy::from_seed([0; 32]);
        let mut irqs = Vec::new();
        let mut ctx =
            dh_devices::ctx::DevCtx::new(0, 0, &mut log, &mut mem, &mut entropy, &mut irqs);
        dh_devices::DetDevice::mmio_write(
            pad,
            dh_devices::pad::REG_IRQ_VECTOR,
            &vector.to_le_bytes(),
            &mut ctx,
        );
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn frame_scheduled_inputs_reject_current_irq_delivery_gap() {
        let mut bus = dh_devices::MmioBus::new();
        let mut pad = dh_devices::pad::PvPad::new();
        set_pad_irq_vector(&mut pad, 0x45);
        bus.register(dh_devices::pad::PV_PAD_BASE, Box::new(pad))
            .unwrap();

        let reason = frame_scheduled_irq_precondition(
            &mut bus,
            &QueuedInputKind::PadSet {
                port: 0,
                buttons: 1,
                frame_hint: 12,
            },
        )
        .unwrap();
        assert!(reason.contains("pv-pad IRQ vector is enabled"));

        let mut polling_bus = dh_devices::MmioBus::new();
        polling_bus
            .register(
                dh_devices::pad::PV_PAD_BASE,
                Box::new(dh_devices::pad::PvPad::new()),
            )
            .unwrap();
        assert_eq!(
            frame_scheduled_irq_precondition(
                &mut polling_bus,
                &QueuedInputKind::PadSet {
                    port: 0,
                    buttons: 1,
                    frame_hint: 12,
                },
            ),
            None
        );
    }

    #[cfg(target_arch = "x86_64")]
    fn spawn_store_for_service_test() -> (
        tokio::runtime::Runtime,
        snapstore_server::build_server::ServerHandle,
        tempfile::TempDir,
        snapstore_client::Transport,
    ) {
        let dir = tempfile::TempDir::new().unwrap();
        let uds_path = dir.path().join("snapstore.sock");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let config = snapstore_server::config::ServerConfig {
            data_root: dir.path().to_path_buf(),
            grpc_tcp_addr: "127.0.0.1:0".parse().unwrap(),
            grpc_uds_path: Some(uds_path.clone()),
            page_channel_path: Some(dir.path().join("snapstore.sock.pages")),
            http_addr: "127.0.0.1:0".parse().unwrap(),
            pagestore: Default::default(),
            meta: Default::default(),
            page_channel: Default::default(),
        };
        let (handle, uds) = rt
            .block_on(snapstore_server::build_server::serve_for_tests(config))
            .unwrap();
        (rt, handle, dir, snapstore_client::Transport::Uds(uds))
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
    async fn create_vm_rejects_missing_config_before_engine_work() {
        let svc = WorkerService::new(test_config(1)).unwrap();
        let err = svc
            .create_vm(Request::new(proto::CreateVmRequest::default()))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert_eq!(err.message(), "missing config");
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn create_vm_and_take_snapshot_use_real_cache_kvm_and_store() {
        if !runtime_tests_available() {
            return;
        }
        let image_cache = tempfile::TempDir::new().unwrap();
        let base_hash = write_cache_blob(image_cache.path(), &vec![0u8; 4096]);
        let kernel_hash = write_cache_blob(image_cache.path(), nanokernel::landing_loop_elf());
        let (_store_rt, _handle, _store_dir, transport) = spawn_store_for_service_test();
        let svc = WorkerService::new(test_config_with_resources(
            1,
            image_cache.path().to_path_buf(),
            Some(transport),
        ))
        .unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let created = svc
                .create_vm(Request::new(proto::CreateVmRequest {
                    config: Some(service_machine_config(base_hash, kernel_hash)),
                    entropy_seed: vec![0xA5; 32],
                }))
                .await
                .unwrap()
                .into_inner();
            let lease = created.lease.unwrap();
            assert_eq!(created.icount, 0);
            assert_eq!(svc.runtime_table().occupied_count(), 1);

            let snap = svc
                .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                    lease: Some(lease.clone()),
                    seal_input_log: Some(true),
                    capture: None,
                }))
                .await
                .unwrap()
                .into_inner();
            let snapshot = snap.snapshot.unwrap();
            assert_eq!(snapshot.hash.len(), 32);
            assert_eq!(snap.input_log_id.len(), 32);
            assert_eq!(snap.icount, 0);
            assert_eq!(snap.vns, 0);
            assert_eq!(snap.state_hash.unwrap().hash.len(), 32);
            assert_eq!(snap.machine_config_hash.len(), 32);
            assert_eq!(snap.dirty_pages, 512);
            assert_eq!(snap.frame_counter, 0);
            assert_eq!(
                svc.slot_manager()
                    .slot_info(lease.slot_id)
                    .unwrap()
                    .base_snapshot_id,
                Some(snapshot.hash.try_into().unwrap())
            );
        });
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn run_rpc_reuses_actor_counter_across_sequential_runs() {
        if !runtime_tests_available() {
            return;
        }
        let image_cache = tempfile::TempDir::new().unwrap();
        let base_hash = write_cache_blob(image_cache.path(), &vec![0u8; 4096]);
        let kernel_hash = write_cache_blob(image_cache.path(), nanokernel::landing_loop_elf());
        let svc = WorkerService::new(test_config_with_resources(
            1,
            image_cache.path().to_path_buf(),
            None,
        ))
        .unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let created = svc
                .create_vm(Request::new(proto::CreateVmRequest {
                    config: Some(service_machine_config(base_hash, kernel_hash)),
                    entropy_seed: vec![0x5A; 32],
                }))
                .await
                .unwrap()
                .into_inner();
            let lease = created.lease.unwrap();
            let injected = svc
                .inject_inputs(Request::new(proto::InjectInputsRequest {
                    lease: Some(lease.clone()),
                    events: vec![proto::ScheduledEvent {
                        at: Some(proto::scheduled_event::At::AtIcount(25_000)),
                        event: Some(proto::scheduled_event::Event::PadSet(proto::PadSet {
                            port: 0,
                            buttons: 0xA5A5,
                        })),
                    }],
                }))
                .await
                .unwrap()
                .into_inner();
            assert_eq!(injected.scheduled, 1);
            assert_eq!(
                svc.runtime_table()
                    .with(lease.slot_id, |actor| actor
                        .with_runtime(|runtime| runtime.queued_inputs.len())
                        .unwrap())
                    .unwrap(),
                1
            );
            let first = svc
                .run(Request::new(proto::RunRequest {
                    lease: Some(lease.clone()),
                    until: Some(proto::run_request::Until::IcountBudget(20_000)),
                    hard_icount_cap: 0,
                    capture: None,
                }))
                .await
                .unwrap()
                .into_inner();
            assert_eq!(
                first.reason,
                proto_stop_reason(dh_vmm::runctl::StopReason::BudgetReached)
            );
            assert_eq!(first.icount, 20_000);
            assert_eq!(first.state_hash.unwrap().hash.len(), 32);
            assert_eq!(
                svc.runtime_table()
                    .with(lease.slot_id, |actor| actor
                        .with_runtime(|runtime| runtime.queued_inputs.len())
                        .unwrap())
                    .unwrap(),
                1,
                "future input should stay queued after a shorter run"
            );

            let second = svc
                .run(Request::new(proto::RunRequest {
                    lease: Some(lease.clone()),
                    until: Some(proto::run_request::Until::IcountBudget(30_000)),
                    hard_icount_cap: 0,
                    capture: None,
                }))
                .await
                .unwrap()
                .into_inner();
            assert_eq!(
                second.reason,
                proto_stop_reason(dh_vmm::runctl::StopReason::BudgetReached)
            );
            assert_eq!(second.icount, 50_000);
            assert_eq!(second.state_hash.unwrap().hash.len(), 32);
            assert_eq!(
                svc.runtime_table()
                    .with(lease.slot_id, |actor| actor
                        .with_runtime(|runtime| runtime.queued_inputs.len())
                        .unwrap())
                    .unwrap(),
                0,
                "scheduled input should drain inside the second run"
            );
            assert_eq!(
                svc.slot_manager().slot_info(lease.slot_id).unwrap().icount,
                50_000
            );
            assert_eq!(
                svc.runtime_table()
                    .with(lease.slot_id, |actor| actor
                        .with_runtime(|runtime| runtime.position.segment_icount)
                        .unwrap())
                    .unwrap(),
                50_000
            );
        });
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn take_snapshot_defaults_to_sealing() {
        if !runtime_tests_available() {
            return;
        }
        let image_cache = tempfile::TempDir::new().unwrap();
        let base_hash = write_cache_blob(image_cache.path(), &vec![0u8; 4096]);
        let kernel_hash = write_cache_blob(image_cache.path(), nanokernel::landing_loop_elf());
        let (_store_rt, _handle, _store_dir, transport) = spawn_store_for_service_test();
        let svc = WorkerService::new(test_config_with_resources(
            1,
            image_cache.path().to_path_buf(),
            Some(transport),
        ))
        .unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let created = svc
                .create_vm(Request::new(proto::CreateVmRequest {
                    config: Some(service_machine_config(base_hash, kernel_hash)),
                    entropy_seed: vec![0xA5; 32],
                }))
                .await
                .unwrap()
                .into_inner();
            let lease = created.lease.unwrap();

            let snap = svc
                .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                    lease: Some(lease),
                    seal_input_log: None,
                    capture: None,
                }))
                .await
                .unwrap()
                .into_inner();
            assert_eq!(snap.input_log_id.len(), 32);
        });
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn run_capture_spec_reads_manifest_ranges_and_lz4_framebuffer() {
        if !runtime_tests_available() {
            return;
        }
        let image_cache = tempfile::TempDir::new().unwrap();
        let base_hash = write_cache_blob(image_cache.path(), &vec![0u8; 4096]);
        let kernel_hash = write_cache_blob(image_cache.path(), nanokernel::capture_fixture_elf());
        let svc = WorkerService::new(test_config_with_resources(
            1,
            image_cache.path().to_path_buf(),
            None,
        ))
        .unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let created = svc
                .create_vm(Request::new(proto::CreateVmRequest {
                    config: Some(capture_fixture_machine_config(base_hash, kernel_hash)),
                    entropy_seed: vec![0xC6; 32],
                }))
                .await
                .unwrap()
                .into_inner();
            let lease = created.lease.unwrap();

            let run = svc
                .run(Request::new(proto::RunRequest {
                    lease: Some(lease),
                    until: Some(proto::run_request::Until::IcountBudget(10_000_000)),
                    hard_icount_cap: 0,
                    capture: Some(proto::CaptureSpec {
                        ranges: vec![proto::ExtractRange {
                            region: "framebuffer".into(),
                            layout_version: nanokernel::CAPTURE_FIXTURE_DEFAULT_LAYOUT_VERSION,
                            offset: 8,
                            len: 24,
                        }],
                        framebuffer: true,
                    }),
                }))
                .await
                .unwrap()
                .into_inner();
            assert_eq!(
                run.reason,
                proto_stop_reason(dh_vmm::runctl::StopReason::GuestHalted)
            );
            assert_eq!(run.feature_bytes, capture_fixture_bytes(8, 24));
            let pixels = lz4_flex::decompress_size_prepended(&run.fb_lz4).unwrap();
            assert_eq!(pixels.len(), nanokernel::CAPTURE_FIXTURE_FB_BYTES as usize);
            assert_eq!(&pixels[..32], &capture_fixture_bytes(0, 32));
            let fb_info = run.fb_info.unwrap();
            assert_eq!(
                fb_info.format,
                proto_pixel_format(proto::PixelFormat::PfUnspecified)
            );
            assert_eq!(fb_info.frame_counter, 0);
        });
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn m6_accept_capture_neutrality_and_layout_precondition() {
        if !runtime_tests_available() {
            return;
        }

        let (plain_epoch_out, plain_epochs) = capture_epoch_leg(false);
        let (captured_epoch_out, captured_epochs) = capture_epoch_leg(true);
        assert!(
            !plain_epochs.is_empty(),
            "acceptance fixture must exercise epoch hash records"
        );
        assert_eq!(captured_epoch_out.state_hash, plain_epoch_out.state_hash);
        assert_eq!(
            captured_epochs, plain_epochs,
            "capture must not perturb epoch hashes"
        );

        let image_cache = tempfile::TempDir::new().unwrap();
        let base_hash = write_cache_blob(image_cache.path(), &vec![0u8; 4096]);
        let kernel_hash = write_cache_blob(image_cache.path(), nanokernel::capture_fixture_elf());
        let (_store_rt, _handle, _store_dir, transport) = spawn_store_for_service_test();
        let svc = WorkerService::new(test_config_with_resources(
            3,
            image_cache.path().to_path_buf(),
            Some(transport),
        ))
        .unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let created = svc
                .create_vm(Request::new(proto::CreateVmRequest {
                    config: Some(capture_fixture_machine_config(base_hash, kernel_hash)),
                    entropy_seed: vec![0xCA; 32],
                }))
                .await
                .unwrap()
                .into_inner();
            let root_lease = created.lease.unwrap();
            let base_snapshot = svc
                .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                    lease: Some(root_lease.clone()),
                    seal_input_log: Some(true),
                    capture: None,
                }))
                .await
                .unwrap()
                .into_inner()
                .snapshot
                .unwrap();
            svc.destroy_vm(Request::new(proto::DestroyVmRequest {
                lease: Some(root_lease),
            }))
            .await
            .unwrap();

            let plain = capture_neutrality_leg(&svc, base_snapshot.clone(), None).await;
            let captured = capture_neutrality_leg(
                &svc,
                base_snapshot.clone(),
                Some(capture_fixture_spec(
                    nanokernel::CAPTURE_FIXTURE_DEFAULT_LAYOUT_VERSION,
                )),
            )
            .await;

            assert_eq!(captured.run.icount, plain.run.icount);
            assert_eq!(captured.run.vns, plain.run.vns);
            assert_eq!(captured.run.state_hash, plain.run.state_hash);
            assert_eq!(
                captured.snap.snapshot.as_ref().unwrap().hash,
                plain.snap.snapshot.as_ref().unwrap().hash,
                "capture must not perturb the child snapshot ref"
            );
            assert_eq!(captured.snap.state_hash, plain.snap.state_hash);
            assert_eq!(
                captured.log_bytes, plain.log_bytes,
                "capture must not perturb the sealed DHILOG"
            );
            assert_eq!(
                captured.epoch_hashes, plain.epoch_hashes,
                "capture must not perturb service DHILOG epoch records"
            );

            let bad_capture =
                capture_fixture_spec(nanokernel::CAPTURE_FIXTURE_DEFAULT_LAYOUT_VERSION + 1);
            let restored = svc
                .restore_snapshot(Request::new(proto::RestoreSnapshotRequest {
                    snapshot: Some(base_snapshot.clone()),
                    entropy_seed: Vec::new(),
                }))
                .await
                .unwrap()
                .into_inner();
            let lease = restored.lease.unwrap();
            let err = svc
                .run(Request::new(proto::RunRequest {
                    lease: Some(lease.clone()),
                    until: Some(proto::run_request::Until::IcountBudget(10_000_000)),
                    hard_icount_cap: 0,
                    capture: Some(bad_capture.clone()),
                }))
                .await
                .unwrap_err();
            assert_eq!(err.code(), tonic::Code::FailedPrecondition);
            assert!(err.message().contains("layout_version"));
            svc.destroy_vm(Request::new(proto::DestroyVmRequest { lease: Some(lease) }))
                .await
                .unwrap();

            let restored = svc
                .restore_snapshot(Request::new(proto::RestoreSnapshotRequest {
                    snapshot: Some(base_snapshot),
                    entropy_seed: Vec::new(),
                }))
                .await
                .unwrap()
                .into_inner();
            let lease = restored.lease.unwrap();
            svc.run(Request::new(proto::RunRequest {
                lease: Some(lease.clone()),
                until: Some(proto::run_request::Until::IcountBudget(10_000_000)),
                hard_icount_cap: 0,
                capture: None,
            }))
            .await
            .unwrap();
            let err = svc
                .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                    lease: Some(lease.clone()),
                    seal_input_log: Some(true),
                    capture: Some(bad_capture),
                }))
                .await
                .unwrap_err();
            assert_eq!(err.code(), tonic::Code::FailedPrecondition);
            assert!(err.message().contains("layout_version"));
            svc.destroy_vm(Request::new(proto::DestroyVmRequest { lease: Some(lease) }))
                .await
                .unwrap();
        });
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn run_capture_layout_mismatch_commits_successful_run_boundary() {
        if !runtime_tests_available() {
            return;
        }
        let image_cache = tempfile::TempDir::new().unwrap();
        let base_hash = write_cache_blob(image_cache.path(), &vec![0u8; 4096]);
        let kernel_hash = write_cache_blob(image_cache.path(), nanokernel::capture_fixture_elf());
        let svc = WorkerService::new(test_config_with_resources(
            1,
            image_cache.path().to_path_buf(),
            None,
        ))
        .unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let created = svc
                .create_vm(Request::new(proto::CreateVmRequest {
                    config: Some(capture_fixture_machine_config(base_hash, kernel_hash)),
                    entropy_seed: vec![0xC9; 32],
                }))
                .await
                .unwrap()
                .into_inner();
            let lease = created.lease.unwrap();

            let err = svc
                .run(Request::new(proto::RunRequest {
                    lease: Some(lease.clone()),
                    until: Some(proto::run_request::Until::IcountBudget(10_000_000)),
                    hard_icount_cap: 0,
                    capture: Some(proto::CaptureSpec {
                        ranges: vec![proto::ExtractRange {
                            region: "framebuffer".into(),
                            layout_version: nanokernel::CAPTURE_FIXTURE_DEFAULT_LAYOUT_VERSION + 1,
                            offset: 0,
                            len: 8,
                        }],
                        framebuffer: false,
                    }),
                }))
                .await
                .unwrap_err();
            assert_eq!(err.code(), tonic::Code::FailedPrecondition);
            assert!(err.message().contains("layout_version"));

            let info = svc.slot_manager().slot_info(lease.slot_id).unwrap();
            assert_eq!(info.state, dh_vmm::SlotState::Paused);
            assert!(
                info.icount > 0,
                "Run capture errors are post-run validation errors; the slot position is committed"
            );
            let runtime_icount = svc
                .runtime_table()
                .with(lease.slot_id, |actor| {
                    actor
                        .with_runtime(|runtime| runtime.position.cumulative_icount)
                        .unwrap()
                })
                .unwrap();
            assert_eq!(runtime_icount, info.icount);
        });
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn take_snapshot_capture_checks_layout_version_and_returns_features() {
        if !runtime_tests_available() {
            return;
        }
        let image_cache = tempfile::TempDir::new().unwrap();
        let base_hash = write_cache_blob(image_cache.path(), &vec![0u8; 4096]);
        let kernel_hash = write_cache_blob(image_cache.path(), nanokernel::capture_fixture_elf());
        let (_store_rt, _handle, _store_dir, transport) = spawn_store_for_service_test();
        let svc = WorkerService::new(test_config_with_resources(
            1,
            image_cache.path().to_path_buf(),
            Some(transport),
        ))
        .unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let created = svc
                .create_vm(Request::new(proto::CreateVmRequest {
                    config: Some(capture_fixture_machine_config(base_hash, kernel_hash)),
                    entropy_seed: vec![0xC7; 32],
                }))
                .await
                .unwrap()
                .into_inner();
            let lease = created.lease.unwrap();

            let run = svc
                .run(Request::new(proto::RunRequest {
                    lease: Some(lease.clone()),
                    until: Some(proto::run_request::Until::IcountBudget(10_000_000)),
                    hard_icount_cap: 0,
                    capture: None,
                }))
                .await
                .unwrap()
                .into_inner();
            assert_eq!(
                run.reason,
                proto_stop_reason(dh_vmm::runctl::StopReason::GuestHalted)
            );

            let bad = svc
                .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                    lease: Some(lease.clone()),
                    seal_input_log: Some(true),
                    capture: Some(proto::CaptureSpec {
                        ranges: vec![proto::ExtractRange {
                            region: "framebuffer".into(),
                            layout_version: nanokernel::CAPTURE_FIXTURE_DEFAULT_LAYOUT_VERSION + 1,
                            offset: 0,
                            len: 8,
                        }],
                        framebuffer: false,
                    }),
                }))
                .await
                .unwrap_err();
            assert_eq!(bad.code(), tonic::Code::FailedPrecondition);
            assert!(bad.message().contains("layout_version"));
            assert_eq!(
                svc.slot_manager()
                    .slot_info(lease.slot_id)
                    .unwrap()
                    .base_snapshot_id,
                None,
                "failed capture must not publish a snapshot"
            );

            let snap = svc
                .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                    lease: Some(lease),
                    seal_input_log: Some(true),
                    capture: Some(proto::CaptureSpec {
                        ranges: vec![proto::ExtractRange {
                            region: "framebuffer".into(),
                            layout_version: nanokernel::CAPTURE_FIXTURE_DEFAULT_LAYOUT_VERSION,
                            offset: 16,
                            len: 16,
                        }],
                        framebuffer: false,
                    }),
                }))
                .await
                .unwrap()
                .into_inner();
            assert_eq!(snap.feature_bytes, capture_fixture_bytes(16, 16));
            assert!(snap.fb_lz4.is_empty());
            assert!(snap.fb_info.is_none());
            assert_eq!(snap.input_log_id.len(), 32);
        });
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn verify_replay_rpc_handles_detchannel_capture_fixture_log() {
        if !runtime_tests_available() {
            return;
        }
        use proto::verify_replay_progress::Msg as VerifyMsg;
        use proto::verify_replay_request::Log as VerifyLog;
        use tokio_stream::StreamExt;

        let image_cache = tempfile::TempDir::new().unwrap();
        let base_hash = write_cache_blob(image_cache.path(), &vec![0u8; 4096]);
        let kernel_hash = write_cache_blob(image_cache.path(), nanokernel::capture_fixture_elf());
        let (_store_rt, _handle, _store_dir, transport) = spawn_store_for_service_test();
        let svc = WorkerService::new(test_config_with_resources(
            2,
            image_cache.path().to_path_buf(),
            Some(transport),
        ))
        .unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let created = svc
                .create_vm(Request::new(proto::CreateVmRequest {
                    config: Some(capture_fixture_machine_config(base_hash, kernel_hash)),
                    entropy_seed: vec![0xC8; 32],
                }))
                .await
                .unwrap()
                .into_inner();
            let root_lease = created.lease.unwrap();
            let base_snapshot = svc
                .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                    lease: Some(root_lease.clone()),
                    seal_input_log: Some(true),
                    capture: None,
                }))
                .await
                .unwrap()
                .into_inner()
                .snapshot
                .unwrap();
            svc.destroy_vm(Request::new(proto::DestroyVmRequest {
                lease: Some(root_lease),
            }))
            .await
            .unwrap();

            let restored = svc
                .restore_snapshot(Request::new(proto::RestoreSnapshotRequest {
                    snapshot: Some(base_snapshot.clone()),
                    entropy_seed: Vec::new(),
                }))
                .await
                .unwrap()
                .into_inner();
            let lease = restored.lease.unwrap();
            let run = svc
                .run(Request::new(proto::RunRequest {
                    lease: Some(lease.clone()),
                    until: Some(proto::run_request::Until::IcountBudget(10_000_000)),
                    hard_icount_cap: 0,
                    capture: Some(proto::CaptureSpec {
                        ranges: vec![proto::ExtractRange {
                            region: "framebuffer".into(),
                            layout_version: nanokernel::CAPTURE_FIXTURE_DEFAULT_LAYOUT_VERSION,
                            offset: 0,
                            len: 8,
                        }],
                        framebuffer: false,
                    }),
                }))
                .await
                .unwrap()
                .into_inner();
            assert_eq!(
                run.reason,
                proto_stop_reason(dh_vmm::runctl::StopReason::GuestHalted)
            );
            assert_eq!(run.feature_bytes, capture_fixture_bytes(0, 8));

            let snap = svc
                .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                    lease: Some(lease),
                    seal_input_log: Some(true),
                    capture: None,
                }))
                .await
                .unwrap()
                .into_inner();
            assert_eq!(snap.input_log_id.len(), 32);

            let mut stream = svc
                .verify_replay(Request::new(proto::VerifyReplayRequest {
                    base: Some(base_snapshot),
                    log: Some(VerifyLog::InputLogId(snap.input_log_id)),
                    bisect_on_divergence: false,
                }))
                .await
                .unwrap()
                .into_inner();
            let mut saw_done = false;
            let mut progress = Vec::new();
            while let Some(event) = stream.next().await {
                let msg = event.unwrap().msg;
                progress.push(format!("{msg:?}"));
                if matches!(msg, Some(VerifyMsg::Done(_))) {
                    saw_done = true;
                }
            }
            assert!(
                saw_done,
                "VerifyReplay should finish the detchannel log, got {progress:?}"
            );
        });
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn verify_replay_rpc_streams_done_for_stored_input_log() {
        if !runtime_tests_available() {
            return;
        }
        use proto::verify_replay_progress::Msg as VerifyMsg;
        use proto::verify_replay_request::Log as VerifyLog;
        use tokio_stream::StreamExt;

        let image_cache = tempfile::TempDir::new().unwrap();
        let base_hash = write_cache_blob(image_cache.path(), &vec![0u8; 4096]);
        let kernel_hash = write_cache_blob(image_cache.path(), nanokernel::landing_loop_elf());
        let (_store_rt, _handle, _store_dir, transport) = spawn_store_for_service_test();
        let svc = WorkerService::new(test_config_with_resources(
            2,
            image_cache.path().to_path_buf(),
            Some(transport),
        ))
        .unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let created = svc
                .create_vm(Request::new(proto::CreateVmRequest {
                    config: Some(service_machine_config(base_hash, kernel_hash)),
                    entropy_seed: vec![0xA5; 32],
                }))
                .await
                .unwrap()
                .into_inner();
            let base_lease = created.lease.unwrap();
            let base_snapshot = svc
                .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                    lease: Some(base_lease.clone()),
                    seal_input_log: Some(true),
                    capture: None,
                }))
                .await
                .unwrap()
                .into_inner()
                .snapshot
                .unwrap();
            svc.destroy_vm(Request::new(proto::DestroyVmRequest {
                lease: Some(base_lease),
            }))
            .await
            .unwrap();

            let restored = svc
                .restore_snapshot(Request::new(proto::RestoreSnapshotRequest {
                    snapshot: Some(base_snapshot.clone()),
                    entropy_seed: Vec::new(),
                }))
                .await
                .unwrap()
                .into_inner();
            let lease = restored.lease.unwrap();
            let run = svc
                .run(Request::new(proto::RunRequest {
                    lease: Some(lease.clone()),
                    until: Some(proto::run_request::Until::IcountBudget(50_000)),
                    hard_icount_cap: 0,
                    capture: None,
                }))
                .await
                .unwrap()
                .into_inner();
            assert_eq!(run.icount, 50_000);

            let snap = svc
                .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                    lease: Some(lease),
                    seal_input_log: Some(true),
                    capture: None,
                }))
                .await
                .unwrap()
                .into_inner();
            assert_eq!(snap.input_log_id.len(), 32);

            let mut stream = svc
                .verify_replay(Request::new(proto::VerifyReplayRequest {
                    base: Some(base_snapshot),
                    log: Some(VerifyLog::InputLogId(snap.input_log_id)),
                    bisect_on_divergence: true,
                }))
                .await
                .unwrap()
                .into_inner();
            let mut done = None;
            while let Some(item) = stream.next().await {
                match item.unwrap().msg.unwrap() {
                    VerifyMsg::EpochOk(_) => {}
                    VerifyMsg::Done(msg) => done = Some(msg),
                    VerifyMsg::Divergence(div) => panic!("unexpected divergence: {div:?}"),
                }
            }
            let done = done.expect("VerifyReplay must stream Done");
            assert_eq!(done.total_icount, 50_000);
            assert_eq!(done.end_state_hash.unwrap().hash.len(), 32);
        });
    }

    #[cfg(target_arch = "x86_64")]
    #[tokio::test]
    async fn verify_replay_rejects_oversized_inline_log_before_resources() {
        let svc = WorkerService::new(test_config(1)).unwrap();
        let err = match svc
            .verify_replay(Request::new(proto::VerifyReplayRequest {
                base: Some(proto::SnapshotRef { hash: vec![0; 32] }),
                log: Some(proto::verify_replay_request::Log::InputLog(vec![
                    0;
                    VERIFY_REPLAY_INLINE_LOG_MAX_BYTES
                        + 1
                ])),
                bisect_on_divergence: false,
            }))
            .await
        {
            Ok(_) => panic!("oversized inline log must be rejected"),
            Err(err) => err,
        };
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("VerifyReplay.input_log exceeds"));
        assert_eq!(svc.slots_free(), 1);
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn verify_replay_requires_worker_slot_capacity() {
        let image_cache = tempfile::TempDir::new().unwrap();
        let (_store_rt, _handle, _store_dir, transport) = spawn_store_for_service_test();
        let svc = WorkerService::new(test_config_with_resources(
            1,
            image_cache.path().to_path_buf(),
            Some(transport),
        ))
        .unwrap();
        let _held = svc.slot_manager().allocate(0).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let err = match svc
                .verify_replay(Request::new(proto::VerifyReplayRequest {
                    base: Some(proto::SnapshotRef { hash: vec![0; 32] }),
                    log: Some(proto::verify_replay_request::Log::InputLog(vec![0; 256])),
                    bisect_on_divergence: false,
                }))
                .await
            {
                Ok(_) => panic!("VerifyReplay must require a free worker slot"),
                Err(err) => err,
            };
            assert_eq!(err.code(), tonic::Code::ResourceExhausted);
        });
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn verify_replay_divergence_mapping_is_honest_about_bisection() {
        use proto::verify_replay_progress::Msg as VerifyMsg;

        let divergence = VerifyProgress::Divergence {
            first_bad_epoch: None,
            at_icount: 123,
            what: "end_state_hash",
            expected: [0x11; 32],
            got: [0x22; 32],
        };
        let err = verify_progress_to_proto(divergence.clone(), true).unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unimplemented);
        assert!(err.message().contains("bisection"));

        let progress = verify_progress_to_proto(divergence, false).unwrap();
        let div = match progress.msg.unwrap() {
            VerifyMsg::Divergence(div) => div,
            other => panic!("expected Divergence, got {other:?}"),
        };
        assert_eq!(div.first_bad_epoch, 0);
        assert_eq!(div.icount_lo, 123);
        assert_eq!(div.icount_hi, 123);
        assert!(div.reg_diff.is_empty());
        assert!(div.diff_page_idx.is_empty());
        assert!(div.suspected_cause.contains("first_bad_epoch=none"));
        assert!(div.suspected_cause.contains("expected_hash="));
        assert!(div.suspected_cause.contains("got_hash="));
    }

    #[cfg(target_arch = "x86_64")]
    #[tokio::test]
    async fn restore_rejects_explicit_zero_entropy_seed() {
        let svc = WorkerService::new(test_config(1)).unwrap();
        let err = svc
            .restore_snapshot(Request::new(proto::RestoreSnapshotRequest {
                snapshot: Some(proto::SnapshotRef {
                    hash: vec![0x11; 32],
                }),
                entropy_seed: vec![0; 32],
            }))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("omit it to continue"));
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
                if std::env::var_os("DH_REQUIRE_KVM_TESTS").is_some() {
                    panic!("KVM runtime tests were required but dirty rings are unavailable");
                }
                false
            }
            Err(e) => {
                eprintln!("skipping runtime service test: KVM unavailable: {e:?}");
                if std::env::var_os("DH_REQUIRE_KVM_TESTS").is_some() {
                    panic!("KVM runtime tests were required but KVM is unavailable: {e:?}");
                }
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
    fn make_actor(
        slot_id: u64,
        seed: u8,
        position: SlotPosition,
        base_snapshot: Option<snapstore_types::SnapshotRef>,
    ) -> Result<Arc<SlotActor>, Status> {
        SlotActor::start(
            slot_id,
            u32::try_from(slot_id).unwrap(),
            make_runtime(seed, position, base_snapshot)?,
        )
        .map(Arc::new)
        .map_err(|e| Status::internal(format!("start slot actor: {e}")))
    }

    #[cfg(target_arch = "x86_64")]
    #[tokio::test]
    async fn slot_actors_own_distinct_threads_and_counters() {
        if !runtime_tests_available() {
            return;
        }
        let svc = WorkerService::new(test_config(2)).unwrap();
        let a = svc
            .install_allocated_runtime("CreateVm", |_| {
                make_runtime(0x41, SlotPosition::default(), None)
            })
            .await
            .unwrap();
        let b = svc
            .install_allocated_runtime("CreateVm", |_| {
                make_runtime(0x42, SlotPosition::default(), None)
            })
            .await
            .unwrap();

        let table = svc.runtime_table();
        let a_info = table
            .with(a.slot_id, |actor| {
                (
                    actor.tid(),
                    actor
                        .with_runtime(|runtime| {
                            (
                                dh_vmm::run::current_tid(),
                                runtime.counter.is_some(),
                                runtime.position.segment_icount,
                            )
                        })
                        .unwrap(),
                )
            })
            .unwrap();
        let b_info = table
            .with(b.slot_id, |actor| {
                (
                    actor.tid(),
                    actor
                        .with_runtime(|runtime| {
                            (
                                dh_vmm::run::current_tid(),
                                runtime.counter.is_some(),
                                runtime.position.segment_icount,
                            )
                        })
                        .unwrap(),
                )
            })
            .unwrap();

        assert_eq!(a_info.0, a_info.1 .0);
        assert_eq!(b_info.0, b_info.1 .0);
        assert_ne!(a_info.0, b_info.0);
        assert!(a_info.1 .1);
        assert!(b_info.1 .1);
        assert_eq!(a_info.1 .2, 0);
        assert_eq!(b_info.1 .2, 0);
    }

    #[cfg(target_arch = "x86_64")]
    fn runtime_status_detail(status: &Status) -> proto::ErrorDetail {
        <proto::ErrorDetail as prost::Message>::decode(status.details()).unwrap()
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn runtime_errors_map_to_api_status_details() {
        let cases = [
            (
                RuntimeError::NoSuchSlot(7),
                "runtime_no_such_slot",
                "runtime slot 7 does not exist",
            ),
            (
                RuntimeError::Empty { slot_id: 7 },
                "runtime_missing",
                "runtime slot 7 is empty",
            ),
            (
                RuntimeError::Occupied { slot_id: 7 },
                "runtime_occupied",
                "runtime slot 7 is occupied",
            ),
        ];
        for (err, code, message) in cases {
            let status = runtime_error_to_status(err);
            assert_eq!(status.code(), tonic::Code::FailedPrecondition);
            assert_eq!(status.message(), message);
            let detail = runtime_status_detail(&status);
            assert_eq!(detail.slot_id, 7);
            assert_eq!(detail.code, code);
        }
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
    async fn allocated_runtime_publish_revalidates_ttl_before_returning_lease() {
        if !runtime_tests_available() {
            return;
        }
        let mut config = test_config(1);
        config.lease_policy = LeasePolicy::with_ttl(1);
        let svc = WorkerService::new(config).unwrap();
        let err = svc
            .install_allocated_runtime("CreateVm", |_| {
                std::thread::sleep(std::time::Duration::from_millis(20));
                make_runtime(0x12, SlotPosition::default(), None)
            })
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert_eq!(runtime_status_detail(&err).code, "lease_expired");
        assert_eq!(svc.runtime_table().occupied_count(), 0);
        assert_eq!(svc.slot_manager().list()[0].state, dh_vmm::SlotState::Empty);
    }

    #[cfg(target_arch = "x86_64")]
    #[tokio::test]
    async fn allocated_runtime_insert_failure_preserves_existing_runtime_entry() {
        if !runtime_tests_available() {
            return;
        }
        let svc = WorkerService::new(test_config(1)).unwrap();
        let existing_position = SlotPosition {
            cumulative_icount: 77,
            ..SlotPosition::default()
        };
        svc.runtime_table()
            .insert(0, make_actor(0, 0x13, existing_position, None).unwrap())
            .unwrap();

        let err = svc
            .install_allocated_runtime("CreateVm", |_| {
                make_runtime(0x14, SlotPosition::default(), None)
            })
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert_eq!(runtime_status_detail(&err).code, "runtime_occupied");
        assert_eq!(svc.runtime_table().occupied_count(), 1);
        assert_eq!(
            svc.runtime_table()
                .with(0, |actor| actor
                    .with_runtime(|runtime| runtime.position.cumulative_icount)
                    .unwrap())
                .unwrap(),
            existing_position.cumulative_icount
        );
        assert_eq!(svc.slot_manager().list()[0].state, dh_vmm::SlotState::Empty);
    }

    #[cfg(target_arch = "x86_64")]
    #[tokio::test]
    async fn fork_validates_manager_lease_before_runtime_presence() {
        let svc = WorkerService::new(test_config(2)).unwrap();
        let parent = svc.slot_manager().allocate(0).unwrap();
        let stale = Lease {
            slot_id: parent.slot_id,
            token: [0xFF; 16],
        };
        let err = svc
            .install_forked_runtimes(
                stale,
                1,
                |_table, _leases| -> Result<Vec<SlotRuntime>, Status> {
                    unreachable!("runtime table must not be consulted for stale fork leases")
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert_eq!(runtime_status_detail(&err).code, "stale_lease");
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
    async fn fork_rejects_parent_that_advanced_within_segment() {
        if !runtime_tests_available() {
            return;
        }
        let svc = WorkerService::new(test_config(2)).unwrap();
        let parent = svc
            .install_allocated_runtime("CreateVm", |_| {
                make_runtime(
                    0x23,
                    SlotPosition {
                        cumulative_icount: 100,
                        segment_icount: 100,
                        vns: 100,
                        epoch_index: 0,
                        frame_counter: 0,
                    },
                    Some(snapstore_types::SnapshotRef::from_bytes([0x23; 32])),
                )
            })
            .await
            .unwrap();

        let err = svc
            .fork(Request::new(proto::ForkRequest {
                parent: Some(lease_to_proto(&parent)),
                count: 1,
                entropy_seeds: vec![],
            }))
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert!(err.message().contains("take a snapshot before forking"));
    }

    #[cfg(target_arch = "x86_64")]
    #[tokio::test]
    async fn fork_insert_many_failure_preserves_existing_runtime_entry() {
        if !runtime_tests_available() {
            return;
        }
        let svc = WorkerService::new(test_config(2)).unwrap();
        let parent = svc
            .install_allocated_runtime("CreateVm", |_| {
                make_runtime(0x24, SlotPosition::default(), None)
            })
            .await
            .unwrap();
        let existing_position = SlotPosition {
            cumulative_icount: 444,
            ..SlotPosition::default()
        };
        svc.runtime_table()
            .insert(1, make_actor(1, 0x25, existing_position, None).unwrap())
            .unwrap();

        let err = svc
            .install_forked_runtimes(parent.clone(), 1, |_table, _leases| {
                Ok(vec![make_runtime(0x26, SlotPosition::default(), None)?])
            })
            .await
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert_eq!(runtime_status_detail(&err).code, "runtime_occupied");
        assert_eq!(svc.runtime_table().occupied_count(), 2);
        assert_eq!(
            svc.runtime_table()
                .with(1, |actor| actor
                    .with_runtime(|runtime| runtime.position.cumulative_icount)
                    .unwrap())
                .unwrap(),
            existing_position.cumulative_icount
        );
        let slots = svc.slot_manager().list();
        assert_eq!(
            slots[parent.slot_id as usize].state,
            dh_vmm::SlotState::Paused
        );
        assert_eq!(slots[parent.slot_id as usize].live_children, 0);
        assert_eq!(slots[1].state, dh_vmm::SlotState::Empty);
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
