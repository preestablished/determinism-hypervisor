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
use crate::replay_engine::ReplayError;
#[cfg(target_arch = "x86_64")]
use crate::runtime::{
    runtime_hash_device_sections, DrainedGuestEvent, QueuedInput, QueuedInputAt, QueuedInputKind,
    RuntimeActorError, RuntimeError, RuntimeThreadState, SlotActor, SlotRuntime,
    WorkerRuntimeTable,
};
use crate::slot_manager::{parse_core_list, Lease, LeasePolicy, SlotError, SlotManager};
use dh_proto::v1 as proto;
use dh_proto::v1::hypervisor_worker_server::{HypervisorWorker, HypervisorWorkerServer};
#[cfg(target_arch = "x86_64")]
use dh_verify::verify::{BisectionMode, VerifyProgress};
use prost::Message;
use std::collections::BTreeMap;
use std::convert::TryFrom;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
#[cfg(target_arch = "x86_64")]
use std::sync::Mutex;
use std::time::{Duration, Instant};
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
#[cfg(target_arch = "x86_64")]
const MAX_READ_GUEST_MEMORY_BYTES: usize = 16 * 1024 * 1024;
#[cfg(target_arch = "x86_64")]
const FRAMEBUFFER_DESCRIPTOR_BYTES: usize = 16;
#[cfg(target_arch = "x86_64")]
const MAX_RETAINED_GUEST_EVENTS_PER_SLOT: usize = 1024;

pub const DEFAULT_TCP_ADDR: &str = "0.0.0.0:7400";
pub const DEFAULT_HTTP_ADDR: &str = "0.0.0.0:7401";
pub const DEFAULT_UDS_PATH: &str = "/run/dh/grpc.sock";
#[cfg(target_arch = "x86_64")]
pub const DEFAULT_SNAPSTORE_TCP: &str = "http://127.0.0.1:7410";
#[cfg(target_arch = "x86_64")]
const VERIFY_REPLAY_INLINE_LOG_MAX_BYTES: usize = 4 * 1024 * 1024;
#[cfg(target_arch = "x86_64")]
const VERIFY_REPLAY_PROGRESS_BUFFER: usize = 16;

pub const ARCH_S9_METRIC_FAMILIES: &[&str] = &[
    "dh_worker_slot_icount",
    "dh_worker_slot_icount_rate",
    "dh_worker_exits_total",
    "dh_worker_landing_single_steps_total",
    "dh_worker_snapshot_duration_milliseconds",
    "dh_worker_fork_duration_milliseconds",
    "dh_worker_restore_duration_milliseconds",
    "dh_worker_snapshot_dirty_pages",
    "dh_worker_verification_failures_total",
    "dh_pmi_skid_instructions",
];

const EXIT_REASON_LABELS: &[&str] = &[
    "debug",
    "dirty_ring_full",
    "fail_entry",
    "hlt",
    "internal_error",
    "io_in",
    "io_out",
    "irq_window_open",
    "mmio_read",
    "mmio_write",
    "shutdown",
    "system_event",
    "unknown",
    "x86_rdmsr",
    "x86_wrmsr",
];

type ResponseStream<T> =
    Pin<Box<dyn tonic::codegen::tokio_stream::Stream<Item = Result<T, Status>> + Send + 'static>>;

#[cfg(target_arch = "x86_64")]
#[doc(hidden)]
pub mod boot_observer {
    use std::sync::atomic::{AtomicU64, Ordering};

    // Process-local diagnostic counters used by ignored Linux lifecycle tests.
    static ELF_LOADS: AtomicU64 = AtomicU64::new(0);
    static BZIMAGE_LOADS: AtomicU64 = AtomicU64::new(0);

    pub fn reset() {
        ELF_LOADS.store(0, Ordering::SeqCst);
        BZIMAGE_LOADS.store(0, Ordering::SeqCst);
    }

    pub fn elf_loads() -> u64 {
        ELF_LOADS.load(Ordering::SeqCst)
    }

    pub fn bzimage_loads() -> u64 {
        BZIMAGE_LOADS.load(Ordering::SeqCst)
    }

    pub(crate) fn record_elf_load() {
        ELF_LOADS.fetch_add(1, Ordering::SeqCst);
    }

    pub(crate) fn record_bzimage_load() {
        BZIMAGE_LOADS.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Clone, Debug)]
pub struct WorkerConfig {
    pub worker_id: String,
    pub slot_cores: Vec<u32>,
    pub lease_policy: LeasePolicy,
    pub class: proto::DeterminismClass,
    pub preflight: PreflightHealth,
    #[cfg(target_arch = "x86_64")]
    pub image_cache_dir: PathBuf,
    #[cfg(target_arch = "x86_64")]
    pub snapstore: Option<snapstore_client::Transport>,
    #[cfg(target_arch = "x86_64")]
    pub bisection_checkpoints: BisectionCheckpointConfig,
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
            preflight: PreflightHealth::skipped("preflight not run by this process"),
            #[cfg(target_arch = "x86_64")]
            image_cache_dir: crate::image_resolver::DEFAULT_IMAGE_CACHE_DIR.into(),
            #[cfg(target_arch = "x86_64")]
            snapstore: Some(snapstore_client::Transport::Tcp(
                DEFAULT_SNAPSTORE_TCP.into(),
            )),
            #[cfg(target_arch = "x86_64")]
            bisection_checkpoints: BisectionCheckpointConfig::default(),
        })
    }
}

/// Private recorder-side bisection checkpoint controls.
///
/// When enabled, the recorder captures one full, parentless checkpoint
/// snapshot at each recorded epoch hash boundary whose vCPU state is
/// directly restoreable and appends a BISECTION_CHECKPOINT AUX record at
/// the same segment-relative icount. The cadence is deterministic because
/// it is inherited from `MachineConfig.epoch_len`; epochs that carry
/// transient run-control vector state are skipped and the next checkpoint
/// reports the widened gap since the previous checkpoint or segment start.
/// This requires `MachineConfig.hash_epochs == EpochsOn` and a configured
/// snapstore.
#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BisectionCheckpointConfig {
    pub enabled: bool,
}

#[cfg(target_arch = "x86_64")]
impl BisectionCheckpointConfig {
    pub const fn disabled() -> Self {
        Self { enabled: false }
    }

