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
    machine_config_to_proto,
};
#[cfg(target_arch = "x86_64")]
use crate::runtime::{RuntimeError, RuntimeThreadState, SlotRuntime, WorkerRuntimeTable};
use crate::slot_manager::{parse_core_list, Lease, LeasePolicy, SlotError, SlotManager};
use dh_proto::v1 as proto;
use dh_proto::v1::hypervisor_worker_server::{HypervisorWorker, HypervisorWorkerServer};
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

pub const DEFAULT_TCP_ADDR: &str = "0.0.0.0:7400";
pub const DEFAULT_UDS_PATH: &str = "/run/dh/grpc.sock";
#[cfg(target_arch = "x86_64")]
pub const DEFAULT_SNAPSTORE_TCP: &str = "http://127.0.0.1:7410";

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
fn fork_engine_error_to_status(e: crate::fork_engine::ForkError) -> Status {
    use crate::fork_engine::ForkError;
    match e {
        ForkError::AgendaNotEmpty | ForkError::ParentNotFrozen { .. } => {
            Status::failed_precondition(format!("{e:?}"))
        }
        ForkError::Capture(m) | ForkError::Apply(m) => Status::data_loss(m),
        ForkError::Kvm(m) => Status::failed_precondition(m),
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
            if let Err(e) = runtimes.insert(lease.slot_id, runtime) {
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
            let entries = child_leases
                .iter()
                .map(|lease| lease.slot_id)
                .zip(child_runtimes)
                .collect();
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
                    let bus = build_bus(&config, assets.base_image)?;
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
                    let mut bus = build_bus(&config, assets.base_image)?;
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
            let (config, state_hash, frame_counter) = self
                .inner
                .runtimes
                .with(lease.slot_id, |runtime| {
                    (
                        machine_config_to_proto(&runtime.machine_config),
                        runtime.state_hash(),
                        runtime.position.frame_counter,
                    )
                })
                .map_err(runtime_error_to_status)?;
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
                    table
                        .with_mut(parent.slot_id, |parent_runtime| {
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
                            let parent_boundary = parent_runtime.boundary_state(true);
                            let mut out = Vec::with_capacity(entropy_seeds.len());
                            for seed in entropy_seeds {
                                let assets = image_resolver
                                    .resolve_create_vm(&parent_runtime.machine_config)
                                    .map_err(image_error_to_status)?;
                                let mut child_bus =
                                    build_bus(&parent_runtime.machine_config, assets.base_image)?;
                                let forked = crate::fork_engine::fork_slot(
                                    &sys,
                                    &parent_runtime.slot,
                                    dh_vmm::SlotState::Frozen,
                                    &parent_runtime.bus,
                                    &parent_runtime.entropy,
                                    &parent_runtime.machine_config,
                                    parent_boundary,
                                    seed,
                                    &mut child_bus,
                                    None,
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
                        })
                        .map_err(runtime_error_to_status)?
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
        request: Request<proto::TakeSnapshotRequest>,
    ) -> Result<Response<proto::TakeSnapshotResponse>, Status> {
        #[cfg(target_arch = "x86_64")]
        {
            let request = request.into_inner();
            if request.capture.is_some() {
                return Err(unimplemented_status("TakeSnapshot capture"));
            }
            let lease = lease_from_proto(request.lease)?;
            let seal_input_log = request.seal_input_log.unwrap_or(true);
            let store = self.store()?;
            let manager = self.inner.manager.clone();
            let runtimes = self.inner.runtimes.clone();
            let class = self.inner.class.clone();
            let snapshot = blocking_lifecycle("TakeSnapshot", move || {
                let store = store
                    .lock()
                    .map_err(|_| Status::internal("snapshot-store client mutex poisoned"))?;
                let now_ms = lease_now_ms();
                manager
                    .validate(&lease, now_ms)
                    .map_err(slot_error_to_status)?;
                let slot_state = manager
                    .slot_info(lease.slot_id)
                    .map_err(slot_error_to_status)?
                    .state;
                runtimes
                    .with_mut(lease.slot_id, |runtime| {
                        let boundary = runtime.boundary_state(true);
                        let segment_icount = runtime.position.segment_icount;
                        let segment_vns =
                            segment_vns_from_icount(&runtime.machine_config, segment_icount)?;
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
                                    .map_err(|e| {
                                        Status::data_loss(format!("seal DHILOG: {e:?}"))
                                    })?;
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
                        let next_log = new_segment_log(
                            &runtime.machine_config,
                            Some(&out.snapshot_ref),
                            [0; 32],
                        )
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
                            runtime.position.frame_counter,
                            boundary.icount,
                            boundary.vns,
                        ))
                    })
                    .map_err(runtime_error_to_status)?
            })
            .await?;
            let (out, machine_config_hash, input_log_id, frame_counter, icount, vns) = snapshot;
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
                feature_bytes: Vec::new(),
                fb_lz4: Vec::new(),
                fb_info: None,
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
    fn take_snapshot_defaults_to_sealing_and_rejects_capture() {
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

            let capture_err = svc
                .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                    lease: Some(lease.clone()),
                    seal_input_log: Some(true),
                    capture: Some(proto::CaptureSpec::default()),
                }))
                .await
                .unwrap_err();
            assert_eq!(capture_err.code(), tonic::Code::Unimplemented);

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
            .insert(0, make_runtime(0x13, existing_position, None).unwrap())
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
                .with(0, |runtime| runtime.position.cumulative_icount)
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
            .insert(1, make_runtime(0x25, existing_position, None).unwrap())
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
                .with(1, |runtime| runtime.position.cumulative_icount)
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