    pub const fn every_epoch() -> Self {
        Self { enabled: true }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreflightHealth {
    ok: bool,
    status: String,
    detail: Vec<String>,
}

impl PreflightHealth {
    pub fn passed(results: &[crate::preflight::CheckResult]) -> Self {
        Self {
            ok: true,
            status: "passed".into(),
            detail: results.iter().map(ToString::to_string).collect(),
        }
    }

    pub fn failed(results: &[crate::preflight::CheckResult]) -> Self {
        Self {
            ok: false,
            status: "failed".into(),
            detail: results.iter().map(ToString::to_string).collect(),
        }
    }

    pub fn skipped(reason: impl Into<String>) -> Self {
        Self {
            ok: true,
            status: "skipped".into(),
            detail: vec![reason.into()],
        }
    }

    pub fn is_healthy(&self) -> bool {
        self.ok
    }

    fn healthz_body(&self) -> String {
        let mut out = String::new();
        out.push_str(if self.ok { "ok\n" } else { "failed\n" });
        out.push_str("preflight=");
        out.push_str(&self.status);
        out.push('\n');
        for line in &self.detail {
            out.push_str(line);
            out.push('\n');
        }
        out
    }
}

#[derive(Default)]
struct WorkerMetrics {
    exits: Mutex<BTreeMap<(u64, &'static str), u64>>,
    slot_icount_samples: Mutex<BTreeMap<u64, (u64, Instant)>>,
    verification_failures_total: std::sync::atomic::AtomicU64,
    snapshot_ms: Mutex<MetricHistogram>,
    fork_ms: Mutex<MetricHistogram>,
    restore_ms: Mutex<MetricHistogram>,
    snapshot_dirty_pages: Mutex<MetricHistogram>,
}

impl WorkerMetrics {
    fn record_exit(&self, slot_id: u64, reason: &'static str) {
        let mut exits = self.exits.lock().expect("metrics mutex poisoned");
        *exits.entry((slot_id, reason)).or_insert(0) += 1;
    }

    fn observe_snapshot(&self, elapsed: Duration, dirty_pages: u64) {
        self.snapshot_ms
            .lock()
            .expect("metrics mutex poisoned")
            .observe(elapsed.as_secs_f64() * 1000.0, MS_BUCKETS);
        self.snapshot_dirty_pages
            .lock()
            .expect("metrics mutex poisoned")
            .observe(dirty_pages as f64, DIRTY_PAGE_BUCKETS);
    }

    fn observe_fork(&self, elapsed: Duration) {
        self.fork_ms
            .lock()
            .expect("metrics mutex poisoned")
            .observe(elapsed.as_secs_f64() * 1000.0, MS_BUCKETS);
    }

    fn observe_restore(&self, elapsed: Duration) {
        self.restore_ms
            .lock()
            .expect("metrics mutex poisoned")
            .observe(elapsed.as_secs_f64() * 1000.0, MS_BUCKETS);
    }

    fn record_verification_failure(&self) {
        self.verification_failures_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    fn render(&self, manager: &SlotManager) -> String {
        let mut out = String::new();
        let slots = manager.list();
        push_help_type(
            &mut out,
            "dh_worker_slot_icount",
            "Current canonical retired-instruction boundary by slot.",
            "gauge",
        );
        for slot in &slots {
            out.push_str(&format!(
                "dh_worker_slot_icount{{slot_id=\"{}\"}} {}\n",
                slot.slot_id, slot.icount
            ));
        }
        push_help_type(
            &mut out,
            "dh_worker_slot_icount_rate",
            "Observed per-slot canonical retired-instruction rate between metrics scrapes.",
            "gauge",
        );
        let now = Instant::now();
        let mut samples = self
            .slot_icount_samples
            .lock()
            .expect("metrics mutex poisoned");
        for slot in &slots {
            let rate = samples
                .get(&slot.slot_id)
                .and_then(|(last_icount, last_at)| {
                    let elapsed = now.saturating_duration_since(*last_at).as_secs_f64();
                    (elapsed > 0.0 && slot.icount >= *last_icount)
                        .then_some((slot.icount - *last_icount) as f64 / elapsed)
                })
                .unwrap_or(0.0);
            samples.insert(slot.slot_id, (slot.icount, now));
            out.push_str(&format!(
                "dh_worker_slot_icount_rate{{slot_id=\"{}\"}} {}\n",
                slot.slot_id,
                format_metric_float(rate)
            ));
        }
        drop(samples);

        push_help_type(
            &mut out,
            "dh_worker_exits_total",
            "KVM exits handled by dh-workerd, partitioned by slot and reason.",
            "counter",
        );
        let exits = self.exits.lock().expect("metrics mutex poisoned");
        for slot in &slots {
            for reason in EXIT_REASON_LABELS {
                let value = exits.get(&(slot.slot_id, *reason)).copied().unwrap_or(0);
                out.push_str(&format!(
                    "dh_worker_exits_total{{slot_id=\"{}\",reason=\"{}\"}} {}\n",
                    slot.slot_id, reason, value
                ));
            }
        }

        push_help_type(
            &mut out,
            "dh_worker_landing_single_steps_total",
            "Boundary-engine single-step refinements.",
            "counter",
        );
        out.push_str(&format!(
            "dh_worker_landing_single_steps_total {}\n",
            dh_vmm::boundary::landing_single_steps_total()
        ));

        self.snapshot_ms
            .lock()
            .expect("metrics mutex poisoned")
            .render(
                &mut out,
                "dh_worker_snapshot_duration_milliseconds",
                "Snapshot commit latency in milliseconds.",
                MS_BUCKETS,
            );
        self.fork_ms.lock().expect("metrics mutex poisoned").render(
            &mut out,
            "dh_worker_fork_duration_milliseconds",
            "Tier-A fork latency in milliseconds.",
            MS_BUCKETS,
        );
        self.restore_ms
            .lock()
            .expect("metrics mutex poisoned")
            .render(
                &mut out,
                "dh_worker_restore_duration_milliseconds",
                "Restore latency in milliseconds.",
                MS_BUCKETS,
            );
        self.snapshot_dirty_pages
            .lock()
            .expect("metrics mutex poisoned")
            .render(
                &mut out,
                "dh_worker_snapshot_dirty_pages",
                "Dirty pages shipped by snapshot commits.",
                DIRTY_PAGE_BUCKETS,
            );

        push_help_type(
            &mut out,
            "dh_worker_verification_failures_total",
            "VerifyReplay failures or divergences.",
            "counter",
        );
        out.push_str(&format!(
            "dh_worker_verification_failures_total {}\n",
            self.verification_failures_total
                .load(std::sync::atomic::Ordering::Relaxed)
        ));

        push_baselined_skid_histogram(&mut out);
        out
    }
}

#[derive(Default)]
struct MetricHistogram {
    count: u64,
    sum: f64,
    bucket_counts: BTreeMap<OrderedF64, u64>,
}

impl MetricHistogram {
    fn observe(&mut self, value: f64, buckets: &[f64]) {
        self.count += 1;
        self.sum += value;
        for &bucket in buckets.iter().filter(|bucket| value <= **bucket) {
            *self.bucket_counts.entry(OrderedF64(bucket)).or_insert(0) += 1;
        }
    }

    fn render(&self, out: &mut String, name: &str, help: &str, buckets: &[f64]) {
        push_help_type(out, name, help, "histogram");
        for &bucket in buckets {
            let count = self
                .bucket_counts
                .get(&OrderedF64(bucket))
                .copied()
                .unwrap_or(0);
            out.push_str(&format!(
                "{name}_bucket{{le=\"{}\"}} {count}\n",
                format_bucket(bucket)
            ));
        }
        out.push_str(&format!("{name}_bucket{{le=\"+Inf\"}} {}\n", self.count));
        out.push_str(&format!("{name}_sum {}\n", format_metric_float(self.sum)));
        out.push_str(&format!("{name}_count {}\n", self.count));
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct OrderedF64(f64);

impl Eq for OrderedF64 {}

impl PartialOrd for OrderedF64 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderedF64 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

const MS_BUCKETS: &[f64] = &[1.0, 2.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0];
const DIRTY_PAGE_BUCKETS: &[f64] = &[0.0, 1.0, 8.0, 64.0, 512.0, 4096.0, 8192.0, 16384.0];

fn push_help_type(out: &mut String, name: &str, help: &str, kind: &str) {
    out.push_str(&format!("# HELP {name} {help}\n"));
    out.push_str(&format!("# TYPE {name} {kind}\n"));
}

fn push_baselined_skid_histogram(out: &mut String) {
    out.push_str(
        "# HELP dh_pmi_skid_instructions Baseline PMI landing skid distribution in retired instructions from docs/ops/skid-histogram-2026-06-10.txt.\n",
    );
    out.push_str("# TYPE dh_pmi_skid_instructions histogram\n");
    out.push_str("dh_pmi_skid_instructions_bucket{le=\"26\"} 1\n");
    out.push_str("dh_pmi_skid_instructions_bucket{le=\"27\"} 16666\n");
    out.push_str("dh_pmi_skid_instructions_bucket{le=\"30\"} 33332\n");
    out.push_str("dh_pmi_skid_instructions_bucket{le=\"31\"} 49997\n");
    out.push_str("dh_pmi_skid_instructions_bucket{le=\"44\"} 49998\n");
    out.push_str("dh_pmi_skid_instructions_bucket{le=\"45\"} 49999\n");
    out.push_str("dh_pmi_skid_instructions_bucket{le=\"79\"} 50000\n");
    out.push_str("dh_pmi_skid_instructions_bucket{le=\"+Inf\"} 50000\n");
    out.push_str("dh_pmi_skid_instructions_sum 1466744\n");
    out.push_str("dh_pmi_skid_instructions_count 50000\n");
}

fn format_bucket(bucket: f64) -> String {
    if bucket.fract() == 0.0 {
        format!("{bucket:.0}")
    } else {
        format_metric_float(bucket)
    }
}

fn format_metric_float(value: f64) -> String {
    let raw = format!("{value:.6}");
    raw.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(target_arch = "x86_64")]
fn vcpu_exit_reason_label(exit: &kvm_ioctls::VcpuExit<'_>) -> &'static str {
    match exit {
        kvm_ioctls::VcpuExit::Debug(_) => "debug",
        kvm_ioctls::VcpuExit::FailEntry(..) => "fail_entry",
        kvm_ioctls::VcpuExit::Hlt => "hlt",
        kvm_ioctls::VcpuExit::InternalError => "internal_error",
        kvm_ioctls::VcpuExit::IoIn(_, _) => "io_in",
        kvm_ioctls::VcpuExit::IoOut(_, _) => "io_out",
        kvm_ioctls::VcpuExit::IrqWindowOpen => "irq_window_open",
        kvm_ioctls::VcpuExit::MmioRead(_, _) => "mmio_read",
        kvm_ioctls::VcpuExit::MmioWrite(_, _) => "mmio_write",
        kvm_ioctls::VcpuExit::Shutdown => "shutdown",
        kvm_ioctls::VcpuExit::SystemEvent(_, _) => "system_event",
        kvm_ioctls::VcpuExit::Unsupported(_) => "unknown",
        kvm_ioctls::VcpuExit::X86Rdmsr(_) => "x86_rdmsr",
        kvm_ioctls::VcpuExit::X86Wrmsr(_) => "x86_wrmsr",
        _ => "unknown",
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
    #[cfg(target_arch = "x86_64")]
    bisection_checkpoints: BisectionCheckpointConfig,
    worker_id: String,
    class: proto::DeterminismClass,
    version: String,
    preflight: PreflightHealth,
    metrics: Arc<WorkerMetrics>,
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
                #[cfg(target_arch = "x86_64")]
                bisection_checkpoints: config.bisection_checkpoints,
                worker_id: config.worker_id,
                class: config.class,
                version: env!("CARGO_PKG_VERSION").into(),
                preflight: config.preflight,
                metrics: Arc::new(WorkerMetrics::default()),
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

    fn healthz_body(&self) -> String {
        self.inner.preflight.healthz_body()
    }

    fn is_healthy(&self) -> bool {
        self.inner.preflight.is_healthy()
    }

    fn metrics_text(&self) -> String {
        self.inner.metrics.render(self.inner.manager.as_ref())
    }
}

pub async fn serve(
    config: WorkerConfig,
    tcp_addr: std::net::SocketAddr,
    uds_path: Option<PathBuf>,
    http_addr: std::net::SocketAddr,
) -> Result<(), ServeError> {
    let service = WorkerService::new(config)?;
    let tcp_service = HypervisorWorkerServer::new(service.clone());
    let tcp = async move {
        Server::builder()
            .add_service(tcp_service)
            .serve(tcp_addr)
            .await
            .map_err(ServeError::Transport)
    };
    let http_service = service.clone();
    let http = async move {
        serve_health_metrics(http_service, http_addr)
            .await
            .map_err(ServeError::Io)
    };

    if let Some(uds_path) = uds_path {
        prepare_uds_path(&uds_path)?;
        let listener = tokio::net::UnixListener::bind(&uds_path)?;
        let incoming = tokio_stream::wrappers::UnixListenerStream::new(listener);
        let uds_service = HypervisorWorkerServer::new(service);
        let uds = async move {
            Server::builder()
                .add_service(uds_service)
                .serve_with_incoming(incoming)
                .await
                .map_err(ServeError::Transport)
        };
        tokio::try_join!(tcp, uds, http)?;
    } else {
        tokio::try_join!(tcp, http)?;
    }
    Ok(())
}

async fn serve_health_metrics(
    service: WorkerService,
    http_addr: std::net::SocketAddr,
) -> Result<(), std::io::Error> {
    let listener = tokio::net::TcpListener::bind(http_addr).await?;
    loop {
        let (stream, _) = listener.accept().await?;
        let service = service.clone();
        tokio::spawn(async move {
            let _ = handle_health_metrics_connection(service, stream).await;
        });
    }
}

async fn handle_health_metrics_connection(
    service: WorkerService,
    mut stream: tokio::net::TcpStream,
) -> Result<(), std::io::Error> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut buf = [0u8; 2048];
    let n = match tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buf)).await {
        Ok(read) => read?,
        Err(_) => return Ok(()),
    };
    let request = String::from_utf8_lossy(&buf[..n]);
    let response = http_response_for_request(&service, &request);
    match tokio::time::timeout(
        Duration::from_secs(5),
        stream.write_all(response.as_bytes()),
    )
    .await
    {
        Ok(write) => write?,
        Err(_) => return Ok(()),
    }
    match tokio::time::timeout(Duration::from_secs(5), stream.shutdown()).await {
        Ok(done) => done,
        Err(_) => Ok(()),
    }
}

fn http_response_for_request(service: &WorkerService, request: &str) -> String {
    let mut parts = request
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();
    match (method, path) {
        ("GET", "/healthz") => {
            let body = service.healthz_body();
            let status = if service.is_healthy() {
                "200 OK"
            } else {
                "503 Service Unavailable"
            };
            http_response(status, "text/plain; charset=utf-8", &body)
        }
        ("GET", "/metrics") => http_response(
            "200 OK",
            "text/plain; version=0.0.4; charset=utf-8",
            &service.metrics_text(),
        ),
        ("GET", "/") => http_response(
            "200 OK",
            "text/plain; charset=utf-8",
            "dh-workerd health endpoints: /healthz /metrics\n",
        ),
        _ => http_response("404 Not Found", "text/plain; charset=utf-8", "not found\n"),
    }
}

fn http_response(status: &str, content_type: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
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
        ImageResolverError::AllocationFailed { .. } => Status::resource_exhausted(e.to_string()),
        ImageResolverError::Io { .. } => Status::unavailable(e.to_string()),
    }
}

#[cfg(target_arch = "x86_64")]
fn resolve_runtime_base_image(
    image_resolver: &ImageResolver,
    config: &dh_vmm::config::MachineConfig,
) -> Result<dh_vmm::blkfile::FileBase, Status> {
    config
        .validate()
        .map_err(ImageResolverError::InvalidConfig)
        .map_err(image_error_to_status)?;
    image_resolver
        .open_base_image(&config.base_image_hash)
        .map(|(_path, base_image)| base_image)
        .map_err(image_error_to_status)
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
        ReplayError::BisectionDivergence(_) => {
            Status::internal("VerifyReplay bisection divergence escaped report translation")
        }
        ReplayError::BisectionPrecondition(m) => Status::failed_precondition(m),
        ReplayError::BisectionCapture(e) => snapshot_engine_error_to_status(e),
        ReplayError::BisectionCompare(e) => {
            Status::data_loss(format!("VerifyReplay bisection snapshot comparison: {e}"))
        }
        ReplayError::NotYetWired(what) => Status::unimplemented(what),
        ReplayError::Cancelled(what) => Status::cancelled(what),
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RunUntil {
    until: dh_vmm::runctl::Until,
    sdk_event_filter: Option<Option<u32>>,
}

#[cfg(target_arch = "x86_64")]
fn until_from_run_request(req: &proto::RunRequest) -> Result<RunUntil, Status> {
    use proto::run_request::Until as WireUntil;
    match req
        .until
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("RunRequest.until is required"))?
    {
        WireUntil::IcountBudget(budget) => Ok(RunUntil {
            until: dh_vmm::runctl::Until::IcountBudget(*budget),
            sdk_event_filter: None,
        }),
        WireUntil::VnsBudget(budget) => Ok(RunUntil {
            until: dh_vmm::runctl::Until::VnsBudget(*budget),
            sdk_event_filter: None,
        }),
        WireUntil::FrameBudget(frames) => Ok(RunUntil {
            until: dh_vmm::runctl::Until::FrameBudget {
                frames: u64::from(*frames),
                hard_cap: hard_icount_cap(req.hard_icount_cap),
            },
            sdk_event_filter: None,
        }),
        WireUntil::NextSdkEvent(filter) => Ok(RunUntil {
            until: dh_vmm::runctl::Until::NextSdkEvent {
                hard_cap: hard_icount_cap(req.hard_icount_cap),
            },
            sdk_event_filter: Some(filter.stream),
        }),
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
fn maybe_hex32(bytes: Option<[u8; 32]>) -> String {
    bytes.as_ref().map(hex32).unwrap_or_else(|| "none".into())
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
                return Err(Status::failed_precondition(
                    "VerifyReplay divergence bisection requires recorded bisection checkpoints; retry without bisection for the coarse epoch verdict",
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
        VerifyProgress::BisectionDivergence(divergence) => {
            if divergence.icount_hi < divergence.icount_lo {
                return Err(Status::internal(
                    "VerifyReplay bisection produced an inverted icount range",
                ));
            }
            if divergence.evidence.coverage_icount_lo != divergence.icount_lo
                || divergence.evidence.coverage_icount_hi != divergence.icount_hi
            {
                return Err(Status::internal(
                    "VerifyReplay bisection range must match its evidence window",
                ));
            }
            if matches!(divergence.evidence.mode, BisectionMode::ReplayVsRecorded)
                && divergence.evidence.expected_checkpoint_ref.is_none()
            {
                return Err(Status::internal(
                    "VerifyReplay replay-vs-recorded bisection lacks an expected checkpoint ref",
                ));
            }
            let mode = match divergence.evidence.mode {
                BisectionMode::ReplayVsReplay => "replay-vs-replay",
                BisectionMode::ReplayVsRecorded => "replay-vs-recorded",
            };
            Msg::Divergence(proto::Divergence {
                first_bad_epoch: divergence.first_bad_epoch.unwrap_or(0),
                icount_lo: divergence.icount_lo,
                icount_hi: divergence.icount_hi,
                rip_expected: divergence.rip_expected,
                rip_actual: divergence.rip_actual,
                reg_diff: divergence.reg_diff,
                diff_page_idx: divergence.diff_page_idx,
                suspected_cause: format!(
                    "{}; evidence_mode={mode}; evidence_window={}..{}; expected_checkpoint_ref={}; actual_probe_ref={}",
                    divergence.suspected_cause,
                    divergence.evidence.coverage_icount_lo,
                    divergence.evidence.coverage_icount_hi,
                    maybe_hex32(divergence.evidence.expected_checkpoint_ref),
                    maybe_hex32(divergence.evidence.actual_probe_ref)
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
    progress_tx: tokio::sync::mpsc::Sender<Result<proto::VerifyReplayProgress, Status>>,
) -> Result<proto::VerifyReplayProgress, Status> {
    let store = snapstore_client::blocking::SnapstoreClient::connect(transport)
        .map_err(|e| store_error_to_status("connect snapstore", e))?;
    let log_bytes = verify_replay_log_bytes(log_input, &store)?;
    let reader = dh_inputlog::reader::LogReader::parse(&log_bytes)
        .map_err(|e| Status::data_loss(format!("DHILOG parse: {e:?}")))?;
    let checkpoint_index = if bisect_on_divergence {
        Some(
            crate::bisection_index::BisectionCheckpointIndex::from_reader(&reader).map_err(
                |e| {
                    Status::failed_precondition(format!(
                        "VerifyReplay bisection checkpoint index invalid: {e}"
                    ))
                },
            )?,
        )
    } else {
        None
    };
    let header = reader.header().clone();
    let log_writer = log_writer_from_reader_header(&header);
    drop(reader);

    let config = crate::restore_engine::recover_machine_config(base_snapshot.clone(), &store)
        .map_err(restore_engine_error_to_status)?;
    validate_verify_replay_header_and_bisection_refs(
        &header,
        &base_snapshot,
        &config,
        checkpoint_index.as_ref(),
        |snapshot_ref| {
            let container = store
                .get_snapshot(snapstore_types::SnapshotRef::from_bytes(snapshot_ref))
                .map_err(|e| format!("get_snapshot: {e}"))?;
            snapstore_manifest::Manifest::decode(&container)
                .map_err(|e| format!("manifest: {e}"))?;
            Ok::<(), String>(())
        },
    )?;
    let base_image = resolve_runtime_base_image(&image_resolver, &config)?;
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
    let _ = config_hash_for_slot(&config, &slot)?;
    let bus = build_bus(&config, base_image, RuntimeVmMem(slot.guest_mem.clone()))?;
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

    let terminal = crate::verify_replay::verify_replay_with_bisection_progress(
        &mut slot,
        rail,
        &config,
        base_snapshot,
        &counter,
        &store,
        &log_bytes,
        if bisect_on_divergence {
            checkpoint_index.as_ref()
        } else {
            None
        },
        |event| {
            let progress = verify_progress_to_proto(event, bisect_on_divergence).map_err(|e| {
                ReplayError::Apply(format!(
                    "VerifyReplay progress translation failed with {}: {}",
                    e.code(),
                    e.message()
                ))
            })?;
            progress_tx
                .blocking_send(Ok(progress))
                .map_err(|_| ReplayError::Cancelled("VerifyReplay client cancelled"))
        },
    )
    .map_err(replay_error_to_status)?;
    verify_progress_to_proto(terminal, bisect_on_divergence)
}

#[cfg(target_arch = "x86_64")]
fn validate_verify_replay_header_and_bisection_refs<F, E>(
    header: &dh_inputlog::reader::Header,
    base_snapshot: &snapstore_types::SnapshotRef,
    config: &dh_vmm::config::MachineConfig,
    checkpoint_index: Option<&crate::bisection_index::BisectionCheckpointIndex>,
    validate_ref: F,
) -> Result<(), Status>
where
    F: FnMut([u8; 32]) -> Result<(), E>,
    E: std::fmt::Display,
{
    verify_log_header_matches_request(header, base_snapshot, config)?;
    if let Some(index) = checkpoint_index {
        index.validate_snapshot_refs(validate_ref).map_err(|e| {
            Status::failed_precondition(format!(
                "VerifyReplay bisection checkpoint ref unusable: {e}"
            ))
        })?;
    }
    Ok(())
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
fn config_hash_for_slot(
    config: &dh_vmm::config::MachineConfig,
    slot: &dh_vmm::kvm::SlotVm,
) -> Result<[u8; 32], Status> {
    if config.cpuid_table != slot.cpuid_table {
        return Err(Status::invalid_argument(format!(
            "MachineConfig cpuid_table does not match masked KVM CPUID table installed on vCPU \
             (got {} leaves, expected {})",
            config.cpuid_table.len(),
            slot.cpuid_table.len()
        )));
    }
    config
        .config_hash()
        .map_err(|e| Status::invalid_argument(format!("MachineConfig hash: {e:?}")))
}

#[cfg(target_arch = "x86_64")]
fn runtime_with_log(
    slot: dh_vmm::kvm::SlotVm,
    bus: dh_devices::MmioBus,
    lapic: dh_vmm::lapic::LocalApic,
    entropy: dh_devices::entropy::DetEntropy,
    config: dh_vmm::config::MachineConfig,
    chain: dh_vmm::hash::StateHashChain,
    base_snapshot: Option<snapstore_types::SnapshotRef>,
    position: crate::runtime::SlotPosition,
    entropy_seed: [u8; 32],
) -> Result<SlotRuntime, Status> {
    let _ = config_hash_for_slot(&config, &slot)?;
    dh_vmm::dirty::enable_dirty_logging(&slot)
        .map_err(|e| kvm_error_to_status("enable dirty logging", e))?;
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
    runtime.lapic = lapic;
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
    boot_slot_with_loaders(
        slot,
        boot,
        |slot, kernel, cmdline| {
            dh_vmm::boot::load_and_enter(slot, kernel, cmdline)
                .map(|_| ())
                .map_err(|e| Status::failed_precondition(format!("ELF boot: {e}")))
        },
        |slot, kernel, initramfs, cmdline| {
            dh_vmm::boot::load_bzimage_and_enter(slot, kernel, initramfs, cmdline)
                .map(|_| ())
                .map_err(|e| Status::failed_precondition(format!("BzImage boot: {e}")))
        },
    )
}

#[cfg(target_arch = "x86_64")]
fn boot_slot_with_loaders<E, B>(
    slot: &dh_vmm::kvm::SlotVm,
    boot: ResolvedBoot,
    load_elf: E,
    load_bzimage: B,
) -> Result<(), Status>
where
    E: FnOnce(&dh_vmm::kvm::SlotVm, &[u8], &[u8]) -> Result<(), Status>,
    B: FnOnce(&dh_vmm::kvm::SlotVm, &[u8], &[u8], &[u8]) -> Result<(), Status>,
{
    match boot {
        ResolvedBoot::Elf { kernel, cmdline } => {
            boot_observer::record_elf_load();
            load_elf(slot, &kernel, &cmdline)
        }
        ResolvedBoot::BzImage {
            kernel,
            initramfs,
            cmdline,
        } => {
            boot_observer::record_bzimage_load();
            load_bzimage(slot, &kernel, &initramfs, &cmdline)
        }
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
fn reseed_pv_clock_vns_base(bus: &mut dh_devices::MmioBus, vns: u64) -> Result<(), Status> {
    for (_base, dev) in bus.devices_mut() {
        if dev.device_id() == dh_devices::clock::DEVICE_ID_PV_CLOCK {
            let Some(clock) = dev
                .as_any_mut()
                .and_then(|a| a.downcast_mut::<dh_devices::clock::PvClock>())
            else {
                return Err(Status::internal(
                    "pv-clock device does not downcast to PvClock",
                ));
            };
            clock.set_vns_base(vns);
            return Ok(());
        }
    }
    Ok(())
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
fn drained_guest_event_to_runtime(
    ev: detguest_host::GuestEvent,
    icount: u64,
) -> Result<DrainedGuestEvent, dh_vmm::boundary::BoundaryError> {
    let (stream, payload) =
        dh_devices::detchannel::stream_guest_event_payload(&ev).ok_or_else(|| {
            dh_vmm::boundary::BoundaryError::Exit(
                "detchannel guest event could not be encoded for streaming".into(),
            )
        })?;
    Ok(DrainedGuestEvent {
        stream: u32::from(stream),
        icount,
        vns: ev.vnanos,
        payload,
    })
}

#[cfg(target_arch = "x86_64")]
fn cumulative_event_icount(
    start_segment_icount: u64,
    start_cumulative_icount: u64,
    segment_icount: u64,
) -> u64 {
    start_cumulative_icount.saturating_add(segment_icount.saturating_sub(start_segment_icount))
}

#[cfg(target_arch = "x86_64")]
fn drained_guest_events_to_runtime(
    events: Vec<detguest_host::GuestEvent>,
    event_icount: u64,
) -> Result<Vec<DrainedGuestEvent>, dh_vmm::boundary::BoundaryError> {
    events
        .into_iter()
        .map(|ev| drained_guest_event_to_runtime(ev, event_icount))
        .collect()
}

#[cfg(target_arch = "x86_64")]
fn append_guest_events_with_retention_cap(
    retained: &mut Vec<DrainedGuestEvent>,
    events: Vec<DrainedGuestEvent>,
) {
    if events.is_empty() {
        return;
    }
    retained.extend(events);
    trim_guest_events_to_retention_cap(retained);
}

#[cfg(target_arch = "x86_64")]
fn sdk_event_matches(filter: Option<u32>, event: &DrainedGuestEvent) -> bool {
    filter.map_or(true, |stream| event.stream == stream)
}

#[cfg(target_arch = "x86_64")]
fn drained_guest_event_to_proto(event: DrainedGuestEvent) -> proto::GuestEvent {
    proto::GuestEvent {
        stream: event.stream,
        icount: event.icount,
        vns: event.vns,
        payload: event.payload,
    }
}

#[cfg(target_arch = "x86_64")]
fn trim_guest_events_to_retention_cap(events: &mut Vec<DrainedGuestEvent>) {
    let overflow = events
        .len()
        .saturating_sub(MAX_RETAINED_GUEST_EVENTS_PER_SLOT);
    if overflow != 0 {
        events.drain(..overflow);
    }
}

#[cfg(target_arch = "x86_64")]
fn select_stream_guest_events(
    guest_events: &mut Vec<DrainedGuestEvent>,
    streams: &[u32],
) -> Vec<proto::GuestEvent> {
    let stream_filter: std::collections::HashSet<u32> = streams.iter().copied().collect();
    let want_all = stream_filter.is_empty();
    let mut selected = Vec::new();
    let mut retained = Vec::new();
    for event in guest_events.drain(..) {
        if want_all || stream_filter.contains(&event.stream) {
            selected.push(drained_guest_event_to_proto(event));
        } else {
            retained.push(event);
        }
    }
    trim_guest_events_to_retention_cap(&mut retained);
    *guest_events = retained;
    selected
}

#[cfg(target_arch = "x86_64")]
fn service_exit_with_detchannel(
    rail: &mut dh_vmm::recording::DeviceRail<RuntimeVmMem>,
    log_icount: u64,
    event_icount: u64,
    exit: kvm_ioctls::VcpuExit<'_>,
) -> Result<Vec<DrainedGuestEvent>, dh_vmm::boundary::BoundaryError> {
    let serial_end = dh_vmm::kvm::PIO_SERIAL_BASE + dh_vmm::kvm::PIO_SERIAL_LEN;
    let detcall_end = dh_vmm::kvm::PIO_DETCALL_BASE + dh_vmm::kvm::PIO_DETCALL_LEN;
    let mut ctx = dh_devices::DevCtx::new(
        log_icount,
        0,
        &mut rail.log,
        &mut rail.mem,
        &mut rail.entropy,
        &mut rail.irqs,
    );

    let events = match exit {
        kvm_ioctls::VcpuExit::IoOut(port, data)
            if (dh_vmm::kvm::PIO_SERIAL_BASE..serial_end).contains(&port) =>
        {
            rail.serial.pio_write(port, data);
            Vec::new()
        }
        kvm_ioctls::VcpuExit::IoIn(port, data)
            if (dh_vmm::kvm::PIO_SERIAL_BASE..serial_end).contains(&port) =>
        {
            rail.serial.pio_read(port, data);
            Vec::new()
        }
        kvm_ioctls::VcpuExit::MmioRead(gpa, data)
            if dh_vmm::lapic::LocalApic::contains_mmio(gpa) =>
        {
            rail.lapic.read_mmio(gpa, data).map_err(|e| {
                dh_vmm::boundary::BoundaryError::Exit(format!("lapic mmio read {gpa:#x}: {e:?}"))
            })?;
            Vec::new()
        }
        kvm_ioctls::VcpuExit::MmioWrite(gpa, data)
            if dh_vmm::lapic::LocalApic::contains_mmio(gpa) =>
        {
            rail.lapic.write_mmio(gpa, data).map_err(|e| {
                dh_vmm::boundary::BoundaryError::Exit(format!("lapic mmio write {gpa:#x}: {e:?}"))
            })?;
            Vec::new()
        }
        kvm_ioctls::VcpuExit::X86Rdmsr(msr)
            if dh_vmm::lapic::LocalApic::is_lapic_msr(msr.index) =>
        {
            match rail.lapic.read_msr(msr.index) {
                Ok(value) => {
                    *msr.data = value;
                    *msr.error = 0;
                    Vec::new()
                }
                Err(e) => {
                    *msr.error = 1;
                    return Err(dh_vmm::boundary::BoundaryError::Exit(format!(
                        "lapic rdmsr {:#x}: {e:?}",
                        msr.index
                    )));
                }
            }
        }
        kvm_ioctls::VcpuExit::X86Wrmsr(msr)
            if dh_vmm::lapic::LocalApic::is_lapic_msr(msr.index) =>
        {
            match rail.lapic.write_msr(msr.index, msr.data) {
                Ok(()) => {
                    *msr.error = 0;
                    Vec::new()
                }
                Err(e) => {
                    *msr.error = 1;
                    return Err(dh_vmm::boundary::BoundaryError::Exit(format!(
                        "lapic wrmsr {:#x}: {e:?}",
                        msr.index
                    )));
                }
            }
        }
        kvm_ioctls::VcpuExit::X86Rdmsr(msr) => {
            match dh_vmm::msr::on_denied_rdmsr(msr.index) {
                dh_vmm::msr::MsrAction::SupplyValue(value) => {
                    *msr.data = value;
                    *msr.error = 0;
                }
                dh_vmm::msr::MsrAction::AckWrite | dh_vmm::msr::MsrAction::InjectGp => {
                    *msr.error = 1;
                }
            }
            Vec::new()
        }
        kvm_ioctls::VcpuExit::X86Wrmsr(msr) => {
            match dh_vmm::msr::on_denied_wrmsr(msr.index) {
                dh_vmm::msr::MsrAction::SupplyValue(_) | dh_vmm::msr::MsrAction::AckWrite => {
                    *msr.error = 0;
                }
                dh_vmm::msr::MsrAction::InjectGp => {
                    *msr.error = 1;
                }
            }
            Vec::new()
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
            let events = host
                .host_mut()
                .pio_out(port, u32::from_le_bytes(word), &mut ctx);
            if host.host().metrics.any_anomaly() {
                return Err(dh_vmm::boundary::BoundaryError::Exit(
                    "detchannel drain anomaly".into(),
                ));
            }
            drained_guest_events_to_runtime(events, event_icount)?
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
            Vec::new()
        }
        kvm_ioctls::VcpuExit::IoIn(_port, data) => {
            data.fill(0);
            Vec::new()
        }
        kvm_ioctls::VcpuExit::IoOut(_port, _data) => Vec::new(),
        kvm_ioctls::VcpuExit::MmioRead(gpa, data) => {
            rail.bus.read(gpa, data, &mut ctx).map_err(|e| {
                dh_vmm::boundary::BoundaryError::Exit(format!("bus read {gpa:#x}: {e:?}"))
            })?;
            Vec::new()
        }
        kvm_ioctls::VcpuExit::MmioWrite(gpa, data) => {
            rail.bus.write(gpa, data, &mut ctx).map_err(|e| {
                dh_vmm::boundary::BoundaryError::Exit(format!("bus write {gpa:#x}: {e:?}"))
            })?;
            Vec::new()
        }
        other => {
            return Err(dh_vmm::boundary::BoundaryError::Exit(format!(
                "unexpected exit: {other:?}"
            )));
        }
    };
    if let Some(e) = ctx.log_fault() {
        return Err(dh_vmm::boundary::BoundaryError::Exit(format!(
            "log fault: {e:?}"
        )));
    }
    Ok(events)
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
fn checked_introspection_total(
    what: &str,
    current: usize,
    len: u64,
) -> Result<(usize, usize), Status> {
    let len = usize::try_from(len)
        .map_err(|_| Status::invalid_argument(format!("{what} is too large")))?;
    let total = current
        .checked_add(len)
        .ok_or_else(|| Status::invalid_argument("ReadGuestMemory ranges are too large"))?;
    if total > MAX_READ_GUEST_MEMORY_BYTES {
        return Err(Status::invalid_argument(format!(
            "ReadGuestMemory total is {total} bytes, max {MAX_READ_GUEST_MEMORY_BYTES}"
        )));
    }
    Ok((total, len))
}

#[cfg(target_arch = "x86_64")]
fn ensure_paused_slot(
    manager: &SlotManager,
    lease: &Lease,
    method: &str,
) -> Result<crate::slot_manager::SlotInfo, Status> {
    let now_ms = lease_now_ms();
    manager
        .validate(lease, now_ms)
        .map_err(slot_error_to_status)?;
    let info = manager
        .slot_info(lease.slot_id)
        .map_err(slot_error_to_status)?;
    if info.state != dh_vmm::SlotState::Paused {
        return Err(Status::failed_precondition(format!(
            "{method} requires Paused slot, got {:?}",
            info.state
        )));
    }
    Ok(info)
}

#[cfg(target_arch = "x86_64")]
fn drain_runtime_detchannel_at_pause(runtime: &mut SlotRuntime) -> Result<(), Status> {
    let Some(log) = runtime.log.take() else {
        return Err(Status::failed_precondition(
            "slot has no active DHILOG segment",
        ));
    };
    let bus = std::mem::take(&mut runtime.bus);
    let entropy = std::mem::replace(
        &mut runtime.entropy,
        dh_devices::entropy::DetEntropy::from_seed([0; 32]),
    );
    let lapic = std::mem::take(&mut runtime.lapic);
    let mut rail = dh_vmm::recording::DeviceRail::new(
        bus,
        entropy,
        log,
        RuntimeVmMem(runtime.slot.guest_mem.clone()),
    );
    rail.lapic = lapic;
    let log_icount = runtime.position.segment_icount;
    let event_icount = runtime.position.cumulative_icount;
    let result = (|| {
        let Some(host) = runtime_detchannel_mut(&mut rail.bus) else {
            return Ok(Vec::new());
        };
        let mut ctx = dh_devices::DevCtx::new(
            log_icount,
            0,
            &mut rail.log,
            &mut rail.mem,
            &mut rail.entropy,
            &mut rail.irqs,
        );
        let events = host.host_mut().drain_at_pause(&mut ctx);
        if host.host().metrics.any_anomaly() {
            return Err(Status::data_loss("detchannel pause drain anomaly"));
        }
        if let Some(e) = ctx.log_fault() {
            return Err(Status::data_loss(format!(
                "detchannel pause drain log fault: {e:?}"
            )));
        }
        drained_guest_events_to_runtime(events, event_icount)
            .map_err(|e| Status::data_loss(e.to_string()))
    })();
    runtime.bus = rail.bus;
    runtime.lapic = rail.lapic;
    runtime.entropy = rail.entropy;
    runtime.log = Some(rail.log);
    append_guest_events_with_retention_cap(&mut runtime.guest_events, result?);
    Ok(())
}

#[cfg(target_arch = "x86_64")]
fn fault_runtime_after_pause_drain_error(
    manager: &SlotManager,
    runtime: &mut SlotRuntime,
    slot_id: u64,
    status: Status,
) -> Status {
    runtime.thread = RuntimeThreadState::Faulted(format!(
        "detchannel pause drain: {}: {}",
        status.code(),
        status.message()
    ));
    if let Err(fault) = manager.mark_faulted(slot_id) {
        return Status::internal(format!(
            "detchannel pause drain failed with {}: {}; also failed to mark slot faulted: {fault:?}",
            status.code(),
            status.message()
        ));
    }
    status
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy)]
enum FramebufferCaller {
    GetFramebuffer,
    CaptureSpec,
}

#[cfg(target_arch = "x86_64")]
impl FramebufferCaller {
    fn missing_device(self) -> &'static str {
        match self {
            FramebufferCaller::GetFramebuffer => {
                "GetFramebuffer requires DetChannelDevice in machine_config"
            }
            FramebufferCaller::CaptureSpec => {
                "CaptureSpec requires DetChannelDevice in machine_config"
            }
        }
    }

    fn missing_channel(self) -> &'static str {
        match self {
            FramebufferCaller::GetFramebuffer => "GetFramebuffer requires an attached detchannel",
            FramebufferCaller::CaptureSpec => "CaptureSpec requires an attached detchannel",
        }
    }

    fn read_manifest_context(self) -> &'static str {
        match self {
            FramebufferCaller::GetFramebuffer => "read framebuffer manifest",
            FramebufferCaller::CaptureSpec => "read capture manifest",
        }
    }

    fn missing_region(self) -> &'static str {
        match self {
            FramebufferCaller::GetFramebuffer => {
                "GetFramebuffer requested but no framebuffer region is published"
            }
            FramebufferCaller::CaptureSpec => {
                "CaptureSpec.framebuffer requested but no framebuffer region is published"
            }
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn read_framebuffer_region_from_bus(
    bus: &mut dh_devices::MmioBus,
    caller: FramebufferCaller,
) -> Result<Vec<u8>, Status> {
    let detchannel = runtime_detchannel_mut(bus)
        .ok_or_else(|| Status::failed_precondition(caller.missing_device()))?;
    let channel = detchannel
        .host()
        .channel()
        .ok_or_else(|| Status::failed_precondition(caller.missing_channel()))?;
    let manifest = channel.read_manifest().map_err(|e| {
        Status::failed_precondition(format!("{}: {e:?}", caller.read_manifest_context()))
    })?;
    let entry = manifest
        .entries
        .iter()
        .find(|entry| {
            entry.is_live() && entry.flags & detguest_wire::manifest::REGION_FLAG_FRAMEBUFFER != 0
        })
        .ok_or_else(|| Status::failed_precondition(caller.missing_region()))?;
    let name = std::str::from_utf8(entry.name_bytes())
        .map_err(|_| Status::failed_precondition("framebuffer region name is not valid UTF-8"))?;
    let region = manifest
        .resolve(name)
        .ok_or_else(|| Status::failed_precondition("framebuffer region could not be resolved"))?;
    let fb_len = checked_capture_len(
        "framebuffer region",
        region.len,
        MAX_CAPTURE_FRAMEBUFFER_BYTES,
    )?;
    let mut pixels = vec![0u8; fb_len];
    channel
        .read_region(name, 0, &mut pixels)
        .map_err(|e| capture_region_error(name, e))?;
    Ok(pixels)
}

#[cfg(target_arch = "x86_64")]
fn read_framebuffer_from_bus(
    bus: &mut dh_devices::MmioBus,
) -> Result<(u32, u32, u32, i32, Vec<u8>), Status> {
    let region = read_framebuffer_region_from_bus(bus, FramebufferCaller::GetFramebuffer)?;
    framebuffer_response_from_region_bytes(&region)
}

#[cfg(target_arch = "x86_64")]
fn framebuffer_response_from_region_bytes(
    region: &[u8],
) -> Result<(u32, u32, u32, i32, Vec<u8>), Status> {
    let descriptor = region.get(..FRAMEBUFFER_DESCRIPTOR_BYTES).ok_or_else(|| {
        Status::failed_precondition(
            "GetFramebuffer requires a descriptor-bearing framebuffer region",
        )
    })?;
    let width = u32::from_le_bytes(descriptor[0..4].try_into().unwrap());
    let height = u32::from_le_bytes(descriptor[4..8].try_into().unwrap());
    let stride = u32::from_le_bytes(descriptor[8..12].try_into().unwrap());
    let format = u32::from_le_bytes(descriptor[12..16].try_into().unwrap());
    if width == 0 || height == 0 || stride == 0 {
        return Err(Status::failed_precondition(
            "GetFramebuffer framebuffer descriptor has zero dimensions",
        ));
    }
    let format_i32 = i32::try_from(format).map_err(|_| {
        Status::failed_precondition(format!("GetFramebuffer unsupported pixel_format {format}"))
    })?;
    let (format, bytes_per_pixel) = match proto::PixelFormat::try_from(format_i32) {
        Ok(proto::PixelFormat::Xrgb8888) => (proto::PixelFormat::Xrgb8888, 4u64),
        Ok(proto::PixelFormat::Rgb565) => (proto::PixelFormat::Rgb565, 2u64),
        _ => {
            return Err(Status::failed_precondition(format!(
                "GetFramebuffer unsupported pixel_format {format}"
            )));
        }
    };
    let min_stride = u64::from(width)
        .checked_mul(bytes_per_pixel)
        .ok_or_else(|| Status::failed_precondition("framebuffer stride overflows"))?;
    if u64::from(stride) < min_stride {
        return Err(Status::failed_precondition(format!(
            "GetFramebuffer stride {stride} is smaller than width {width} * bytes_per_pixel {bytes_per_pixel}"
        )));
    }
    let pixel_len = u64::from(stride)
        .checked_mul(u64::from(height))
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| Status::failed_precondition("framebuffer pixel length overflows"))?;
    let pixel_end = FRAMEBUFFER_DESCRIPTOR_BYTES
        .checked_add(pixel_len)
        .ok_or_else(|| Status::failed_precondition("framebuffer pixel length overflows"))?;
    let pixels = region
        .get(FRAMEBUFFER_DESCRIPTOR_BYTES..pixel_end)
        .ok_or_else(|| Status::failed_precondition("framebuffer pixels are truncated"))?
        .to_vec();
    Ok((width, height, stride, proto_pixel_format(format), pixels))
}

#[cfg(target_arch = "x86_64")]
fn descriptor_framebuffer_capture(
    region: &[u8],
    frame_counter: u32,
) -> Result<Option<(Vec<u8>, proto::FbInfo)>, Status> {
    if !framebuffer_region_advertises_descriptor(region) {
        return Ok(None);
    }
    let (width, height, stride, format, pixels) = framebuffer_response_from_region_bytes(region)?;
    Ok(Some((
        pixels,
        proto::FbInfo {
            width,
            height,
            stride,
            format,
            frame_counter,
        },
    )))
}

#[cfg(target_arch = "x86_64")]
fn framebuffer_region_advertises_descriptor(region: &[u8]) -> bool {
    let Some(descriptor) = region.get(..FRAMEBUFFER_DESCRIPTOR_BYTES) else {
        return false;
    };
    let width = u32::from_le_bytes(descriptor[0..4].try_into().unwrap());
    let height = u32::from_le_bytes(descriptor[4..8].try_into().unwrap());
    let stride = u32::from_le_bytes(descriptor[8..12].try_into().unwrap());
    let format = u32::from_le_bytes(descriptor[12..16].try_into().unwrap());
    let known_format = matches!(
        i32::try_from(format)
            .ok()
            .and_then(|format| proto::PixelFormat::try_from(format).ok()),
        Some(proto::PixelFormat::PfUnspecified)
            | Some(proto::PixelFormat::Xrgb8888)
            | Some(proto::PixelFormat::Rgb565)
    );
    let plausible_dimensions = width != 0 && height != 0 && stride >= width;
    known_format || plausible_dimensions
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
        let region = read_framebuffer_region_from_bus(bus, FramebufferCaller::CaptureSpec)?;
        match descriptor_framebuffer_capture(&region, frame_counter)? {
            Some((pixels, fb_info)) => {
                out.fb_lz4 = lz4_flex::compress_prepend_size(&pixels);
                out.fb_info = Some(fb_info);
            }
            None => {
                out.fb_lz4 = lz4_flex::compress_prepend_size(&region);
                out.fb_info = Some(proto::FbInfo {
                    width: 0,
                    height: 0,
                    stride: 0,
                    format: proto_pixel_format(proto::PixelFormat::PfUnspecified),
                    frame_counter,
                });
            }
        }
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
fn with_paused_runtime_mut<R>(
    manager: Arc<SlotManager>,
    runtimes: Arc<WorkerRuntimeTable>,
    lease: Lease,
    method: &'static str,
    f: impl FnOnce(&mut SlotRuntime) -> Result<R, Status> + Send + 'static,
) -> Result<R, Status>
where
    R: Send + 'static,
{
    let expected = ensure_paused_slot(&manager, &lease, method)?;
    with_runtime_mut(runtimes.as_ref(), lease.slot_id, move |runtime| {
        let current = ensure_paused_slot(&manager, &lease, method)?;
        if current.icount != expected.icount || runtime.position.cumulative_icount != current.icount
        {
            return Err(Status::aborted(format!(
                "{method} boundary changed before introspection: manager {} -> {}, runtime {}",
                expected.icount, current.icount, runtime.position.cumulative_icount
            )));
        }
        f(runtime)
    })?
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
                    let config_hash = config_hash_for_slot(&config, &slot)?;
                    runtime_with_log(
                        slot,
                        bus,
                        dh_vmm::lapic::LocalApic::new(),
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
            let started = Instant::now();
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
                    let base_image = resolve_runtime_base_image(&image_resolver, &config)?;
                    let sys = dh_vmm::kvm::KvmSystem::open()
                        .map_err(|e| kvm_error_to_status("open KVM", e))?;
                    if !sys.dirty_ring {
                        return Err(Status::failed_precondition("KVM dirty ring unavailable"));
                    }
                    let slot = sys
                        .create_slot_vm(config.mem_bytes)
                        .map_err(|e| kvm_error_to_status("create slot VM", e))?;
                    let _ = config_hash_for_slot(&config, &slot)?;
                    let mut bus =
                        build_bus(&config, base_image, RuntimeVmMem(slot.guest_mem.clone()))?;
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
                        outcome.lapic,
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
            self.inner.metrics.observe_restore(started.elapsed());
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
            let started = Instant::now();
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
                            let base_image = resolve_runtime_base_image(
                                &image_resolver,
                                &parent_runtime.machine_config,
                            )?;
                            let (forked, child_bus) =
                                crate::fork_engine::fork_slot_with_child_bus_with_lapic(
                                    &sys,
                                    &parent_runtime.slot,
                                    dh_vmm::SlotState::Frozen,
                                    &parent_runtime.bus,
                                    &parent_runtime.lapic,
                                    &parent_runtime.entropy,
                                    &parent_runtime.machine_config,
                                    parent_boundary,
                                    seed,
                                    None,
                                    |child| {
                                        build_bus(
                                            &parent_runtime.machine_config,
                                            base_image,
                                            RuntimeVmMem(child.guest_mem.clone()),
                                        )
                                        .map_err(|e| format!("{}: {}", e.code(), e.message()))
                                    },
                                )
                                .map_err(fork_engine_error_to_status)?;
                            out.push(runtime_with_log(
                                forked.child,
                                child_bus,
                                forked.lapic,
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
            self.inner.metrics.observe_fork(started.elapsed());
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
            let run_until = until_from_run_request(&request)?;
            let manager = self.inner.manager.clone();
            let runtimes = self.inner.runtimes.clone();
            let metrics = self.inner.metrics.clone();
            let bisection_checkpoints = self.inner.bisection_checkpoints;
            let checkpoint_store = if bisection_checkpoints.enabled {
                Some(self.store()?)
            } else {
                None
            };
            let response = blocking_lifecycle("Run", move || {
                manager
                    .checkout_write(&lease, "Run", lease_now_ms())
                    .map_err(slot_error_to_status)?;
                with_runtime_mut(runtimes.as_ref(), lease.slot_id, move |runtime| {
                    let tid = dh_vmm::run::current_tid();
                    let start_segment_icount = runtime.position.segment_icount;
                    let start_cumulative_icount = runtime.position.cumulative_icount;
                    let start_vns = runtime.position.vns;
                    let start_cumulative_epoch = runtime.position.epoch_index;
                    let start_segment_vns =
                        segment_vns_from_icount(&runtime.machine_config, start_segment_icount)?;
                    let sdk_event_filter = run_until.sdk_event_filter;
                    let epoch_len = runtime.machine_config.epoch_len.max(1);
                    let start_segment_epoch = start_segment_icount / epoch_len;
                    if bisection_checkpoints.enabled {
                        if runtime.machine_config.hash_epochs != dh_vmm::config::HashEpochs::EpochsOn
                        {
                            return Err(Status::failed_precondition(
                                "bisection checkpoint recording requires hash_epochs=EPOCHS_ON",
                            ));
                        }
                    }
                    let checkpoint_machine_config = bisection_checkpoints
                        .enabled
                        .then(|| runtime.machine_config.clone());
                    let mut checkpoint_anchor_icount =
                        runtime.bisection_checkpoint_anchor_icount;
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
                    let lapic = std::mem::take(&mut runtime.lapic);
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
                    let (
                        run_result,
                        consumed_input_orders,
                        drained_guest_events,
                        first_matching_sdk_event,
                        rail,
                    ) = {
                        let mut rail_inner = dh_vmm::recording::DeviceRail::new(
                            bus,
                            entropy,
                            log,
                            RuntimeVmMem(runtime.slot.guest_mem.clone()),
                        );
                        rail_inner.lapic = lapic;
                        let rail = std::cell::RefCell::new(rail_inner);
                        let mut consumed_input_orders = Vec::new();
                        let mut drained_guest_events = Vec::new();
                        let sdk_event_feed = std::cell::Cell::new(0u64);
                        let mut first_matching_sdk_event: Option<DrainedGuestEvent> = None;
                        let counter_ref = counter;
                        let mut on_exit = |exit: kvm_ioctls::VcpuExit<'_>| {
                            metrics.record_exit(lease.slot_id, vcpu_exit_reason_label(&exit));
                            let icount = counter_ref.read().map_err(|e| {
                                dh_vmm::boundary::BoundaryError::Exit(format!(
                                    "counter read: {e:?}"
                                ))
                            })?;
                            let event_icount = cumulative_event_icount(
                                start_segment_icount,
                                start_cumulative_icount,
                                icount,
                            );
                            let events = service_exit_with_detchannel(
                                &mut rail.borrow_mut(),
                                icount,
                                event_icount,
                                exit,
                            )?;
                            if let Some(filter) = sdk_event_filter {
                                for event in &events {
                                    if sdk_event_matches(filter, event) {
                                        sdk_event_feed.set(sdk_event_feed.get() + 1);
                                        if first_matching_sdk_event.is_none() {
                                            first_matching_sdk_event = Some(event.clone());
                                        }
                                    }
                                }
                            }
                            drained_guest_events.extend(events);
                            Ok(())
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
                        let mut epoch_sink =
                            |epoch_index,
                             boundary: dh_vmm::boundary::Boundary,
                             chain_value,
                             checkpoint_slot: Option<&dh_vmm::kvm::SlotVm>| {
                                let icount = boundary.icount;
                                let pending_checkpoint = if let Some(slot) = checkpoint_slot {
                                    if let Some(machine_config) = checkpoint_machine_config.as_ref()
                                    {
                                        let checkpoint_vns =
                                            segment_vns_from_icount(machine_config, icount)
                                                .map_err(|e| {
                                                    dh_vmm::boundary::BoundaryError::Exit(
                                                        format!(
                                                            "bisection checkpoint vns: {}: {}",
                                                            e.code(),
                                                            e.message()
                                                        ),
                                                    )
                                                })?;
                                        let max_covered_gap = u32::try_from(
                                            icount.saturating_sub(checkpoint_anchor_icount),
                                        )
                                        .map_err(|_| {
                                            dh_vmm::boundary::BoundaryError::Exit(format!(
                                                "bisection checkpoint gap {} exceeds u32",
                                                icount.saturating_sub(checkpoint_anchor_icount)
                                            ))
                                        })?;
                                        let segment_delta =
                                            icount.saturating_sub(start_segment_icount);
                                        let vns_delta =
                                            checkpoint_vns.saturating_sub(start_segment_vns);
                                        let segment_epoch = icount / epoch_len;
                                        let epoch_delta =
                                            segment_epoch.saturating_sub(start_segment_epoch);
                                        let agenda_empty = !pending_inputs.iter().any(|input| {
                                            match input.at {
                                                QueuedInputAt::Icount(at) => at > icount,
                                                QueuedInputAt::Frame(_) => true,
                                            }
                                        });
                                        let checkpoint_boundary =
                                            crate::snapshot_engine::BoundaryState {
                                                icount: start_cumulative_icount
                                                    .saturating_add(segment_delta),
                                                vns: start_vns.saturating_add(vns_delta),
                                                epoch_index: start_cumulative_epoch
                                                    .saturating_add(epoch_delta),
                                                hash_chain: chain_value,
                                                agenda_empty,
                                            };
                                        let rail_ref = rail.borrow();
                                        let store = checkpoint_store
                                            .as_ref()
                                            .ok_or_else(|| {
                                                dh_vmm::boundary::BoundaryError::Exit(
                                                    "bisection checkpoint store missing".into(),
                                                )
                                            })?
                                            .lock()
                                            .map_err(|_| {
                                                dh_vmm::boundary::BoundaryError::Exit(
                                                    "snapshot-store client mutex poisoned".into(),
                                                )
                                            })?;
                                        let checkpoint =
                                            crate::snapshot_engine::capture_bisection_checkpoint_snapshot_with_lapic(
                                            slot,
                                            dh_vmm::SlotState::Paused,
                                            &rail_ref.bus,
                                            &rail_ref.lapic,
                                            &rail_ref.entropy,
                                            machine_config,
                                            checkpoint_boundary,
                                            &store,
                                        )
                                        .map_err(|e| {
                                            dh_vmm::boundary::BoundaryError::Exit(format!(
                                                "bisection checkpoint snapshot: {e:?}"
                                            ))
                                        })?;
                                        Some((
                                            checkpoint.snapshot_ref.to_bytes(),
                                            checkpoint_vns,
                                            max_covered_gap,
                                        ))
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                };
                                rail.borrow_mut()
                                    .log_epoch_hash(epoch_index, icount, chain_value)
                                    .map_err(|e| {
                                        dh_vmm::boundary::BoundaryError::Exit(format!(
                                            "epoch log: {e:?}"
                                        ))
                                    })?;
                                if let Some((
                                    checkpoint_snapshot_ref,
                                    checkpoint_vns,
                                    max_covered_gap,
                                )) = pending_checkpoint
                                {
                                    rail.borrow_mut()
                                        .log_bisection_checkpoint(
                                            icount,
                                            boundary.rip,
                                            max_covered_gap,
                                            checkpoint_snapshot_ref,
                                            checkpoint_vns,
                                        )
                                        .map_err(|e| {
                                            dh_vmm::boundary::BoundaryError::Exit(format!(
                                                "bisection checkpoint log: {e:?}"
                                            ))
                                        })?;
                                    checkpoint_anchor_icount = icount;
                                }
                                Ok(())
                        };
                        let hash_device_sections = || {
                            let rail_ref = rail.borrow();
                            runtime_hash_device_sections(&rail_ref.bus, &rail_ref.lapic)
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
                                sdk_events: sdk_event_filter.map(|_| &sdk_event_feed),
                                hash_device_sections: Some(&hash_device_sections),
                            };
                            dh_vmm::runctl::run_segment_with_scheduled_inputs_frames_and_epochs(
                                &mut segment,
                                run_until.until,
                                &scheduled_input_icounts,
                                &scheduled_frame_inputs,
                                runtime.position.frame_counter,
                                &mut goal,
                                &mut on_exit,
                                &mut input_sink,
                                &mut epoch_sink,
                            )
                        };
                        (
                            run_result,
                            consumed_input_orders,
                            drained_guest_events,
                            first_matching_sdk_event,
                            rail.into_inner(),
                        )
                    };
                    runtime.bus = rail.bus;
                    runtime.lapic = rail.lapic;
                    runtime.entropy = rail.entropy;
                    runtime.log = Some(rail.log);
                    append_guest_events_with_retention_cap(
                        &mut runtime.guest_events,
                        drained_guest_events,
                    );

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
                            runtime.bisection_checkpoint_anchor_icount =
                                checkpoint_anchor_icount;
                            runtime.position.frame_counter =
                                frame_counter_from_bus(&mut runtime.bus);
                            if !consumed_input_orders.is_empty() {
                                runtime
                                    .queued_inputs
                                    .retain(|input| !consumed_input_orders.contains(&input.order));
                            }
                            manager
                                .mark_paused_at_position(
                                    &lease,
                                    cumulative_icount,
                                    runtime
                                        .base_snapshot
                                        .as_ref()
                                        .map(snapstore_types::SnapshotRef::to_bytes),
                                    lease_now_ms(),
                                )
                                .map_err(slot_error_to_status)?;
                            if let Err(e) = drain_runtime_detchannel_at_pause(runtime) {
                                return Err(fault_runtime_after_pause_drain_error(
                                    manager.as_ref(),
                                    runtime,
                                    lease.slot_id,
                                    e,
                                ));
                            }
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
                                sdk_event: if outcome.reason
                                    == dh_vmm::runctl::StopReason::NextSdkEvent
                                {
                                    first_matching_sdk_event.map(drained_guest_event_to_proto)
                                } else {
                                    None
                                },
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
            let started = Instant::now();
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
                    let out = crate::snapshot_engine::take_snapshot_with_lapic(
                        &runtime.slot,
                        slot_state,
                        &runtime.bus,
                        &runtime.lapic,
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
                    if let Err(e) = reseed_pv_clock_vns_base(&mut runtime.bus, boundary.vns) {
                        return Err(fault_runtime_after_snapshot_loss(
                            manager.as_ref(),
                            runtime,
                            lease.slot_id,
                            "TakeSnapshot could not reseed pv-clock",
                            e,
                        ));
                    }
                    let counter_reset = match runtime.counter.as_ref() {
                        Some(counter) => counter.reset(),
                        None => {
                            return Err(fault_runtime_after_snapshot_loss(
                                manager.as_ref(),
                                runtime,
                                lease.slot_id,
                                "TakeSnapshot could not reset segment counter",
                                Status::failed_precondition(
                                    "slot actor has no InstRetired counter",
                                ),
                            ));
                        }
                    };
                    if let Err(e) = counter_reset {
                        return Err(fault_runtime_after_snapshot_loss(
                            manager.as_ref(),
                            runtime,
                            lease.slot_id,
                            "TakeSnapshot could not reset segment counter",
                            Status::data_loss(format!("counter reset: {e:?}")),
                        ));
                    }
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
                    runtime.bisection_checkpoint_anchor_icount = 0;
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
            self.inner
                .metrics
                .observe_snapshot(started.elapsed(), out.pages_shipped.into());
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
        request: Request<proto::ReadGuestMemoryRequest>,
    ) -> Result<Response<proto::ReadGuestMemoryResponse>, Status> {
        #[cfg(target_arch = "x86_64")]
        {
            let request = request.into_inner();
            let lease = lease_from_proto(request.lease)?;
            let manager = self.inner.manager.clone();
            let runtimes = self.inner.runtimes.clone();
            let response = blocking_lifecycle("ReadGuestMemory", move || {
                with_paused_runtime_mut(manager, runtimes, lease, "ReadGuestMemory", move |runtime| {
                    use vm_memory::Bytes;

                    let mut chunks =
                        Vec::with_capacity(request.ranges.len() + request.region_ranges.len());
                    let mut total = 0usize;
                    for (index, range) in request.ranges.iter().enumerate() {
                        range.gpa.checked_add(range.len).ok_or_else(|| {
                            Status::invalid_argument(format!(
                                "ReadGuestMemory.ranges[{index}] overflows"
                            ))
                        })?;
                        let (new_total, len) = checked_introspection_total(
                            &format!("ReadGuestMemory.ranges[{index}]"),
                            total,
                            range.len,
                        )?;
                        total = new_total;
                        let mut chunk = vec![0u8; len];
                        runtime
                            .slot
                            .guest_mem
                            .read_slice(&mut chunk, vm_memory::GuestAddress(range.gpa))
                            .map_err(|e| {
                                Status::invalid_argument(format!(
                                    "ReadGuestMemory.ranges[{index}] read at {:#x}: {e:?}",
                                    range.gpa
                                ))
                            })?;
                        chunks.push(chunk);
                    }

                    if !request.region_ranges.is_empty() {
                        let detchannel = runtime_detchannel_mut(&mut runtime.bus).ok_or_else(|| {
                            Status::failed_precondition(
                                "ReadGuestMemory.region_ranges requires DetChannelDevice in machine_config",
                            )
                        })?;
                        let channel = detchannel.host().channel().ok_or_else(|| {
                            Status::failed_precondition(
                                "ReadGuestMemory.region_ranges requires an attached detchannel",
                            )
                        })?;
                        let manifest = channel.read_manifest().map_err(|e| {
                            Status::failed_precondition(format!("read region manifest: {e:?}"))
                        })?;
                        for (index, range) in request.region_ranges.iter().enumerate() {
                            if range.region.is_empty() {
                                return Err(Status::invalid_argument(format!(
                                    "ReadGuestMemory.region_ranges[{index}].region must not be empty"
                                )));
                            }
                            let region = manifest.resolve(&range.region).ok_or_else(|| {
                                Status::failed_precondition(format!(
                                    "ReadGuestMemory.region_ranges[{index}].region {:?} is not published",
                                    range.region
                                ))
                            })?;
                            if region.layout_version != range.layout_version {
                                return Err(Status::failed_precondition(format!(
                                    "ReadGuestMemory.region_ranges[{index}] layout_version {} != manifest {} for region {:?}",
                                    range.layout_version, region.layout_version, range.region
                                )));
                            }
                            let end = range.offset.checked_add(range.len).ok_or_else(|| {
                                Status::invalid_argument(format!(
                                    "ReadGuestMemory.region_ranges[{index}] overflows"
                                ))
                            })?;
                            if end > region.len {
                                return Err(Status::invalid_argument(format!(
                                    "ReadGuestMemory.region_ranges[{index}] exceeds region {:?} length {}",
                                    range.region, region.len
                                )));
                            }
                            let (new_total, len) = checked_introspection_total(
                                &format!("ReadGuestMemory.region_ranges[{index}]"),
                                total,
                                range.len,
                            )?;
                            total = new_total;
                            let mut chunk = vec![0u8; len];
                            channel
                                .read_region(&range.region, range.offset, &mut chunk)
                                .map_err(|e| capture_region_error(&range.region, e))?;
                            chunks.push(chunk);
                        }
                    }

                    Ok(proto::ReadGuestMemoryResponse {
                        chunks,
                        icount: runtime.position.cumulative_icount,
                    })
                })
            })
            .await?;
            Ok(Response::new(response))
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let _ = request;
            Err(unimplemented_status("ReadGuestMemory"))
        }
    }

    async fn get_framebuffer(
        &self,
        request: Request<proto::GetFramebufferRequest>,
    ) -> Result<Response<proto::GetFramebufferResponse>, Status> {
        #[cfg(target_arch = "x86_64")]
        {
            let request = request.into_inner();
            let lease = lease_from_proto(request.lease)?;
            let manager = self.inner.manager.clone();
            let runtimes = self.inner.runtimes.clone();
            let response = blocking_lifecycle("GetFramebuffer", move || {
                let slot_id = lease.slot_id;
                let manager_for_fault = manager.clone();
                with_paused_runtime_mut(
                    manager,
                    runtimes,
                    lease,
                    "GetFramebuffer",
                    move |runtime| {
                        if let Err(e) = drain_runtime_detchannel_at_pause(runtime) {
                            return Err(fault_runtime_after_pause_drain_error(
                                manager_for_fault.as_ref(),
                                runtime,
                                slot_id,
                                e,
                            ));
                        }
                        let frame_counter = frame_counter_from_bus(&mut runtime.bus);
                        runtime.position.frame_counter = frame_counter;
                        let (width, height, stride, format, pixels) =
                            read_framebuffer_from_bus(&mut runtime.bus)?;
                        Ok(proto::GetFramebufferResponse {
                            width,
                            height,
                            stride,
                            format,
                            frame_counter,
                            icount: runtime.position.cumulative_icount,
                            pixels,
                        })
                    },
                )
            })
            .await?;
            Ok(Response::new(response))
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let _ = request;
            Err(unimplemented_status("GetFramebuffer"))
        }
    }

    async fn stream_guest_events(
        &self,
        request: Request<proto::StreamGuestEventsRequest>,
    ) -> Result<Response<Self::StreamGuestEventsStream>, Status> {
        #[cfg(target_arch = "x86_64")]
        {
            let request = request.into_inner();
            let lease = lease_from_proto(request.lease)?;
            let streams = request.streams;
            let manager = self.inner.manager.clone();
            let runtimes = self.inner.runtimes.clone();
            let events = blocking_lifecycle("StreamGuestEvents", move || {
                let slot_id = lease.slot_id;
                let manager_for_fault = manager.clone();
                with_paused_runtime_mut(
                    manager,
                    runtimes,
                    lease,
                    "StreamGuestEvents",
                    move |runtime| {
                        if let Err(e) = drain_runtime_detchannel_at_pause(runtime) {
                            return Err(fault_runtime_after_pause_drain_error(
                                manager_for_fault.as_ref(),
                                runtime,
                                slot_id,
                                e,
                            ));
                        }
                        Ok(select_stream_guest_events(
                            &mut runtime.guest_events,
                            &streams,
                        ))
                    },
                )
            })
            .await?;
            let stream = tokio_stream::iter(events.into_iter().map(Ok));
            Ok(Response::new(
                Box::pin(stream) as Self::StreamGuestEventsStream
            ))
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let _ = request;
            Err(unimplemented_status("StreamGuestEvents"))
        }
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
            let bisect_on_divergence = request.bisect_on_divergence.unwrap_or(true);
            let transport = self.snapstore_transport()?;
            let image_resolver = self.inner.image_resolver.clone();
            let manager = self.inner.manager.clone();
            let metrics = self.inner.metrics.clone();
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

            let (tx, rx) = tokio::sync::mpsc::channel(VERIFY_REPLAY_PROGRESS_BUFFER);
            let thread_manager = manager.clone();
            let thread_lease = verify_lease.clone();
            let cleanup_manager = manager.clone();
            let cleanup_lease = verify_lease.clone();
            let thread_metrics = metrics.clone();
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
                            tx.clone(),
                        )
                    }))
                    .unwrap_or_else(|_| Err(Status::internal("VerifyReplay thread panicked")));
                    let cleanup = thread_manager
                        .destroy(&thread_lease, lease_now_ms())
                        .map_err(slot_error_to_status);
                    let result = match result {
                        Ok(terminal) => match cleanup {
                            Ok(()) => Ok(terminal),
                            Err(cleanup) => Err(Status::internal(format!(
                                "VerifyReplay succeeded but slot cleanup failed with {}: {}",
                                cleanup.code(),
                                cleanup.message()
                            ))),
                        },
                        Err(e) => Err(original_or_rollback("VerifyReplay", e, cleanup)),
                    };
                    match result {
                        Ok(terminal) => {
                            if matches!(
                                terminal.msg,
                                Some(proto::verify_replay_progress::Msg::Divergence(_))
                            ) {
                                thread_metrics.record_verification_failure();
                            }
                            let _ = tx.blocking_send(Ok(terminal));
                        }
                        Err(e) => {
                            if e.code() != Code::Cancelled {
                                thread_metrics.record_verification_failure();
                            }
                            let _ = tx.blocking_send(Err(e));
                        }
                    }
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
            let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
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
        use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
        use tokio_stream::StreamExt;

        let stream = tokio_stream::wrappers::BroadcastStream::new(self.inner.manager.subscribe())
            .map(|event| match event {
                Ok(slot) => Ok(proto::SlotEvent {
                    slot: Some(slot_info_to_proto(&slot)),
                }),
                Err(BroadcastStreamRecvError::Lagged(n)) => {
                    Err(Status::resource_exhausted(format!(
                        "WatchSlots receiver lagged by {n} slot transitions; resync with ListSlots"
                    )))
                }
            });
        Ok(Response::new(Box::pin(stream) as Self::WatchSlotsStream))
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
            preflight: PreflightHealth::skipped("test config"),
            #[cfg(target_arch = "x86_64")]
            image_cache_dir: std::env::temp_dir(),
            #[cfg(target_arch = "x86_64")]
            snapstore: None,
            #[cfg(target_arch = "x86_64")]
            bisection_checkpoints: BisectionCheckpointConfig::default(),
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
    fn service_test_rail() -> dh_vmm::recording::DeviceRail<RuntimeVmMem> {
        let mem = RuntimeVmMem(
            vm_memory::GuestMemoryMmap::<()>::from_ranges(&[(vm_memory::GuestAddress(0), 0x1000)])
                .unwrap(),
        );
        dh_vmm::recording::DeviceRail::new(
            dh_devices::MmioBus::new(),
            dh_devices::entropy::DetEntropy::from_seed([7; 32]),
            dh_inputlog::dhilog::LogWriter::new(dh_inputlog::dhilog::SegmentHeader {
                base_snapshot_id: [0; 32],
                entropy_seed: [7; 32],
                machine_config_hash: [8; 32],
                clock_num: 1,
                clock_den: 1,
                encoder_fingerprint: 0,
            }),
            mem,
        )
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn linux_lapic_worker_service_handles_apic_exits_before_generic_paths() {
        let mut rail = service_test_rail();

        let mut version = [0u8; 4];
        service_exit_with_detchannel(
            &mut rail,
            0,
            0,
            kvm_ioctls::VcpuExit::MmioRead(dh_vmm::lapic::XAPIC_MMIO_BASE + 0x30, &mut version),
        )
        .unwrap();
        assert_eq!(u32::from_le_bytes(version), (5 << 16) | 0x14);

        let tpr = 0x44u32.to_le_bytes();
        service_exit_with_detchannel(
            &mut rail,
            0,
            0,
            kvm_ioctls::VcpuExit::MmioWrite(dh_vmm::lapic::XAPIC_MMIO_BASE + 0x80, &tpr),
        )
        .unwrap();
        let mut tpr_back = [0u8; 4];
        service_exit_with_detchannel(
            &mut rail,
            0,
            0,
            kvm_ioctls::VcpuExit::MmioRead(dh_vmm::lapic::XAPIC_MMIO_BASE + 0x80, &mut tpr_back),
        )
        .unwrap();
        assert_eq!(u32::from_le_bytes(tpr_back), 0x44);

        let mut read_error = 0u8;
        let mut read_data = 0u64;
        service_exit_with_detchannel(
            &mut rail,
            0,
            0,
            kvm_ioctls::VcpuExit::X86Rdmsr(kvm_ioctls::ReadMsrExit {
                error: &mut read_error,
                reason: kvm_ioctls::MsrExitReason::Unknown,
                index: dh_vmm::msr::MSR_IA32_APIC_BASE,
                data: &mut read_data,
            }),
        )
        .unwrap();
        assert_eq!(read_error, 0);
        assert_eq!(read_data, dh_vmm::lapic::RESET_APIC_BASE_MSR);

        let mut write_error = 0u8;
        service_exit_with_detchannel(
            &mut rail,
            0,
            0,
            kvm_ioctls::VcpuExit::X86Wrmsr(kvm_ioctls::WriteMsrExit {
                error: &mut write_error,
                reason: kvm_ioctls::MsrExitReason::Unknown,
                index: dh_vmm::msr::MSR_IA32_APIC_BASE,
                data: dh_vmm::lapic::RESET_APIC_BASE_MSR,
            }),
        )
        .unwrap();
        assert_eq!(write_error, 0);

        let icr_delivery = 0x40u32.to_le_bytes();
        let err = service_exit_with_detchannel(
            &mut rail,
            0,
            0,
            kvm_ioctls::VcpuExit::MmioWrite(dh_vmm::lapic::XAPIC_MMIO_BASE + 0x300, &icr_delivery),
        )
        .unwrap_err();
        assert!(format!("{err:?}").contains("UnsupportedIcr"));
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn linux_worker_service_applies_denied_msr_policy() {
        let mut rail = service_test_rail();

        let mut read_error = 99u8;
        let mut read_data = u64::MAX;
        service_exit_with_detchannel(
            &mut rail,
            0,
            0,
            kvm_ioctls::VcpuExit::X86Rdmsr(kvm_ioctls::ReadMsrExit {
                error: &mut read_error,
                reason: kvm_ioctls::MsrExitReason::Filter,
                index: dh_vmm::msr::MSR_IA32_MISC_ENABLE,
                data: &mut read_data,
            }),
        )
        .unwrap();
        assert_eq!(read_error, 0);
        assert_eq!(read_data, 0);

        let mut denied_write_error = 0u8;
        service_exit_with_detchannel(
            &mut rail,
            0,
            0,
            kvm_ioctls::VcpuExit::X86Wrmsr(kvm_ioctls::WriteMsrExit {
                error: &mut denied_write_error,
                reason: kvm_ioctls::MsrExitReason::Filter,
                index: dh_vmm::msr::MSR_IA32_TSC,
                data: 1,
            }),
        )
        .unwrap();
        assert_eq!(denied_write_error, 1);

        let mut acked_write_error = 99u8;
        service_exit_with_detchannel(
            &mut rail,
            0,
            0,
            kvm_ioctls::VcpuExit::X86Wrmsr(kvm_ioctls::WriteMsrExit {
                error: &mut acked_write_error,
                reason: kvm_ioctls::MsrExitReason::Filter,
                index: dh_vmm::msr::MSR_IA32_BIOS_SIGN_ID,
                data: 1,
            }),
        )
        .unwrap();
        assert_eq!(acked_write_error, 0);

        let mut ignored_in = [0xAA, 0xBB];
        service_exit_with_detchannel(
            &mut rail,
            0,
            0,
            kvm_ioctls::VcpuExit::IoIn(0x61, &mut ignored_in),
        )
        .unwrap();
        assert_eq!(ignored_in, [0, 0]);

        let ignored_out = [0xCC];
        service_exit_with_detchannel(
            &mut rail,
            0,
            0,
            kvm_ioctls::VcpuExit::IoOut(0xD3, &ignored_out),
        )
        .unwrap();
    }

    #[cfg(target_arch = "x86_64")]
    fn write_cache_blob(root: &Path, bytes: &[u8]) -> [u8; 32] {
        let hash = *blake3::hash(bytes).as_bytes();
        std::fs::write(root.join(crate::image_resolver::cache_key(&hash)), bytes).unwrap();
        hash
    }

    #[cfg(target_arch = "x86_64")]
    fn synthetic_bzimage(payload: &[u8]) -> Vec<u8> {
        const SETUP_SECTS_OFF: usize = 0x1f1;
        const SETUP_HEADER_LEN_OFF: usize = 0x201;
        const HEADER_MAGIC_OFF: usize = 0x202;
        const PROTOCOL_VERSION_OFF: usize = 0x206;
        const LOADFLAGS_OFF: usize = 0x211;
        const INITRD_ADDR_MAX_OFF: usize = 0x22c;
        const KERNEL_ALIGNMENT_OFF: usize = 0x230;
        const RELOCATABLE_KERNEL_OFF: usize = 0x234;
        const XLOADFLAGS_OFF: usize = 0x236;
        const CMDLINE_SIZE_OFF: usize = 0x238;
        const PAYLOAD_OFFSET_OFF: usize = 0x248;
        const PAYLOAD_LENGTH_OFF: usize = 0x24c;
        const PREF_ADDRESS_OFF: usize = 0x258;
        const INIT_SIZE_OFF: usize = 0x260;
        const SETUP_HEADER_END: usize = 0x268;
        const LINUX_64BIT_ENTRY_OFFSET: usize = 0x200;

        let setup_sects = 4u8;
        let setup_bytes = (u64::from(setup_sects) + 1) * 512;
        let payload_offset = 0x400u32;
        let init_size = 0x40_0000u32;
        let total = setup_bytes as usize + payload_offset as usize + payload.len();
        let mut image = vec![0u8; total];
        image[SETUP_SECTS_OFF] = setup_sects;
        image[SETUP_HEADER_LEN_OFF] = (SETUP_HEADER_END - HEADER_MAGIC_OFF) as u8;
        image[0x1fe..0x200].copy_from_slice(&0xaa55u16.to_le_bytes());
        image[0x200..0x202].copy_from_slice(&[0xeb, 0x66]);
        image[HEADER_MAGIC_OFF..HEADER_MAGIC_OFF + 4].copy_from_slice(b"HdrS");
        image[PROTOCOL_VERSION_OFF..PROTOCOL_VERSION_OFF + 2]
            .copy_from_slice(&0x020au16.to_le_bytes());
        image[LOADFLAGS_OFF] = 0x01;
        image[INITRD_ADDR_MAX_OFF..INITRD_ADDR_MAX_OFF + 4]
            .copy_from_slice(&0x37ff_ffffu32.to_le_bytes());
        image[KERNEL_ALIGNMENT_OFF..KERNEL_ALIGNMENT_OFF + 4]
            .copy_from_slice(&0x20_0000u32.to_le_bytes());
        image[RELOCATABLE_KERNEL_OFF] = 1;
        image[XLOADFLAGS_OFF..XLOADFLAGS_OFF + 2].copy_from_slice(&0x0001u16.to_le_bytes());
        image[CMDLINE_SIZE_OFF..CMDLINE_SIZE_OFF + 4]
            .copy_from_slice(&(dh_vmm::config::MAX_CMDLINE as u32).to_le_bytes());
        image[PAYLOAD_OFFSET_OFF..PAYLOAD_OFFSET_OFF + 4]
            .copy_from_slice(&payload_offset.to_le_bytes());
        image[PAYLOAD_LENGTH_OFF..PAYLOAD_LENGTH_OFF + 4]
            .copy_from_slice(&(payload.len() as u32).to_le_bytes());
        image[PREF_ADDRESS_OFF..PREF_ADDRESS_OFF + 8].copy_from_slice(&0x20_0000u64.to_le_bytes());
        image[INIT_SIZE_OFF..INIT_SIZE_OFF + 4].copy_from_slice(&init_size.to_le_bytes());
        let payload_start = setup_bytes as usize + payload_offset as usize;
        image[setup_bytes as usize..payload_start].fill(0x5a);
        image[setup_bytes as usize + LINUX_64BIT_ENTRY_OFFSET] = 0xcc;
        image[payload_start..payload_start + payload.len()].copy_from_slice(payload);
        image
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn image_resolver_errors_map_to_create_vm_status_codes() {
        let path = PathBuf::from("/cache/blob");
        let hash = [0x11; 32];
        let expected = [0x22; 32];
        let actual = [0x33; 32];

        let cases = [
            (
                image_error_to_status(ImageResolverError::InvalidConfig(
                    dh_vmm::config::ConfigError::CmdlineTooLong,
                )),
                tonic::Code::InvalidArgument,
            ),
            (
                image_error_to_status(ImageResolverError::NotFound {
                    kind: crate::image_resolver::ImageBlobKind::Initramfs,
                    hash,
                    path: path.clone(),
                }),
                tonic::Code::FailedPrecondition,
            ),
            (
                image_error_to_status(ImageResolverError::NotFile {
                    kind: crate::image_resolver::ImageBlobKind::BaseImage,
                    path: path.clone(),
                }),
                tonic::Code::FailedPrecondition,
            ),
            (
                image_error_to_status(ImageResolverError::HashMismatch {
                    kind: crate::image_resolver::ImageBlobKind::BaseImage,
                    path: path.clone(),
                    expected,
                    actual,
                }),
                tonic::Code::DataLoss,
            ),
            (
                image_error_to_status(ImageResolverError::TooLarge {
                    kind: crate::image_resolver::ImageBlobKind::BaseImage,
                    path: path.clone(),
                    len: crate::image_resolver::MAX_BASE_IMAGE_BYTES + 1,
                    max: crate::image_resolver::MAX_BASE_IMAGE_BYTES,
                }),
                tonic::Code::InvalidArgument,
            ),
            (
                image_error_to_status(ImageResolverError::AllocationFailed {
                    kind: crate::image_resolver::ImageBlobKind::BaseImage,
                    path: path.clone(),
                    requested: crate::image_resolver::MAX_BASE_IMAGE_BYTES,
                }),
                tonic::Code::ResourceExhausted,
            ),
            (
                image_error_to_status(ImageResolverError::Io {
                    kind: crate::image_resolver::ImageBlobKind::BaseImage,
                    path,
                    source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
                }),
                tonic::Code::Unavailable,
            ),
        ];

        for (status, code) in cases {
            assert_eq!(status.code(), code, "{status}");
        }
    }

    #[cfg(target_arch = "x86_64")]
    fn service_test_cpuid_table() -> Vec<dh_vmm::config::CpuidLeaf> {
        dh_vmm::kvm::KvmSystem::open()
            .and_then(|sys| sys.masked_cpuid_table())
            .unwrap_or_else(|_| {
                vec![dh_vmm::config::CpuidLeaf {
                    function: 0,
                    index: 0,
                    flags: 0,
                    eax: 0,
                    ebx: 0,
                    ecx: 0,
                    edx: 0,
                }]
            })
    }

    #[cfg(target_arch = "x86_64")]
    fn service_machine_config(base_hash: [u8; 32], kernel_hash: [u8; 32]) -> proto::MachineConfig {
        service_machine_config_with_mem_epoch_len(
            base_hash,
            kernel_hash,
            2 * 1024 * 1024,
            dh_vmm::config::DEFAULT_EPOCH_LEN,
        )
    }

    #[cfg(target_arch = "x86_64")]
    fn service_machine_config_with_mem_epoch_len(
        base_hash: [u8; 32],
        kernel_hash: [u8; 32],
        mem_bytes: u64,
        epoch_len: u64,
    ) -> proto::MachineConfig {
        let mut config = dh_vmm::config::MachineConfig::new(
            mem_bytes,
            base_hash,
            dh_vmm::config::BootSpec::Elf {
                kernel_hash,
                cmdline: b"1000000".to_vec(),
            },
        );
        config.epoch_len = epoch_len;
        config.cpuid_table = service_test_cpuid_table();
        config.device_set = vec![
            dh_devices::clock::DEVICE_ID_PV_CLOCK,
            dh_devices::pad::DEVICE_ID_PV_PAD,
            dh_devices::entropy::DEVICE_ID_PV_ENTROPY,
            dh_devices::serial::DEVICE_ID_DEBUG_SERIAL,
        ];
        machine_config_to_proto(&config)
    }

    #[cfg(target_arch = "x86_64")]
    fn device_exercise_machine_config(
        base_hash: [u8; 32],
        kernel_hash: [u8; 32],
    ) -> proto::MachineConfig {
        let mut config = dh_vmm::config::MachineConfig::new(
            8 * 1024 * 1024,
            base_hash,
            dh_vmm::config::BootSpec::Elf {
                kernel_hash,
                cmdline: Vec::new(),
            },
        );
        config.cpuid_table = service_test_cpuid_table();
        config.device_set = vec![
            dh_devices::detchannel::DEVICE_ID_DETCHANNEL,
            dh_devices::clock::DEVICE_ID_PV_CLOCK,
            dh_devices::pad::DEVICE_ID_PV_PAD,
            dh_devices::entropy::DEVICE_ID_PV_ENTROPY,
            dh_devices::blk::DEVICE_ID_PV_BLK,
            dh_devices::serial::DEVICE_ID_DEBUG_SERIAL,
        ];
        machine_config_to_proto(&config)
    }

    #[cfg(target_arch = "x86_64")]
    fn bzimage_service_machine_config(
        base_hash: [u8; 32],
        kernel_hash: [u8; 32],
        initramfs_hash: [u8; 32],
    ) -> proto::MachineConfig {
        let mut config = dh_vmm::config::MachineConfig::new(
            64 * 1024 * 1024,
            base_hash,
            dh_vmm::config::BootSpec::BzImage {
                kernel_hash,
                initramfs_hash,
                cmdline: dh_vmm::config::canonicalize_bzimage_cmdline_extras(b"quiet")
                    .expect("allowed BzImage cmdline extra"),
            },
        );
        config.cpuid_table = service_test_cpuid_table();
        config.device_set = vec![
            dh_devices::detchannel::DEVICE_ID_DETCHANNEL,
            dh_devices::clock::DEVICE_ID_PV_CLOCK,
            dh_devices::pad::DEVICE_ID_PV_PAD,
            dh_devices::entropy::DEVICE_ID_PV_ENTROPY,
            dh_devices::blk::DEVICE_ID_PV_BLK,
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
    fn framebuffer_fixture_machine_config(
        base_hash: [u8; 32],
        kernel_hash: [u8; 32],
    ) -> proto::MachineConfig {
        capture_fixture_machine_config(base_hash, kernel_hash)
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
        config.cpuid_table = service_test_cpuid_table();
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
    fn framebuffer_fixture_pixels() -> Vec<u8> {
        let mut pixels = Vec::with_capacity(nanokernel::FRAMEBUFFER_FIXTURE_PIXEL_BYTES as usize);
        for j in 0..nanokernel::FRAMEBUFFER_FIXTURE_PIXEL_BYTES / 8 {
            pixels.extend_from_slice(
                &(nanokernel::FRAMEBUFFER_FIXTURE_FB_QWORD_BASE + j).to_le_bytes(),
            );
        }
        pixels
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
    fn capture_epoch_leg(
        capture: bool,
    ) -> (
        dh_vmm::runctl::SegmentOutcome,
        Vec<(u64, u64, [u8; 32])>,
        [u8; 32],
    ) {
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
                hash_device_sections: None,
            };
            dh_vmm::runctl::run_segment_with_epochs(
                &mut segment,
                dh_vmm::runctl::Until::IcountBudget(100_000),
                &mut || false,
                &mut |exit| {
                    let icount = counter.read().map_err(|e| {
                        dh_vmm::boundary::BoundaryError::Exit(format!("counter read: {e:?}"))
                    })?;
                    service_exit_with_detchannel(&mut rail.borrow_mut(), icount, icount, exit)
                        .map(|_| ())
                },
                &mut |epoch_index, boundary, chain_value, _slot| {
                    epochs.push((epoch_index, boundary.icount, chain_value));
                    rail.borrow_mut()
                        .log_epoch_hash(epoch_index, boundary.icount, chain_value)
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
        let mut post_capture_chain = dh_vmm::hash::StateHashChain::from_value(outcome.state_hash);
        let device_sections = dh_vmm::hash::device_sections(&rail.bus);
        post_capture_chain
            .push_final_link(
                &slot,
                &device_sections,
                outcome.boundary.icount,
                outcome.vns,
            )
            .unwrap();
        (outcome, epochs, post_capture_chain.value())
    }

    #[cfg(target_arch = "x86_64")]
    struct CaptureNeutralityLeg {
        run: proto::RunResponse,
        snap: proto::TakeSnapshotResponse,
        log_bytes: Vec<u8>,
    }

    #[cfg(target_arch = "x86_64")]
    struct BisectionCheckpointEquivalenceLeg {
        first_run: proto::RunResponse,
        final_run: proto::RunResponse,
        snap: proto::TakeSnapshotResponse,
        log_bytes: Vec<u8>,
        slot_icount: u64,
        slot_base_snapshot_id: Option<[u8; 32]>,
    }

    #[cfg(target_arch = "x86_64")]
    #[derive(Debug, PartialEq, Eq)]
    struct ComparableLogRecord {
        kind: u8,
        rflags: u8,
        icount: u64,
        boundary_rip: u64,
        payload: Vec<u8>,
    }

    #[cfg(target_arch = "x86_64")]
    #[derive(Debug, PartialEq, Eq)]
    struct BisectionCheckpointAuxRecord {
        seq: u32,
        icount: u64,
        boundary_rip: u64,
        format_version: u16,
        flags: u16,
        max_covered_gap: u32,
        checkpoint_snapshot_ref: [u8; 32],
        checkpoint_icount: u64,
        checkpoint_vns: u64,
    }

    #[cfg(target_arch = "x86_64")]
    fn log_records_without_bisection_checkpoints(log: &[u8]) -> Vec<ComparableLogRecord> {
        let reader = dh_inputlog::reader::LogReader::parse(log).unwrap();
        reader
            .records()
            .filter(|rec| {
                !matches!(
                    rec.body(),
                    dh_inputlog::reader::RecordBody::BisectionCheckpoint { .. }
                )
            })
            .map(|rec| ComparableLogRecord {
                kind: rec.kind(),
                rflags: rec.rflags(),
                icount: rec.icount(),
                boundary_rip: rec.boundary_rip(),
                payload: rec.payload().to_vec(),
            })
            .collect()
    }

    #[cfg(target_arch = "x86_64")]
    fn bisection_checkpoint_aux_records(log: &[u8]) -> Vec<BisectionCheckpointAuxRecord> {
        let reader = dh_inputlog::reader::LogReader::parse(log).unwrap();
        reader
            .records()
            .filter_map(|rec| match rec.body() {
                dh_inputlog::reader::RecordBody::BisectionCheckpoint {
                    format_version,
                    flags,
                    max_covered_gap,
                    checkpoint_snapshot_ref,
                    checkpoint_icount,
                    checkpoint_vns,
                } => Some(BisectionCheckpointAuxRecord {
                    seq: rec.seq(),
                    icount: rec.icount(),
                    boundary_rip: rec.boundary_rip(),
                    format_version,
                    flags,
                    max_covered_gap,
                    checkpoint_snapshot_ref,
                    checkpoint_icount,
                    checkpoint_vns,
                }),
                _ => None,
            })
            .collect()
    }

    #[cfg(target_arch = "x86_64")]
    fn epoch_hash_record_order(log: &[u8]) -> Vec<(u32, u64)> {
        let reader = dh_inputlog::reader::LogReader::parse(log).unwrap();
        reader
            .records()
            .filter_map(|rec| match rec.body() {
                dh_inputlog::reader::RecordBody::EpochHash { .. } => {
                    Some((rec.seq(), rec.icount()))
                }
                _ => None,
            })
            .collect()
    }

    #[cfg(target_arch = "x86_64")]
    fn skip_first_checkpoint_and_widen_second(log: &[u8]) -> Vec<u8> {
        let reader = dh_inputlog::reader::LogReader::parse(log).unwrap();
        let header = reader.header().clone();
        let (stop_reason, end_state_hash) = reader.end();
        let mut writer = log_writer_from_reader_header(&header);
        let mut checkpoint_count = 0usize;
        let mut widened_second = false;

        for rec in reader.records() {
            match rec.body() {
                dh_inputlog::reader::RecordBody::EpochHash {
                    epoch_index,
                    chain_value,
                } => writer
                    .epoch_hash(rec.icount(), rec.boundary_rip(), epoch_index, chain_value)
                    .unwrap(),
                dh_inputlog::reader::RecordBody::BisectionCheckpoint {
                    format_version,
                    flags,
                    max_covered_gap,
                    checkpoint_snapshot_ref,
                    checkpoint_vns,
                    ..
                } => {
                    assert_eq!(
                        format_version,
                        dh_inputlog::dhilog::BISECTION_CHECKPOINT_FORMAT_VERSION
                    );
                    assert_eq!(flags, dh_inputlog::dhilog::BISECTION_CHECKPOINT_FLAGS);
                    checkpoint_count += 1;
                    if checkpoint_count == 1 {
                        continue;
                    }
                    let max_covered_gap = if checkpoint_count == 2 {
                        widened_second = true;
                        u32::try_from(rec.icount()).unwrap()
                    } else {
                        max_covered_gap
                    };
                    writer
                        .bisection_checkpoint(
                            rec.icount(),
                            rec.boundary_rip(),
                            max_covered_gap,
                            checkpoint_snapshot_ref,
                            checkpoint_vns,
                        )
                        .unwrap();
                }
                dh_inputlog::reader::RecordBody::End { .. } => {}
                other => panic!("test log contains unexpected record {other:?}"),
            }
        }

        assert!(
            widened_second,
            "test fixture must contain at least two checkpoint records"
        );
        writer
            .seal(dh_inputlog::dhilog::SealParams {
                end_snapshot_id: header.end_snapshot_id,
                end_icount: header.end_icount,
                end_vns: header.end_vns,
                end_state_hash,
                stop_reason,
            })
            .unwrap()
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
        svc.destroy_vm(Request::new(proto::DestroyVmRequest { lease: Some(lease) }))
            .await
            .unwrap();
        CaptureNeutralityLeg {
            run,
            snap,
            log_bytes,
        }
    }

    #[cfg(target_arch = "x86_64")]
    async fn bisection_checkpoint_equivalence_leg(
        svc: &WorkerService,
        base_snapshot: proto::SnapshotRef,
    ) -> BisectionCheckpointEquivalenceLeg {
        let restored = svc
            .restore_snapshot(Request::new(proto::RestoreSnapshotRequest {
                snapshot: Some(base_snapshot),
                entropy_seed: Vec::new(),
            }))
            .await
            .unwrap()
            .into_inner();
        let lease = restored.lease.unwrap();

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

        let first_run = svc
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
            first_run.reason,
            proto_stop_reason(dh_vmm::runctl::StopReason::BudgetReached)
        );
        assert_eq!(first_run.icount, 20_000);
        assert_eq!(
            svc.runtime_table()
                .with(lease.slot_id, |actor| actor
                    .with_runtime(|runtime| runtime.queued_inputs.len())
                    .unwrap())
                .unwrap(),
            1,
            "future input must still be queued at the checkpoint boundary"
        );

        let final_run = svc
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
            final_run.reason,
            proto_stop_reason(dh_vmm::runctl::StopReason::BudgetReached)
        );
        assert_eq!(final_run.icount, 40_000);
        assert_eq!(
            svc.runtime_table()
                .with(lease.slot_id, |actor| actor
                    .with_runtime(|runtime| runtime.queued_inputs.len())
                    .unwrap())
                .unwrap(),
            0,
            "future input should drain after the post-checkpoint run"
        );

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
        let slot = svc.slot_manager().slot_info(lease.slot_id).unwrap();
        let slot_icount = slot.icount;
        let slot_base_snapshot_id = slot.base_snapshot_id;
        assert_eq!(
            slot_base_snapshot_id,
            Some(
                snap.snapshot
                    .as_ref()
                    .unwrap()
                    .hash
                    .clone()
                    .try_into()
                    .unwrap()
            )
        );

        svc.destroy_vm(Request::new(proto::DestroyVmRequest { lease: Some(lease) }))
            .await
            .unwrap();

        BisectionCheckpointEquivalenceLeg {
            first_run,
            final_run,
            snap,
            log_bytes,
            slot_icount,
            slot_base_snapshot_id,
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

        assert_eq!(
            checked_introspection_total("ReadGuestMemory.ranges[0]", 0, 1024).unwrap(),
            (1024, 1024)
        );
        assert_eq!(
            checked_introspection_total(
                "ReadGuestMemory.ranges[0]",
                0,
                MAX_READ_GUEST_MEMORY_BYTES as u64
            )
            .unwrap(),
            (MAX_READ_GUEST_MEMORY_BYTES, MAX_READ_GUEST_MEMORY_BYTES)
        );
        let over = checked_introspection_total(
            "ReadGuestMemory.ranges[1]",
            MAX_READ_GUEST_MEMORY_BYTES,
            1,
        )
        .unwrap_err();
        assert_eq!(over.code(), tonic::Code::InvalidArgument);
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn framebuffer_descriptor_shape_is_enforced() {
        let mut region = Vec::new();
        region.extend_from_slice(&4u32.to_le_bytes());
        region.extend_from_slice(&2u32.to_le_bytes());
        region.extend_from_slice(&16u32.to_le_bytes());
        region.extend_from_slice(&(proto::PixelFormat::Xrgb8888 as u32).to_le_bytes());
        region.extend_from_slice(&(0u8..32).collect::<Vec<_>>());

        let (width, height, stride, format, pixels) =
            framebuffer_response_from_region_bytes(&region).unwrap();
        assert_eq!(width, 4);
        assert_eq!(height, 2);
        assert_eq!(stride, 16);
        assert_eq!(format, proto_pixel_format(proto::PixelFormat::Xrgb8888));
        assert_eq!(pixels, (0u8..32).collect::<Vec<_>>());
        let (capture_pixels, capture_info) =
            descriptor_framebuffer_capture(&region, 7).unwrap().unwrap();
        assert_eq!(capture_pixels, pixels);
        assert_eq!(capture_info.width, 4);
        assert_eq!(capture_info.height, 2);
        assert_eq!(capture_info.stride, 16);
        assert_eq!(
            capture_info.format,
            proto_pixel_format(proto::PixelFormat::Xrgb8888)
        );
        assert_eq!(capture_info.frame_counter, 7);

        let raw = capture_fixture_bytes(0, FRAMEBUFFER_DESCRIPTOR_BYTES + 32);
        let err = framebuffer_response_from_region_bytes(&raw).unwrap_err();
        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert!(err.message().contains("framebuffer descriptor"));
        assert!(descriptor_framebuffer_capture(&raw, 7).unwrap().is_none());

        let mut zero_width = region.clone();
        zero_width[0..4].copy_from_slice(&0u32.to_le_bytes());
        let err = descriptor_framebuffer_capture(&zero_width, 7).unwrap_err();
        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert!(err.message().contains("zero dimensions"));

        let mut bad_stride = region.clone();
        bad_stride[8..12].copy_from_slice(&4u32.to_le_bytes());
        let err = descriptor_framebuffer_capture(&bad_stride, 7).unwrap_err();
        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert!(err.message().contains("stride"));

        let mut bad_format = region.clone();
        bad_format[12..16].copy_from_slice(&99u32.to_le_bytes());
        let err = descriptor_framebuffer_capture(&bad_format, 7).unwrap_err();
        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert!(err.message().contains("unsupported pixel_format"));

        let truncated = &region[..region.len() - 1];
        let err = descriptor_framebuffer_capture(truncated, 7).unwrap_err();
        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert!(err.message().contains("truncated"));
    }

    #[cfg(target_arch = "x86_64")]
    fn retained_test_event(stream: u32, sequence: u64) -> DrainedGuestEvent {
        DrainedGuestEvent {
            stream,
            icount: sequence,
            vns: sequence * 10,
            payload: sequence.to_le_bytes().to_vec(),
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn guest_event_retention_cap_keeps_newest_events() {
        let mut retained = Vec::new();
        append_guest_events_with_retention_cap(
            &mut retained,
            (0..MAX_RETAINED_GUEST_EVENTS_PER_SLOT + 3)
                .map(|sequence| retained_test_event(sequence as u32, sequence as u64))
                .collect(),
        );

        assert_eq!(retained.len(), MAX_RETAINED_GUEST_EVENTS_PER_SLOT);
        assert_eq!(retained.first().unwrap().icount, 3);
        assert_eq!(
            retained.last().unwrap().icount,
            (MAX_RETAINED_GUEST_EVENTS_PER_SLOT + 2) as u64
        );
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn stream_guest_events_filter_retains_unselected_and_consumes_selected() {
        let beacon = detguest_wire::record::EventKind::Beacon as u32;
        let frame_mark = detguest_wire::record::EventKind::FrameMark as u32;
        let mut retained = vec![
            retained_test_event(beacon, 1),
            retained_test_event(frame_mark, 2),
            retained_test_event(beacon, 3),
        ];

        let selected = select_stream_guest_events(&mut retained, &[beacon]);
        assert_eq!(
            selected
                .iter()
                .map(|event| event.icount)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert_eq!(retained, vec![retained_test_event(frame_mark, 2)]);

        let selected = select_stream_guest_events(&mut retained, &[beacon]);
        assert!(selected.is_empty());
        assert_eq!(retained, vec![retained_test_event(frame_mark, 2)]);

        let selected = select_stream_guest_events(&mut retained, &[frame_mark]);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].icount, 2);
        assert!(retained.is_empty());
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn filtered_guest_events_stay_capped() {
        let mut retained: Vec<_> = (0..MAX_RETAINED_GUEST_EVENTS_PER_SLOT + 2)
            .map(|sequence| retained_test_event(1, sequence as u64))
            .collect();

        let selected = select_stream_guest_events(&mut retained, &[2]);
        assert!(selected.is_empty());
        assert_eq!(retained.len(), MAX_RETAINED_GUEST_EVENTS_PER_SLOT);
        assert_eq!(retained.first().unwrap().icount, 2);
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn event_icount_conversion_keeps_segment_log_and_cumulative_stream_domains() {
        assert_eq!(cumulative_event_icount(0, 0, 42), 42);
        assert_eq!(cumulative_event_icount(10, 1_000, 25), 1_015);
        assert_eq!(cumulative_event_icount(50, 1_000, 49), 1_000);
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
    fn healthz_reflects_preflight_status() {
        let svc = WorkerService::new(test_config(1)).unwrap();
        let ok = http_response_for_request(&svc, "GET /healthz HTTP/1.1\r\n\r\n");
        assert!(ok.starts_with("HTTP/1.1 200 OK"));
        assert!(ok.contains("preflight=skipped\n"));

        let failed_check = crate::preflight::CheckResult {
            name: "test.check",
            ok: false,
            got: "bad".into(),
            want: "good".into(),
        };
        let mut config = test_config(1);
        config.preflight = PreflightHealth::failed(&[failed_check]);
        let svc = WorkerService::new(config).unwrap();
        let failed = http_response_for_request(&svc, "GET /healthz HTTP/1.1\r\n\r\n");
        assert!(failed.starts_with("HTTP/1.1 503 Service Unavailable"));
        assert!(failed.contains("preflight=failed\n"));
    }

    #[test]
    fn metrics_endpoint_exposes_arch_s9_families() {
        let svc = WorkerService::new(test_config(1)).unwrap();
        let lease = svc.slot_manager().allocate(0).unwrap();
        svc.inner.metrics.record_exit(lease.slot_id, "hlt");
        svc.inner
            .metrics
            .observe_snapshot(Duration::from_millis(3), 8);
        svc.inner.metrics.observe_fork(Duration::from_millis(4));
        svc.inner.metrics.observe_restore(Duration::from_millis(5));
        svc.inner.metrics.record_verification_failure();

        let response = http_response_for_request(&svc, "GET /metrics HTTP/1.1\r\n\r\n");
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        for family in ARCH_S9_METRIC_FAMILIES {
            assert!(
                response.contains(&format!("# TYPE {family} ")),
                "missing metric family {family}\n{response}"
            );
        }
        assert!(response.contains("# TYPE dh_worker_slot_icount gauge\n"));
        assert!(response.contains("dh_worker_slot_icount{slot_id=\"0\"} 0\n"));
        assert!(response.contains("dh_worker_slot_icount_rate{slot_id=\"0\"} 0\n"));
        assert!(response.contains("dh_worker_exits_total{slot_id=\"0\",reason=\"hlt\"} 1\n"));
        assert!(response.contains("dh_worker_snapshot_duration_milliseconds_count 1\n"));
        assert!(response.contains("dh_worker_snapshot_dirty_pages_bucket{le=\"8\"} 1\n"));
        assert!(response.contains("dh_worker_verification_failures_total 1\n"));
        assert!(response.contains("dh_pmi_skid_instructions_bucket{le=\"79\"} 50000\n"));
        assert!(response.contains("dh_pmi_skid_instructions_count 50000\n"));
    }

    #[tokio::test]
    async fn watch_slots_streams_state_transitions() {
        use tokio_stream::StreamExt;

        let svc = WorkerService::new(test_config(1)).unwrap();
        let response = svc
            .watch_slots(Request::new(proto::WatchSlotsRequest {}))
            .await
            .unwrap();
        let mut stream = response.into_inner();

        let lease = svc.slot_manager().allocate(0).unwrap();
        let event = tokio::time::timeout(Duration::from_secs(1), stream.as_mut().next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let slot = event.slot.unwrap();
        assert_eq!(slot.slot_id, lease.slot_id);
        assert_eq!(slot.state, i32::from(proto::SlotState::PausedS));

        svc.slot_manager().mark_running(&lease, 0).unwrap();
        let event = tokio::time::timeout(Duration::from_secs(1), stream.as_mut().next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let slot = event.slot.unwrap();
        assert_eq!(slot.slot_id, lease.slot_id);
        assert_eq!(slot.state, i32::from(proto::SlotState::Running));

        svc.slot_manager()
            .mark_paused_at_position(&lease, 42, Some([0xAB; 32]), 0)
            .unwrap();
        let event = tokio::time::timeout(Duration::from_secs(1), stream.as_mut().next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let slot = event.slot.unwrap();
        assert_eq!(slot.slot_id, lease.slot_id);
        assert_eq!(slot.state, i32::from(proto::SlotState::PausedS));
        assert_eq!(slot.icount, 42);
        assert_eq!(slot.base.unwrap().hash, vec![0xAB; 32]);
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
    fn boot_slot_routes_bzimage_to_linux_loader_once() {
        if !runtime_tests_available() {
            return;
        }
        let sys = dh_vmm::kvm::KvmSystem::open().unwrap();
        let slot = sys.create_slot_vm(64 * 1024 * 1024).unwrap();
        let elf_calls = std::cell::Cell::new(0u32);
        let bzimage_calls = std::cell::Cell::new(0u32);

        boot_slot_with_loaders(
            &slot,
            crate::image_resolver::ResolvedBoot::BzImage {
                kernel: b"kernel".to_vec(),
                initramfs: b"initramfs".to_vec(),
                cmdline: b"cmdline".to_vec(),
            },
            |_, _, _| {
                elf_calls.set(elf_calls.get() + 1);
                Ok(())
            },
            |_, kernel, initramfs, cmdline| {
                bzimage_calls.set(bzimage_calls.get() + 1);
                assert_eq!(kernel, b"kernel");
                assert_eq!(initramfs, b"initramfs");
                assert_eq!(cmdline, b"cmdline");
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(elf_calls.get(), 0);
        assert_eq!(bzimage_calls.get(), 1);
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn create_vm_accepts_bzimage_boot_through_linux_loader() {
        if !runtime_tests_available() {
            return;
        }
        let image_cache = tempfile::TempDir::new().unwrap();
        let base_hash = write_cache_blob(image_cache.path(), &vec![0u8; 4096]);
        let kernel_hash =
            write_cache_blob(image_cache.path(), &synthetic_bzimage(&vec![0xf4; 0x1000]));
        let initramfs_hash = write_cache_blob(image_cache.path(), b"initramfs");
        let svc = WorkerService::new(test_config_with_resources(
            1,
            image_cache.path().to_path_buf(),
            None,
        ))
        .unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let config = bzimage_service_machine_config(base_hash, kernel_hash, initramfs_hash);
            let created = svc
                .create_vm(Request::new(proto::CreateVmRequest {
                    config: Some(config),
                    entropy_seed: vec![0xB7; 32],
                }))
                .await
                .unwrap()
                .into_inner();
            let lease = created.lease.unwrap();
            assert_eq!(created.icount, 0);
            let expected_base_hash = base_hash;
            svc.runtime_table()
                .with(lease.slot_id, |actor| {
                    actor
                        .with_runtime(move |runtime| {
                            assert_eq!(runtime.machine_config.base_image_hash, expected_base_hash);
                            assert!(matches!(
                                &runtime.machine_config.boot,
                                dh_vmm::config::BootSpec::BzImage { .. }
                            ));
                            let devices: Vec<(u64, u16)> = runtime
                                .bus
                                .devices()
                                .map(|(base, dev)| (base, dev.device_id()))
                                .collect();
                            assert!(devices.contains(&(
                                DETCHANNEL_MMIO_BASE,
                                dh_devices::detchannel::DEVICE_ID_DETCHANNEL
                            )));
                            assert!(
                                devices.contains(&(0xD000_4000, dh_devices::blk::DEVICE_ID_PV_BLK))
                            );
                        })
                        .unwrap()
                })
                .unwrap();
            svc.destroy_vm(Request::new(proto::DestroyVmRequest { lease: Some(lease) }))
                .await
                .unwrap();
        });
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn bisection_checkpoint_capture_preserves_runtime_lineage_and_log_surfaces() {
        if !runtime_tests_available() {
            return;
        }
        let (_store_rt, _handle, _store_dir, transport) = spawn_store_for_service_test();
        let svc = WorkerService::new(test_config_with_resources(
            1,
            std::env::temp_dir(),
            Some(transport),
        ))
        .unwrap();
        let position = SlotPosition {
            cumulative_icount: 12_345,
            segment_icount: 4_321,
            vns: 987_654,
            epoch_index: 7,
            frame_counter: 3,
        };
        let base_snapshot = snapstore_types::SnapshotRef::from_bytes([0x55; 32]);
        let mut runtime = make_runtime(0x61, position, Some(base_snapshot.clone())).unwrap();
        runtime.log = Some(
            new_segment_log(
                &runtime.machine_config,
                runtime.base_snapshot.as_ref(),
                [0x11; 32],
            )
            .unwrap(),
        );
        runtime.dirty.insert(7).unwrap();

        let before = (
            runtime.base_snapshot.clone(),
            runtime.position,
            runtime.chain.value(),
            runtime.entropy.state(),
            runtime.dirty.len(),
            runtime.log.as_ref().map(|log| log.record_count()),
        );
        let store = svc.store().unwrap();
        let checkpoint = {
            let store = store.lock().unwrap();
            crate::snapshot_engine::capture_bisection_checkpoint_snapshot(
                &runtime.slot,
                dh_vmm::SlotState::Paused,
                &runtime.bus,
                &runtime.entropy,
                &runtime.machine_config,
                runtime.boundary_state(runtime.queued_inputs.is_empty()),
                &store,
            )
            .unwrap()
        };
        let after = (
            runtime.base_snapshot.clone(),
            runtime.position,
            runtime.chain.value(),
            runtime.entropy.state(),
            runtime.dirty.len(),
            runtime.log.as_ref().map(|log| log.record_count()),
        );
        assert_eq!(after, before);
        assert_eq!(
            checkpoint.pages_shipped,
            runtime.slot.mem_bytes / dh_vmm::dirty::PAGE_SIZE
        );
        assert_eq!(checkpoint.hash_chain, before.2);

        let container = {
            let store = store.lock().unwrap();
            store.get_snapshot(checkpoint.snapshot_ref).unwrap()
        };
        let manifest = snapstore_manifest::Manifest::decode(&container).unwrap();
        assert_eq!(manifest.parent, None);
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn bisection_checkpoint_capture_is_execution_equivalent_to_no_capture() {
        if !runtime_tests_available() {
            return;
        }
        let image_cache = tempfile::TempDir::new().unwrap();
        let base_hash = write_cache_blob(image_cache.path(), &vec![0u8; 4096]);
        let kernel_hash = write_cache_blob(image_cache.path(), nanokernel::landing_loop_elf());
        let (_store_rt, _handle, _store_dir, transport) = spawn_store_for_service_test();
        let svc_control = WorkerService::new(test_config_with_resources(
            1,
            image_cache.path().to_path_buf(),
            Some(transport.clone()),
        ))
        .unwrap();
        let mut checkpoint_config =
            test_config_with_resources(1, image_cache.path().to_path_buf(), Some(transport));
        checkpoint_config.bisection_checkpoints = BisectionCheckpointConfig::every_epoch();
        let svc_checkpoint = WorkerService::new(checkpoint_config).unwrap();
        let machine_config = service_machine_config_with_mem_epoch_len(
            base_hash,
            kernel_hash,
            2 * 1024 * 1024,
            20_000,
        );
        let expected_machine_config = machine_config_from_proto(&machine_config).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let created = svc_control
                .create_vm(Request::new(proto::CreateVmRequest {
                    config: Some(machine_config),
                    entropy_seed: vec![0xB5; 32],
                }))
                .await
                .unwrap()
                .into_inner();
            let root_lease = created.lease.unwrap();
            let base_snapshot = svc_control
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
            svc_control
                .destroy_vm(Request::new(proto::DestroyVmRequest {
                    lease: Some(root_lease),
                }))
                .await
                .unwrap();

            let control =
                bisection_checkpoint_equivalence_leg(&svc_control, base_snapshot.clone()).await;
            let captured =
                bisection_checkpoint_equivalence_leg(&svc_checkpoint, base_snapshot).await;

            assert_eq!(captured.first_run.icount, control.first_run.icount);
            assert_eq!(captured.first_run.vns, control.first_run.vns);
            assert_eq!(captured.first_run.state_hash, control.first_run.state_hash);
            assert_eq!(captured.final_run.icount, control.final_run.icount);
            assert_eq!(captured.final_run.vns, control.final_run.vns);
            assert_eq!(captured.final_run.state_hash, control.final_run.state_hash);
            assert_eq!(
                captured.snap.snapshot.as_ref().unwrap().hash,
                control.snap.snapshot.as_ref().unwrap().hash,
                "checkpoint capture must not perturb the final snapshot ref"
            );
            assert_ne!(
                captured.snap.input_log_id, control.snap.input_log_id,
                "checkpoint AUX records should change the stored DHILOG id"
            );
            assert_eq!(captured.snap.icount, control.snap.icount);
            assert_eq!(captured.snap.vns, control.snap.vns);
            assert_eq!(captured.snap.state_hash, control.snap.state_hash);
            assert_eq!(captured.snap.dirty_pages, control.snap.dirty_pages);
            assert_eq!(
                log_records_without_bisection_checkpoints(&captured.log_bytes),
                log_records_without_bisection_checkpoints(&control.log_bytes),
                "only BISECTION_CHECKPOINT AUX records may differ"
            );
            assert!(
                bisection_checkpoint_aux_records(&control.log_bytes).is_empty(),
                "disabled recorder must not emit checkpoint AUX records"
            );
            let checkpoints = bisection_checkpoint_aux_records(&captured.log_bytes);
            let epoch_hashes = epoch_hash_record_order(&captured.log_bytes);
            assert_eq!(checkpoints.len(), 2);
            assert!(
                checkpoints
                    .windows(2)
                    .all(|pair| (pair[0].icount, pair[0].seq) < (pair[1].icount, pair[1].seq)),
                "checkpoint records must preserve monotone (icount, seq) order"
            );
            for (record, expected_icount) in checkpoints.iter().zip([20_000, 40_000]) {
                let epoch_seq = epoch_hashes
                    .iter()
                    .find_map(|(seq, icount)| (*icount == record.icount).then_some(*seq))
                    .expect("checkpoint must share an icount with an EPOCH_HASH record");
                assert!(
                    epoch_seq < record.seq,
                    "checkpoint evidence must follow its EPOCH_HASH chain link"
                );
                assert_eq!(
                    record.format_version,
                    dh_inputlog::dhilog::BISECTION_CHECKPOINT_FORMAT_VERSION
                );
                assert_eq!(
                    record.flags,
                    dh_inputlog::dhilog::BISECTION_CHECKPOINT_FLAGS
                );
                assert_eq!(record.icount, expected_icount);
                assert_ne!(record.boundary_rip, 0);
                assert_eq!(record.checkpoint_icount, expected_icount);
                assert_eq!(record.max_covered_gap, 20_000);
                assert_eq!(
                    record.checkpoint_vns,
                    expected_machine_config
                        .clock
                        .vns_from_icount(expected_icount)
                        .unwrap()
                );
            }
            let checkpoint_refs: Vec<[u8; 32]> = checkpoints
                .iter()
                .map(|record| record.checkpoint_snapshot_ref)
                .collect();
            let svc_for_store = svc_checkpoint.clone();
            tokio::task::spawn_blocking(move || {
                let store = svc_for_store.store().unwrap();
                let store = store.lock().unwrap();
                for snapshot_ref in checkpoint_refs {
                    let container = store
                        .get_snapshot(snapstore_types::SnapshotRef::from_bytes(snapshot_ref))
                        .unwrap();
                    let manifest = snapstore_manifest::Manifest::decode(&container).unwrap();
                    assert_eq!(manifest.parent, None);
                }
            })
            .await
            .unwrap();
            assert_eq!(captured.slot_icount, control.slot_icount);
            assert_eq!(
                captured.slot_base_snapshot_id,
                control.slot_base_snapshot_id
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
    fn descriptor_framebuffer_fixture_feeds_capture_and_get_framebuffer() {
        if !runtime_tests_available() {
            return;
        }
        let image_cache = tempfile::TempDir::new().unwrap();
        let base_hash = write_cache_blob(image_cache.path(), &vec![0u8; 4096]);
        let kernel_hash =
            write_cache_blob(image_cache.path(), nanokernel::framebuffer_fixture_elf());
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
                    config: Some(framebuffer_fixture_machine_config(base_hash, kernel_hash)),
                    entropy_seed: vec![0xF0; 32],
                }))
                .await
                .unwrap()
                .into_inner();
            let lease = created.lease.unwrap();
            let expected_pixels = framebuffer_fixture_pixels();

            let run = svc
                .run(Request::new(proto::RunRequest {
                    lease: Some(lease.clone()),
                    until: Some(proto::run_request::Until::IcountBudget(10_000_000)),
                    hard_icount_cap: 0,
                    capture: Some(proto::CaptureSpec {
                        ranges: Vec::new(),
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
            let capture_pixels = lz4_flex::decompress_size_prepended(&run.fb_lz4).unwrap();
            assert_eq!(capture_pixels, expected_pixels);
            assert_eq!(
                capture_pixels.len(),
                (nanokernel::FRAMEBUFFER_FIXTURE_STRIDE * nanokernel::FRAMEBUFFER_FIXTURE_HEIGHT)
                    as usize
            );
            let capture_info = run.fb_info.unwrap();
            assert_eq!(capture_info.width, nanokernel::FRAMEBUFFER_FIXTURE_WIDTH);
            assert_eq!(capture_info.height, nanokernel::FRAMEBUFFER_FIXTURE_HEIGHT);
            assert_eq!(capture_info.stride, nanokernel::FRAMEBUFFER_FIXTURE_STRIDE);
            assert_eq!(
                capture_info.format,
                proto_pixel_format(proto::PixelFormat::Xrgb8888)
            );
            assert_eq!(capture_info.frame_counter, 0);

            let fb = svc
                .get_framebuffer(Request::new(proto::GetFramebufferRequest {
                    lease: Some(lease),
                }))
                .await
                .unwrap()
                .into_inner();
            assert_eq!(fb.icount, run.icount);
            assert_eq!(fb.width, nanokernel::FRAMEBUFFER_FIXTURE_WIDTH);
            assert_eq!(fb.height, nanokernel::FRAMEBUFFER_FIXTURE_HEIGHT);
            assert_eq!(fb.stride, nanokernel::FRAMEBUFFER_FIXTURE_STRIDE);
            assert_eq!(fb.format, proto_pixel_format(proto::PixelFormat::Xrgb8888));
            assert_eq!(fb.frame_counter, capture_info.frame_counter);
            assert_eq!(fb.pixels, expected_pixels);
            assert_eq!(
                fb.pixels.len(),
                (u64::from(fb.stride) * u64::from(fb.height)) as usize
            );
        });
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn introspection_rpcs_read_memory_framebuffer_and_stream_guest_events() {
        if !runtime_tests_available() {
            return;
        }
        let image_cache = tempfile::TempDir::new().unwrap();
        let base_hash = write_cache_blob(image_cache.path(), &vec![0u8; 4096]);
        let capture_kernel_hash =
            write_cache_blob(image_cache.path(), nanokernel::capture_fixture_elf());
        let device_kernel_hash =
            write_cache_blob(image_cache.path(), nanokernel::device_exercise_elf());
        let svc = WorkerService::new(test_config_with_resources(
            1,
            image_cache.path().to_path_buf(),
            None,
        ))
        .unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            use tokio_stream::StreamExt;

            let created = svc
                .create_vm(Request::new(proto::CreateVmRequest {
                    config: Some(capture_fixture_machine_config(
                        base_hash,
                        capture_kernel_hash,
                    )),
                    entropy_seed: vec![0xA5; 32],
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

            let memory = svc
                .read_guest_memory(Request::new(proto::ReadGuestMemoryRequest {
                    lease: Some(lease.clone()),
                    ranges: vec![proto::GpaRange { gpa: 0, len: 16 }],
                    region_ranges: vec![proto::RegionRange {
                        region: "framebuffer".into(),
                        layout_version: nanokernel::CAPTURE_FIXTURE_DEFAULT_LAYOUT_VERSION,
                        offset: 8,
                        len: 24,
                    }],
                }))
                .await
                .unwrap()
                .into_inner();
            assert_eq!(memory.icount, run.icount);
            assert_eq!(memory.chunks.len(), 2);
            assert_eq!(memory.chunks[0].len(), 16);
            assert_eq!(memory.chunks[1], capture_fixture_bytes(8, 24));

            let fb_err = svc
                .get_framebuffer(Request::new(proto::GetFramebufferRequest {
                    lease: Some(lease.clone()),
                }))
                .await
                .unwrap_err();
            assert_eq!(fb_err.code(), tonic::Code::FailedPrecondition);
            assert!(fb_err.message().contains("framebuffer descriptor"));

            svc.destroy_vm(Request::new(proto::DestroyVmRequest { lease: Some(lease) }))
                .await
                .unwrap();

            let created = svc
                .create_vm(Request::new(proto::CreateVmRequest {
                    config: Some(device_exercise_machine_config(
                        base_hash,
                        device_kernel_hash,
                    )),
                    entropy_seed: vec![0xA6; 32],
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

            let response = svc
                .stream_guest_events(Request::new(proto::StreamGuestEventsRequest {
                    lease: Some(lease),
                    streams: vec![detguest_wire::record::EventKind::Beacon as u32],
                }))
                .await
                .unwrap();
            let mut stream = response.into_inner();
            let mut saw_beacon = false;
            while let Some(event) = stream.as_mut().next().await {
                let event = event.unwrap();
                assert_eq!(
                    event.stream,
                    detguest_wire::record::EventKind::Beacon as u32
                );
                assert!(event.icount > 0);
                assert!(event.icount <= run.icount);
                assert_eq!(event.payload.len(), 8);
                assert_eq!(
                    u32::from_le_bytes(event.payload[0..4].try_into().unwrap()),
                    nanokernel::DEVICE_EXERCISE_BEACON_ID
                );
                saw_beacon = true;
            }
            assert!(saw_beacon, "device exercise should emit one Beacon event");
        });
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn run_next_sdk_event_returns_matching_guest_event_and_keeps_stream_backlog() {
        if !runtime_tests_available() {
            return;
        }
        let image_cache = tempfile::TempDir::new().unwrap();
        let base_hash = write_cache_blob(image_cache.path(), &vec![0u8; 4096]);
        let kernel_hash = write_cache_blob(image_cache.path(), nanokernel::device_exercise_elf());
        let svc = WorkerService::new(test_config_with_resources(
            1,
            image_cache.path().to_path_buf(),
            None,
        ))
        .unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            use tokio_stream::StreamExt;

            let created = svc
                .create_vm(Request::new(proto::CreateVmRequest {
                    config: Some(device_exercise_machine_config(base_hash, kernel_hash)),
                    entropy_seed: vec![0xA8; 32],
                }))
                .await
                .unwrap()
                .into_inner();
            let lease = created.lease.unwrap();
            let beacon = detguest_wire::record::EventKind::Beacon as u32;
            let run = svc
                .run(Request::new(proto::RunRequest {
                    lease: Some(lease.clone()),
                    until: Some(proto::run_request::Until::NextSdkEvent(
                        proto::NextSdkEvent {
                            stream: Some(beacon),
                        },
                    )),
                    hard_icount_cap: 10_000_000,
                    capture: None,
                }))
                .await
                .unwrap()
                .into_inner();
            assert_eq!(
                run.reason,
                proto_stop_reason(dh_vmm::runctl::StopReason::NextSdkEvent)
            );
            let sdk_event = run.sdk_event.as_ref().expect("matching SDK event");
            assert_eq!(sdk_event.stream, beacon);
            assert!(sdk_event.icount > 0);
            assert_eq!(sdk_event.icount, run.icount);
            assert_eq!(sdk_event.payload.len(), 8);
            assert_eq!(
                u32::from_le_bytes(sdk_event.payload[0..4].try_into().unwrap()),
                nanokernel::DEVICE_EXERCISE_BEACON_ID
            );
            assert_eq!(
                svc.runtime_table()
                    .with(lease.slot_id, |actor| actor
                        .with_runtime(|runtime| runtime.guest_events.len())
                        .unwrap())
                    .unwrap(),
                1,
                "RunResponse.sdk_event must not consume StreamGuestEvents backlog"
            );

            let mut stream = svc
                .stream_guest_events(Request::new(proto::StreamGuestEventsRequest {
                    lease: Some(lease.clone()),
                    streams: vec![beacon],
                }))
                .await
                .unwrap()
                .into_inner();
            let streamed = stream
                .as_mut()
                .next()
                .await
                .expect("one streamed event")
                .unwrap();
            assert_eq!(&streamed, sdk_event);
            assert!(stream.as_mut().next().await.is_none());

            svc.destroy_vm(Request::new(proto::DestroyVmRequest { lease: Some(lease) }))
                .await
                .unwrap();
        });
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn stream_guest_events_retains_filtered_events_and_consumes_on_cancel() {
        if !runtime_tests_available() {
            return;
        }
        let image_cache = tempfile::TempDir::new().unwrap();
        let base_hash = write_cache_blob(image_cache.path(), &vec![0u8; 4096]);
        let kernel_hash = write_cache_blob(image_cache.path(), nanokernel::device_exercise_elf());
        let svc = WorkerService::new(test_config_with_resources(
            1,
            image_cache.path().to_path_buf(),
            None,
        ))
        .unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            use tokio_stream::StreamExt;

            let created = svc
                .create_vm(Request::new(proto::CreateVmRequest {
                    config: Some(device_exercise_machine_config(base_hash, kernel_hash)),
                    entropy_seed: vec![0xA7; 32],
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

            let mut filtered = svc
                .stream_guest_events(Request::new(proto::StreamGuestEventsRequest {
                    lease: Some(lease.clone()),
                    streams: vec![detguest_wire::record::EventKind::FrameMark as u32],
                }))
                .await
                .unwrap()
                .into_inner();
            assert!(filtered.as_mut().next().await.is_none());
            let retained_after_filter = svc
                .inner
                .runtimes
                .with(lease.slot_id, |actor| {
                    actor
                        .with_runtime(|runtime| runtime.guest_events.len())
                        .unwrap()
                })
                .unwrap();
            assert_eq!(retained_after_filter, 1);

            let response = svc
                .stream_guest_events(Request::new(proto::StreamGuestEventsRequest {
                    lease: Some(lease.clone()),
                    streams: vec![detguest_wire::record::EventKind::Beacon as u32],
                }))
                .await
                .unwrap();
            drop(response);
            let retained_after_cancel = svc
                .inner
                .runtimes
                .with(lease.slot_id, |actor| {
                    actor
                        .with_runtime(|runtime| runtime.guest_events.len())
                        .unwrap()
                })
                .unwrap();
            assert_eq!(retained_after_cancel, 0);

            let mut after_cancel = svc
                .stream_guest_events(Request::new(proto::StreamGuestEventsRequest {
                    lease: Some(lease.clone()),
                    streams: vec![detguest_wire::record::EventKind::Beacon as u32],
                }))
                .await
                .unwrap()
                .into_inner();
            assert!(after_cancel.as_mut().next().await.is_none());

            svc.destroy_vm(Request::new(proto::DestroyVmRequest { lease: Some(lease) }))
                .await
                .unwrap();
        });
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn m6_accept_capture_neutrality_and_layout_precondition() {
        if !runtime_tests_available() {
            return;
        }

        let (plain_epoch_out, plain_epochs, plain_post_capture_hash) = capture_epoch_leg(false);
        let (captured_epoch_out, captured_epochs, captured_post_capture_hash) =
            capture_epoch_leg(true);
        assert!(
            !plain_epochs.is_empty(),
            "acceptance fixture must exercise epoch hash records"
        );
        assert_eq!(captured_epoch_out.state_hash, plain_epoch_out.state_hash);
        assert_eq!(
            captured_epochs, plain_epochs,
            "capture must not perturb epoch hashes"
        );
        assert_eq!(
            captured_post_capture_hash, plain_post_capture_hash,
            "capture must not perturb state/device hash after the capture boundary"
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
                    base: Some(base_snapshot.clone()),
                    log: Some(VerifyLog::InputLogId(snap.input_log_id)),
                    bisect_on_divergence: Some(false),
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
                    config: Some(service_machine_config_with_mem_epoch_len(
                        base_hash,
                        kernel_hash,
                        2 * 1024 * 1024,
                        10_000,
                    )),
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
                    base: Some(base_snapshot.clone()),
                    log: Some(VerifyLog::InputLogId(snap.input_log_id)),
                    bisect_on_divergence: Some(true),
                }))
                .await
                .unwrap()
                .into_inner();
            let mut done = None;
            let mut epoch_ok_count = 0;
            while let Some(item) = stream.next().await {
                match item.unwrap().msg.unwrap() {
                    VerifyMsg::EpochOk(_) => {
                        assert!(done.is_none(), "EpochOk must precede terminal Done");
                        epoch_ok_count += 1;
                    }
                    VerifyMsg::Done(msg) => done = Some(msg),
                    VerifyMsg::Divergence(div) => panic!("unexpected divergence: {div:?}"),
                }
            }
            assert!(
                epoch_ok_count > 0,
                "VerifyReplay must stream epoch progress before Done"
            );
            let done = done.expect("VerifyReplay must stream Done");
            assert_eq!(done.total_icount, 50_000);
            assert_eq!(done.end_state_hash.unwrap().hash.len(), 32);
        });
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn verify_replay_rpc_streams_done_for_bisection_checkpoint_log() {
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
        let mut config =
            test_config_with_resources(2, image_cache.path().to_path_buf(), Some(transport));
        config.bisection_checkpoints = BisectionCheckpointConfig::every_epoch();
        let svc = WorkerService::new(config).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let created = svc
                .create_vm(Request::new(proto::CreateVmRequest {
                    config: Some(service_machine_config_with_mem_epoch_len(
                        base_hash,
                        kernel_hash,
                        2 * 1024 * 1024,
                        10_000,
                    )),
                    entropy_seed: vec![0xBC; 32],
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
            assert!(
                !bisection_checkpoint_aux_records(&log_bytes).is_empty(),
                "fixture must carry bisection checkpoint evidence"
            );

            let mut stream = svc
                .verify_replay(Request::new(proto::VerifyReplayRequest {
                    base: Some(base_snapshot),
                    log: Some(VerifyLog::InputLog(log_bytes)),
                    bisect_on_divergence: Some(true),
                }))
                .await
                .unwrap()
                .into_inner();
            let mut done = None;
            let mut epoch_ok_count = 0;
            while let Some(item) = stream.next().await {
                match item.unwrap().msg.unwrap() {
                    VerifyMsg::EpochOk(_) => {
                        assert!(done.is_none(), "EpochOk must precede terminal Done");
                        epoch_ok_count += 1;
                    }
                    VerifyMsg::Done(msg) => done = Some(msg),
                    VerifyMsg::Divergence(div) => panic!("unexpected divergence: {div:?}"),
                }
            }
            assert!(
                epoch_ok_count > 0,
                "VerifyReplay must stream epoch progress before Done"
            );
            let done = done.expect("VerifyReplay must stream Done");
            assert_eq!(done.total_icount, 50_000);
            assert_eq!(done.end_state_hash.unwrap().hash.len(), 32);

            svc.destroy_vm(Request::new(proto::DestroyVmRequest { lease: Some(lease) }))
                .await
                .unwrap();
        });
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn verify_replay_rpc_streams_divergence_for_semantically_bad_log() {
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
                    config: Some(service_machine_config_with_mem_epoch_len(
                        base_hash,
                        kernel_hash,
                        8 * 1024 * 1024,
                        10_000,
                    )),
                    entropy_seed: vec![0xB1; 32],
                }))
                .await
                .unwrap()
                .into_inner();
            let lease = created.lease.unwrap();
            let base_snapshot = svc
                .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                    lease: Some(lease.clone()),
                    seal_input_log: Some(true),
                    capture: None,
                }))
                .await
                .unwrap()
                .into_inner()
                .snapshot
                .unwrap();

            svc.runtime_table()
                .with(lease.slot_id, |actor| {
                    actor
                        .with_runtime_mut(|runtime| {
                            use vm_memory::{Bytes, GuestAddress};
                            runtime
                                .slot
                                .guest_mem
                                .write_slice(&[0xDD; 64], GuestAddress(0x60_0000))
                                .unwrap();
                        })
                        .unwrap()
                })
                .unwrap();

            let run = svc
                .run(Request::new(proto::RunRequest {
                    lease: Some(lease.clone()),
                    until: Some(proto::run_request::Until::IcountBudget(100_000)),
                    hard_icount_cap: 0,
                    capture: None,
                }))
                .await
                .unwrap()
                .into_inner();
            assert_eq!(run.icount, 100_000);

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
            let input_log_id = snap.input_log_id;
            let poisoned_log = tokio::task::spawn_blocking(move || {
                stored_input_log_payload(&svc_for_log, input_log_id)
            })
            .await
            .unwrap();

            let mut stream = svc
                .verify_replay(Request::new(proto::VerifyReplayRequest {
                    base: Some(base_snapshot.clone()),
                    log: Some(VerifyLog::InputLog(poisoned_log.clone())),
                    bisect_on_divergence: Some(false),
                }))
                .await
                .unwrap()
                .into_inner();
            let mut saw_divergence = false;
            while let Some(item) = stream.next().await {
                match item.unwrap().msg.unwrap() {
                    VerifyMsg::EpochOk(_) => {}
                    VerifyMsg::Done(done) => panic!("expected divergence, got Done {done:?}"),
                    VerifyMsg::Divergence(divergence) => {
                        assert!(divergence.suspected_cause.contains("EPOCH_HASH"));
                        assert!(divergence.suspected_cause.contains("chain value"));
                        assert_ne!(divergence.icount_lo, 0);
                        saw_divergence = true;
                    }
                }
            }
            assert!(saw_divergence, "VerifyReplay must stream a divergence");

            for (bisect_on_divergence, label) in [(Some(true), "explicit"), (None, "default")] {
                let mut stream = svc
                    .verify_replay(Request::new(proto::VerifyReplayRequest {
                        base: Some(base_snapshot.clone()),
                        log: Some(VerifyLog::InputLog(poisoned_log.clone())),
                        bisect_on_divergence,
                    }))
                    .await
                    .unwrap()
                    .into_inner();
                let mut saw_checkpoint_error = false;
                while let Some(item) = stream.next().await {
                    match item {
                        Ok(progress) => {
                            if matches!(progress.msg, Some(VerifyMsg::Divergence(_))) {
                                panic!("{label} bisection must not emit fabricated Divergence");
                            }
                        }
                        Err(status) => {
                            assert_eq!(status.code(), tonic::Code::FailedPrecondition);
                            assert!(status.message().contains("checkpoint evidence"));
                            saw_checkpoint_error = true;
                        }
                    }
                }
                assert!(
                    saw_checkpoint_error,
                    "{label} bisection must fail without checkpoint evidence"
                );
            }

            svc.destroy_vm(Request::new(proto::DestroyVmRequest { lease: Some(lease) }))
                .await
                .unwrap();
        });
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn verify_replay_rpc_streams_bisection_divergence_with_checkpoint_evidence() {
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
        let mut config =
            test_config_with_resources(2, image_cache.path().to_path_buf(), Some(transport));
        config.bisection_checkpoints = BisectionCheckpointConfig::every_epoch();
        let svc = WorkerService::new(config).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let created = svc
                .create_vm(Request::new(proto::CreateVmRequest {
                    config: Some(service_machine_config_with_mem_epoch_len(
                        base_hash,
                        kernel_hash,
                        8 * 1024 * 1024,
                        10_000,
                    )),
                    entropy_seed: vec![0xB6; 32],
                }))
                .await
                .unwrap()
                .into_inner();
            let lease = created.lease.unwrap();
            let base_snapshot = svc
                .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                    lease: Some(lease.clone()),
                    seal_input_log: Some(true),
                    capture: None,
                }))
                .await
                .unwrap()
                .into_inner()
                .snapshot
                .unwrap();

            svc.runtime_table()
                .with(lease.slot_id, |actor| {
                    actor
                        .with_runtime_mut(|runtime| {
                            use vm_memory::{Bytes, GuestAddress};
                            runtime
                                .slot
                                .guest_mem
                                .write_slice(&[0xDD; 64], GuestAddress(0x60_0000))
                                .unwrap();
                        })
                        .unwrap()
                })
                .unwrap();

            let run = svc
                .run(Request::new(proto::RunRequest {
                    lease: Some(lease.clone()),
                    until: Some(proto::run_request::Until::IcountBudget(100_000)),
                    hard_icount_cap: 0,
                    capture: None,
                }))
                .await
                .unwrap()
                .into_inner();
            assert_eq!(run.icount, 100_000);

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
            let input_log_id = snap.input_log_id;
            let poisoned_log = tokio::task::spawn_blocking(move || {
                stored_input_log_payload(&svc_for_log, input_log_id)
            })
            .await
            .unwrap();

            let mut stream = svc
                .verify_replay(Request::new(proto::VerifyReplayRequest {
                    base: Some(base_snapshot.clone()),
                    log: Some(VerifyLog::InputLog(poisoned_log.clone())),
                    bisect_on_divergence: Some(true),
                }))
                .await
                .unwrap()
                .into_inner();
            let mut divergence = None;
            while let Some(item) = stream.next().await {
                match item.unwrap().msg.unwrap() {
                    VerifyMsg::EpochOk(_) => {}
                    VerifyMsg::Done(done) => panic!("expected bisection divergence, got {done:?}"),
                    VerifyMsg::Divergence(div) => divergence = Some(div),
                }
            }
            let divergence = divergence.expect("VerifyReplay must stream a bisection divergence");
            assert_eq!(divergence.first_bad_epoch, 1);
            assert_eq!(divergence.icount_lo, 0);
            assert_eq!(divergence.icount_hi, 10_000);
            assert_eq!(divergence.icount_hi - divergence.icount_lo, 10_000);
            assert_ne!(
                divergence.icount_hi - divergence.icount_lo,
                1024,
                "bisection evidence must not fabricate the old coarse 1024-instruction window"
            );
            assert_ne!(
                divergence.rip_expected, 0,
                "RIP fields must come from compared snapshots"
            );
            assert_eq!(
                divergence.rip_expected, divergence.rip_actual,
                "this memory-only divergence should not fabricate a register/RIP mismatch"
            );
            assert!(
                !divergence.reg_diff.is_empty(),
                "register diff must come from the expected-vs-actual snapshot comparison"
            );
            let decoded_reg_diff: Vec<crate::snapshot_compare::RegDiff> =
                postcard::from_bytes(&divergence.reg_diff).unwrap();
            assert!(
                decoded_reg_diff.is_empty(),
                "memory-only divergence should encode an empty register diff, got {decoded_reg_diff:?}"
            );
            assert!(divergence
                .diff_page_idx
                .contains(&(0x60_0000u64 / snapstore_types::PAGE_SIZE as u64)));
            assert!(divergence
                .suspected_cause
                .contains("replay-vs-recorded:EPOCH_HASH chain value"));
            assert!(divergence
                .suspected_cause
                .contains("evidence_mode=replay-vs-recorded"));
            assert!(divergence
                .suspected_cause
                .contains("expected_checkpoint_ref="));
            assert!(divergence.suspected_cause.contains("actual_probe_ref="));

            let wide_log = skip_first_checkpoint_and_widen_second(&poisoned_log);
            let mut stream = svc
                .verify_replay(Request::new(proto::VerifyReplayRequest {
                    base: Some(base_snapshot),
                    log: Some(VerifyLog::InputLog(wide_log)),
                    bisect_on_divergence: Some(true),
                }))
                .await
                .unwrap()
                .into_inner();
            let mut wide_divergence = None;
            while let Some(item) = stream.next().await {
                match item.unwrap().msg.unwrap() {
                    VerifyMsg::EpochOk(_) => {}
                    VerifyMsg::Done(done) => {
                        panic!("expected wide bisection divergence, got {done:?}")
                    }
                    VerifyMsg::Divergence(div) => wide_divergence = Some(div),
                }
            }
            let wide_divergence =
                wide_divergence.expect("VerifyReplay must stream a wide bisection divergence");
            assert_eq!(wide_divergence.first_bad_epoch, 1);
            assert_eq!(wide_divergence.icount_lo, 0);
            assert_eq!(wide_divergence.icount_hi, 20_000);
            assert!(wide_divergence
                .diff_page_idx
                .contains(&(0x60_0000u64 / snapstore_types::PAGE_SIZE as u64)));
            assert!(wide_divergence
                .suspected_cause
                .contains("evidence_mode=replay-vs-recorded"));

            svc.destroy_vm(Request::new(proto::DestroyVmRequest { lease: Some(lease) }))
                .await
                .unwrap();
        });
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn verify_replay_rpc_rejects_invalid_bisection_checkpoint_gap() {
        use proto::verify_replay_request::Log as VerifyLog;
        use tokio_stream::StreamExt;

        let config =
            machine_config_from_proto(&service_machine_config([0x01; 32], [0x02; 32])).unwrap();
        let recorded_base = snapstore_types::SnapshotRef::from_bytes([0x10; 32]);
        let mut writer = new_segment_log(&config, Some(&recorded_base), [0x30; 32]).unwrap();
        writer.epoch_hash(20, 0x2000, 1, [0x01; 32]).unwrap();
        writer
            .bisection_checkpoint(20, 0x2000, 10, [0xEE; 32], 20)
            .unwrap();
        let log = writer
            .seal(dh_inputlog::dhilog::SealParams {
                end_snapshot_id: [0; 32],
                end_icount: 20,
                end_vns: 20,
                end_state_hash: [0x44; 32],
                stop_reason: 0,
            })
            .unwrap();

        let image_cache = tempfile::TempDir::new().unwrap();
        let (_store_rt, _handle, _store_dir, transport) = spawn_store_for_service_test();
        let svc = WorkerService::new(test_config_with_resources(
            1,
            image_cache.path().to_path_buf(),
            Some(transport),
        ))
        .unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut stream = svc
                .verify_replay(Request::new(proto::VerifyReplayRequest {
                    base: Some(proto::SnapshotRef {
                        hash: recorded_base.to_bytes().to_vec(),
                    }),
                    log: Some(VerifyLog::InputLog(log)),
                    bisect_on_divergence: Some(true),
                }))
                .await
                .unwrap()
                .into_inner();

            let mut saw_gap_error = false;
            while let Some(item) = stream.next().await {
                match item {
                    Ok(progress) => panic!("invalid bisection checkpoint streamed {progress:?}"),
                    Err(status) => {
                        assert_eq!(status.code(), tonic::Code::FailedPrecondition);
                        assert!(status.message().contains("checkpoint index invalid"));
                        assert!(status.message().contains("max_covered_gap 10"));
                        assert!(status.message().contains("requires 20"));
                        saw_gap_error = true;
                    }
                }
            }
            assert!(
                saw_gap_error,
                "VerifyReplay must fail publicly on invalid checkpoint gap metadata"
            );
        });
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn verify_replay_rpc_cancellation_releases_worker_slot() {
        if !runtime_tests_available() {
            return;
        }
        use proto::verify_replay_request::Log as VerifyLog;

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
            let epoch_len = 10_000;
            let run_budget = (VERIFY_REPLAY_PROGRESS_BUFFER as u64 + 8) * epoch_len;
            let created = svc
                .create_vm(Request::new(proto::CreateVmRequest {
                    config: Some(service_machine_config_with_mem_epoch_len(
                        base_hash,
                        kernel_hash,
                        8 * 1024 * 1024,
                        epoch_len,
                    )),
                    entropy_seed: vec![0xB2; 32],
                }))
                .await
                .unwrap()
                .into_inner();
            let lease = created.lease.unwrap();
            let base_snapshot = svc
                .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                    lease: Some(lease.clone()),
                    seal_input_log: Some(true),
                    capture: None,
                }))
                .await
                .unwrap()
                .into_inner()
                .snapshot
                .unwrap();
            let run = svc
                .run(Request::new(proto::RunRequest {
                    lease: Some(lease.clone()),
                    until: Some(proto::run_request::Until::IcountBudget(run_budget)),
                    hard_icount_cap: 0,
                    capture: None,
                }))
                .await
                .unwrap()
                .into_inner();
            assert_eq!(run.icount, run_budget);
            let snap = svc
                .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                    lease: Some(lease.clone()),
                    seal_input_log: Some(true),
                    capture: None,
                }))
                .await
                .unwrap()
                .into_inner();
            svc.destroy_vm(Request::new(proto::DestroyVmRequest { lease: Some(lease) }))
                .await
                .unwrap();
            assert_eq!(svc.slots_free(), 2);

            let response = svc
                .verify_replay(Request::new(proto::VerifyReplayRequest {
                    base: Some(base_snapshot),
                    log: Some(VerifyLog::InputLogId(snap.input_log_id)),
                    bisect_on_divergence: Some(false),
                }))
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert_eq!(
                svc.slots_free(),
                1,
                "unconsumed VerifyReplay stream should hold the temporary slot"
            );
            drop(response);

            for _ in 0..100 {
                if svc.slots_free() == 2 {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            panic!("VerifyReplay cancellation did not release its worker slot");
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
                bisect_on_divergence: Some(false),
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
                    bisect_on_divergence: Some(false),
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
    fn verify_replay_header_mismatch_precedes_bisection_ref_validation() {
        let config =
            machine_config_from_proto(&service_machine_config([0x01; 32], [0x02; 32])).unwrap();
        let recorded_base = snapstore_types::SnapshotRef::from_bytes([0x10; 32]);
        let requested_base = snapstore_types::SnapshotRef::from_bytes([0x20; 32]);
        let mut writer = new_segment_log(&config, Some(&recorded_base), [0x30; 32]).unwrap();
        writer.epoch_hash(20, 0x2000, 1, [0x01; 32]).unwrap();
        writer
            .bisection_checkpoint(20, 0x2000, 20, [0xEE; 32], 20)
            .unwrap();
        let log = writer
            .seal(dh_inputlog::dhilog::SealParams {
                end_snapshot_id: [0; 32],
                end_icount: 20,
                end_vns: 20,
                end_state_hash: [0x44; 32],
                stop_reason: 0,
            })
            .unwrap();
        let reader = dh_inputlog::reader::LogReader::parse(&log).unwrap();
        let header = reader.header().clone();
        let index = crate::bisection_index::BisectionCheckpointIndex::from_reader(&reader)
            .expect("test log has valid checkpoint index");

        let mut ref_validator_called = false;
        let err = validate_verify_replay_header_and_bisection_refs(
            &header,
            &requested_base,
            &config,
            Some(&index),
            |_snapshot_ref| {
                ref_validator_called = true;
                Err::<(), &str>("checkpoint ref should not be validated after header mismatch")
            },
        )
        .unwrap_err();

        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert!(err.message().contains("base_snapshot_id"));
        assert!(
            !ref_validator_called,
            "checkpoint refs must not be dereferenced before header identity is validated"
        );
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
        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert!(err.message().contains("bisection checkpoints"));

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

        let refined = dh_verify::verify::BisectionDivergence {
            first_bad_epoch: Some(4),
            icount_lo: 40_960,
            icount_hi: 41_472,
            rip_expected: 0x401000,
            rip_actual: 0x401004,
            reg_diff: vec![0xA1, 0xB2],
            diff_page_idx: vec![17, 19],
            suspected_cause: "RDTSC at divergent RIP".into(),
            evidence: dh_verify::verify::BisectionEvidence {
                mode: dh_verify::verify::BisectionMode::ReplayVsRecorded,
                expected_checkpoint_ref: Some([0xE1; 32]),
                actual_probe_ref: Some([0xA7; 32]),
                coverage_icount_lo: 40_960,
                coverage_icount_hi: 41_472,
            },
        };

        let mut narrowed = refined.clone();
        narrowed.icount_lo += 1;
        let err = verify_progress_to_proto(VerifyProgress::BisectionDivergence(narrowed), true)
            .unwrap_err();
        assert_eq!(err.code(), tonic::Code::Internal);
        assert!(err.message().contains("must match its evidence window"));

        let progress =
            verify_progress_to_proto(VerifyProgress::BisectionDivergence(refined), true).unwrap();
        let div = match progress.msg.unwrap() {
            VerifyMsg::Divergence(div) => div,
            other => panic!("expected refined Divergence, got {other:?}"),
        };
        assert_eq!(div.first_bad_epoch, 4);
        assert_eq!(div.icount_lo, 40_960);
        assert_eq!(div.icount_hi, 41_472);
        assert_eq!(div.rip_expected, 0x401000);
        assert_eq!(div.rip_actual, 0x401004);
        assert_eq!(div.reg_diff, vec![0xA1, 0xB2]);
        assert_eq!(div.diff_page_idx, vec![17, 19]);
        assert!(div
            .suspected_cause
            .contains("evidence_mode=replay-vs-recorded"));
        assert!(div.suspected_cause.contains("expected_checkpoint_ref="));
        assert!(div.suspected_cause.contains("actual_probe_ref="));
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
    #[test]
    fn linux_cpu_compat_config_hash_requires_installed_cpuid_table_live() {
        if !runtime_tests_available() {
            return;
        }
        let sys = dh_vmm::kvm::KvmSystem::open().unwrap();
        let slot = sys.create_slot_vm(2 * 1024 * 1024).unwrap();
        let mut config = dh_vmm::config::MachineConfig::new(
            2 * 1024 * 1024,
            [0x44; 32],
            dh_vmm::config::BootSpec::Elf {
                kernel_hash: [0x55; 32],
                cmdline: Vec::new(),
            },
        );
        config.device_set = vec![
            dh_devices::clock::DEVICE_ID_PV_CLOCK,
            dh_devices::pad::DEVICE_ID_PV_PAD,
            dh_devices::entropy::DEVICE_ID_PV_ENTROPY,
            dh_devices::serial::DEVICE_ID_DEBUG_SERIAL,
        ];

        let err = config_hash_for_slot(&config, &slot).unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("cpuid_table does not match"));

        config.cpuid_table = slot.cpuid_table.clone();
        let hash = config_hash_for_slot(&config, &slot).unwrap();
        assert_eq!(hash, config.config_hash().unwrap());

        config.cpuid_table[0].eax ^= 1;
        let err = config_hash_for_slot(&config, &slot).unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("cpuid_table does not match"));
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
