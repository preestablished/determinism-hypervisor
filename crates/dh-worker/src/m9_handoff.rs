use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
#[cfg(unix)]
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use dh_proto::v1 as proto;
use dh_proto::v1::hypervisor_worker_server::HypervisorWorker;
use dh_vmm::SlotState;
use snapstore_client::blocking::SnapstoreClient as BlockingSnapstoreClient;
use snapstore_client::Transport;
use snapstore_manifest::Manifest;
use snapstore_server::build_server::ServerHandle;
use snapstore_server::config::ServerConfig;
use tonic::Request;

pub const DH_M9_BZIMAGE: &str = "DH_M9_BZIMAGE";
pub const DH_M9_INITRAMFS: &str = "DH_M9_INITRAMFS";
pub const DH_M9_BASE_IMAGE: &str = "DH_M9_BASE_IMAGE";
pub const DH_M9_GAME_IMAGE: &str = "DH_M9_GAME_IMAGE";
pub const DH_M9_IMAGE_CACHE: &str = "DH_M9_IMAGE_CACHE";
pub const M9_LINUX_ARTIFACT_ENV_VARS: [&str; 5] = [
    DH_M9_BZIMAGE,
    DH_M9_INITRAMFS,
    DH_M9_BASE_IMAGE,
    DH_M9_GAME_IMAGE,
    DH_M9_IMAGE_CACHE,
];

pub const M9_LINUX_MEM_BYTES: u64 = 128 * 1024 * 1024;
pub const M9_READY_HARD_CAP: u64 = 10_000_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct M9LinuxArtifacts {
    pub bzimage: PathBuf,
    pub initramfs: PathBuf,
    pub base_image: PathBuf,
    pub game_image: PathBuf,
    pub image_cache: PathBuf,
}

impl M9LinuxArtifacts {
    pub fn from_env_required() -> HandoffResult<Self> {
        let artifacts = Self::from_lookup(|name| std::env::var_os(name))?;
        artifacts.validate_paths()?;
        Ok(artifacts)
    }

    pub fn from_lookup<F>(mut lookup: F) -> HandoffResult<Self>
    where
        F: FnMut(&str) -> Option<OsString>,
    {
        let mut missing = Vec::new();
        let mut required = |name: &'static str| -> Option<PathBuf> {
            match lookup(name) {
                Some(value) if !value.is_empty() => Some(PathBuf::from(value)),
                _ => {
                    missing.push(name);
                    None
                }
            }
        };

        let bzimage = required(DH_M9_BZIMAGE);
        let initramfs = required(DH_M9_INITRAMFS);
        let base_image = required(DH_M9_BASE_IMAGE);
        let game_image = required(DH_M9_GAME_IMAGE);
        let image_cache = required(DH_M9_IMAGE_CACHE);

        if !missing.is_empty() {
            return Err(HandoffError::new(
                "load M9 artifact environment",
                format!(
                    "missing required artifact env vars: {}. Set all of {}. DH_M9_ALLOW_SKIP=1 is not accepted for this handoff generator.",
                    missing.join(", "),
                    M9_LINUX_ARTIFACT_ENV_VARS.join(", ")
                ),
            ));
        }

        Ok(Self {
            bzimage: bzimage.expect("missing handled above"),
            initramfs: initramfs.expect("missing handled above"),
            base_image: base_image.expect("missing handled above"),
            game_image: game_image.expect("missing handled above"),
            image_cache: image_cache.expect("missing handled above"),
        })
    }

    fn validate_paths(&self) -> HandoffResult<()> {
        require_regular_file(DH_M9_BZIMAGE, &self.bzimage)?;
        require_regular_file(DH_M9_INITRAMFS, &self.initramfs)?;
        require_regular_file(DH_M9_BASE_IMAGE, &self.base_image)?;
        require_regular_file(DH_M9_GAME_IMAGE, &self.game_image)?;
        require_directory(DH_M9_IMAGE_CACHE, &self.image_cache)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct M9CachedHashes {
    pub bzimage: [u8; 32],
    pub initramfs: [u8; 32],
    pub base_image: [u8; 32],
    pub game_image: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandoffArgs {
    pub private_root: PathBuf,
    pub snapstore_data_root: PathBuf,
    pub snapstore_uds: PathBuf,
    pub reference_workload_checkout: PathBuf,
    pub workload_manifest: PathBuf,
    pub bridge_hypervisor_endpoint: String,
    pub bridge_private_root: PathBuf,
    pub bridge_workload_image_ref: String,
    pub bridge_capture_spec_ref: String,
    pub handoff_env: PathBuf,
    pub snapstore_config: PathBuf,
    pub public_summary: PathBuf,
    pub slot_cores: Option<Vec<u32>>,
}

impl HandoffArgs {
    fn private_literals(
        &self,
        artifacts: &M9LinuxArtifacts,
        snapshot_ref: Option<&str>,
    ) -> Vec<String> {
        let mut values = vec![
            self.private_root.display().to_string(),
            self.snapstore_data_root.display().to_string(),
            self.snapstore_uds.display().to_string(),
            restart_snapstore_uds(self).display().to_string(),
            self.bridge_hypervisor_endpoint.clone(),
            self.bridge_private_root.display().to_string(),
            self.bridge_workload_image_ref.clone(),
            self.bridge_capture_spec_ref.clone(),
            self.handoff_env.display().to_string(),
            self.snapstore_config.display().to_string(),
            artifacts.bzimage.display().to_string(),
            artifacts.initramfs.display().to_string(),
            artifacts.base_image.display().to_string(),
            artifacts.game_image.display().to_string(),
            artifacts.image_cache.display().to_string(),
        ];
        if let Some(snapshot_ref) = snapshot_ref {
            values.push(snapshot_ref.to_owned());
        }
        values
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandoffReport {
    pub snapshot_ref_hex: String,
    pub slots_total: usize,
    pub slots_free_before: usize,
    pub slots_free_after: usize,
}

#[derive(Debug)]
pub struct HandoffError {
    stage: &'static str,
    private_detail: String,
}

pub type HandoffResult<T> = Result<T, HandoffError>;

impl HandoffError {
    pub fn new(stage: &'static str, private_detail: impl Into<String>) -> Self {
        Self {
            stage,
            private_detail: private_detail.into(),
        }
    }

    pub fn public_message(&self) -> String {
        format!(
            "failed during {}; see private evidence for details",
            self.stage
        )
    }

    pub fn private_detail(&self) -> &str {
        &self.private_detail
    }

    pub fn write_private_log(&self, private_root: &Path) -> std::io::Result<()> {
        if !is_safe_private_root_for_log(private_root) {
            return Ok(());
        }
        let evidence_dir = private_root.join("rom-bridge-o73").join("evidence");
        fs::create_dir_all(&evidence_dir)?;
        #[cfg(unix)]
        fs::set_permissions(&evidence_dir, fs::Permissions::from_mode(0o700))?;
        let path = evidence_dir.join("dh-m9-ready-handoff-error.private.log");
        write_atomic(
            &path,
            format!("stage: {}\n{}\n", self.stage, self.private_detail).as_bytes(),
            0o600,
        )
    }
}

pub fn parse_args<I>(args: I) -> HandoffResult<HandoffArgs>
where
    I: IntoIterator<Item = String>,
{
    let args: Vec<String> = args.into_iter().collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        return Err(HandoffError::new("parse arguments", usage()));
    }

    let mut private_root = None;
    let mut snapstore_data_root = None;
    let mut snapstore_uds = None;
    let mut reference_workload_checkout = None;
    let mut workload_manifest = None;
    let mut bridge_hypervisor_endpoint = None;
    let mut bridge_private_root = None;
    let mut bridge_workload_image_ref = None;
    let mut bridge_capture_spec_ref = None;
    let mut handoff_env = None;
    let mut snapstore_config = None;
    let mut public_summary = None;
    let mut slot_cores = None;

    let mut i = 0;
    while i < args.len() {
        let flag = args[i].as_str();
        let mut value = || -> HandoffResult<String> {
            i += 1;
            args.get(i).cloned().ok_or_else(|| {
                HandoffError::new("parse arguments", format!("{flag} requires a value"))
            })
        };
        match flag {
            "--private-root" => set_path_once(&mut private_root, flag, value()?)?,
            "--snapstore-data-root" => set_path_once(&mut snapstore_data_root, flag, value()?)?,
            "--snapstore-uds" => set_path_once(&mut snapstore_uds, flag, value()?)?,
            "--reference-workload-checkout" => {
                set_path_once(&mut reference_workload_checkout, flag, value()?)?
            }
            "--workload-manifest" => set_path_once(&mut workload_manifest, flag, value()?)?,
            "--bridge-hypervisor-endpoint" => {
                set_string_once(&mut bridge_hypervisor_endpoint, flag, value()?)?
            }
            "--bridge-private-root" => set_path_once(&mut bridge_private_root, flag, value()?)?,
            "--bridge-workload-image-ref" => {
                set_string_once(&mut bridge_workload_image_ref, flag, value()?)?
            }
            "--bridge-capture-spec-ref" => {
                set_string_once(&mut bridge_capture_spec_ref, flag, value()?)?
            }
            "--handoff-env" => set_path_once(&mut handoff_env, flag, value()?)?,
            "--snapstore-config" => set_path_once(&mut snapstore_config, flag, value()?)?,
            "--public-summary" => set_path_once(&mut public_summary, flag, value()?)?,
            "--slot-cores" => {
                let value = value()?;
                if slot_cores.is_some() {
                    return Err(HandoffError::new(
                        "parse arguments",
                        "--slot-cores supplied more than once",
                    ));
                }
                slot_cores =
                    Some(crate::slot_manager::parse_core_list(&value).ok_or_else(|| {
                        HandoffError::new("parse arguments", "invalid --slot-cores core list")
                    })?);
            }
            other => {
                return Err(HandoffError::new(
                    "parse arguments",
                    format!("unknown argument {other}\n{}", usage()),
                ));
            }
        }
        i += 1;
    }

    let parsed = HandoffArgs {
        private_root: required_path(private_root, "--private-root")?,
        snapstore_data_root: required_path(snapstore_data_root, "--snapstore-data-root")?,
        snapstore_uds: required_path(snapstore_uds, "--snapstore-uds")?,
        reference_workload_checkout: required_path(
            reference_workload_checkout,
            "--reference-workload-checkout",
        )?,
        workload_manifest: required_path(workload_manifest, "--workload-manifest")?,
        bridge_hypervisor_endpoint: required_string(
            bridge_hypervisor_endpoint,
            "--bridge-hypervisor-endpoint",
        )?,
        bridge_private_root: required_path(bridge_private_root, "--bridge-private-root")?,
        bridge_workload_image_ref: required_string(
            bridge_workload_image_ref,
            "--bridge-workload-image-ref",
        )?,
        bridge_capture_spec_ref: required_string(
            bridge_capture_spec_ref,
            "--bridge-capture-spec-ref",
        )?,
        handoff_env: required_path(handoff_env, "--handoff-env")?,
        snapstore_config: required_path(snapstore_config, "--snapstore-config")?,
        public_summary: required_path(public_summary, "--public-summary")?,
        slot_cores,
    };
    validate_no_newline(
        "bridge hypervisor endpoint",
        &parsed.bridge_hypervisor_endpoint,
    )?;
    validate_no_newline(
        "bridge workload image ref",
        &parsed.bridge_workload_image_ref,
    )?;
    validate_no_newline("bridge capture spec ref", &parsed.bridge_capture_spec_ref)?;
    Ok(parsed)
}

pub fn private_root_from_raw_args(args: &[String]) -> Option<PathBuf> {
    args.windows(2)
        .find_map(|pair| (pair[0] == "--private-root").then(|| PathBuf::from(&pair[1])))
}

pub fn run_cli<I, W>(args: I, out: &mut W) -> HandoffResult<HandoffReport>
where
    I: IntoIterator<Item = String>,
    W: Write,
{
    let args = parse_args(args)?;
    let artifacts = M9LinuxArtifacts::from_env_required()?;
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| HandoffError::new("create runtime", e.to_string()))?;
    let report = rt.block_on(run_handoff_with_artifacts(&args, artifacts.clone()))?;
    let summary = public_summary(&report);
    ensure_no_private_literals(
        "public summary",
        &summary,
        &args.private_literals(&artifacts, Some(&report.snapshot_ref_hex)),
    )?;
    write_atomic(&args.public_summary, summary.as_bytes(), 0o644)
        .map_err(|e| HandoffError::new("write public summary", e.to_string()))?;
    out.write_all(summary.as_bytes())
        .map_err(|e| HandoffError::new("write stdout", e.to_string()))?;
    Ok(report)
}

pub async fn run_handoff(args: &HandoffArgs) -> HandoffResult<HandoffReport> {
    let artifacts = M9LinuxArtifacts::from_env_required()?;
    run_handoff_with_artifacts(args, artifacts).await
}

async fn run_handoff_with_artifacts(
    args: &HandoffArgs,
    artifacts: M9LinuxArtifacts,
) -> HandoffResult<HandoffReport> {
    validate_private_layout(args)?;
    validate_reference_workload(args)?;
    prepare_private_dirs(args)?;
    write_snapstore_config(args)?;

    let (preflight_results, preflight_ok) = crate::preflight::run_preflight();
    if !preflight_ok {
        return Err(HandoffError::new(
            "worker preflight",
            "worker preflight failed on this host",
        ));
    }

    let Some(cpuid_table) = masked_cpuid_table()? else {
        return Err(HandoffError::new("probe KVM", "KVM dirty ring unavailable"));
    };
    let hashes = populate_m9_image_cache(&artifacts)?;
    let config = m9_linux_machine_config(&hashes, cpuid_table);
    let config_hash = config
        .config_hash()
        .map_err(|e| HandoffError::new("hash MachineConfig", format!("{e:?}")))?;

    let server = start_snapstore(args, &args.snapstore_uds).await?;
    let ready = create_ready_snapshot(
        args,
        &artifacts,
        &preflight_results,
        &args.snapstore_uds,
        config,
        config_hash,
    )
    .await;
    let stop = stop_snapstore(server, &args.snapstore_uds).await;
    let ready = match (ready, stop) {
        (Err(err), _) => return Err(err),
        (Ok(_), Err(err)) => return Err(err),
        (Ok(ready), Ok(())) => ready,
    };

    let restart_uds = restart_snapstore_uds(args);
    let restart_server = start_snapstore(args, &restart_uds).await?;
    let restore =
        restore_ready_snapshot(args, &artifacts, &preflight_results, &restart_uds, &ready).await;
    let stop = stop_snapstore(restart_server, &restart_uds).await;
    let slots_free_after = match (restore, stop) {
        (Err(err), _) => return Err(err),
        (Ok(_), Err(err)) => return Err(err),
        (Ok(slots_free_after), Ok(())) => slots_free_after,
    };

    let snapshot_ref_hex = hex_lower(&ready.snapshot_ref.hash);
    write_handoff_env(args, &artifacts.image_cache, &snapshot_ref_hex)?;
    Ok(HandoffReport {
        snapshot_ref_hex,
        slots_total: ready.slots_total,
        slots_free_before: ready.slots_free_before,
        slots_free_after,
    })
}

fn validate_reference_workload(args: &HandoffArgs) -> HandoffResult<()> {
    require_directory(
        "reference workload checkout",
        &args.reference_workload_checkout,
    )?;
    require_regular_file("workload manifest", &args.workload_manifest)?;
    reject_existing_symlink_components(
        "reference workload checkout",
        &args.reference_workload_checkout,
    )?;
    reject_existing_symlink_components("workload manifest", &args.workload_manifest)?;
    let checkout = fs::canonicalize(&args.reference_workload_checkout).map_err(|e| {
        HandoffError::new(
            "validate reference workload",
            format!(
                "canonicalize reference workload checkout {}: {e}",
                args.reference_workload_checkout.display()
            ),
        )
    })?;
    let manifest = fs::canonicalize(&args.workload_manifest).map_err(|e| {
        HandoffError::new(
            "validate reference workload",
            format!(
                "canonicalize workload manifest {}: {e}",
                args.workload_manifest.display()
            ),
        )
    })?;
    if !manifest.starts_with(checkout) {
        return Err(HandoffError::new(
            "validate reference workload",
            "workload manifest must live under reference workload checkout",
        ));
    }
    Ok(())
}

fn validate_private_layout(args: &HandoffArgs) -> HandoffResult<()> {
    for (name, path) in [
        ("private root", &args.private_root),
        ("snapstore data root", &args.snapstore_data_root),
        ("snapstore UDS", &args.snapstore_uds),
        ("bridge private root", &args.bridge_private_root),
        ("handoff env", &args.handoff_env),
        ("snapstore config", &args.snapstore_config),
    ] {
        if !path.is_absolute() {
            return Err(HandoffError::new(
                "validate private layout",
                format!("{name} must be absolute: {}", path.display()),
            ));
        }
        reject_existing_symlink_components(name, path)?;
        if is_under_git_checkout(path) {
            return Err(HandoffError::new(
                "validate private layout",
                format!(
                    "{name} must not be inside a git checkout: {}",
                    path.display()
                ),
            ));
        }
    }

    if !args.snapstore_data_root.starts_with(&args.private_root) {
        return Err(HandoffError::new(
            "validate private layout",
            "snapstore data root must live under private root",
        ));
    }
    if !args.snapstore_uds.starts_with(&args.private_root) {
        return Err(HandoffError::new(
            "validate private layout",
            "snapstore UDS must live under private root",
        ));
    }
    if !args.handoff_env.starts_with(&args.private_root) {
        return Err(HandoffError::new(
            "validate private layout",
            "handoff env must live under private root",
        ));
    }
    if !args.snapstore_config.starts_with(&args.private_root) {
        return Err(HandoffError::new(
            "validate private layout",
            "snapstore config must live under private root",
        ));
    }
    validate_public_summary_path(args)?;
    Ok(())
}

fn prepare_private_dirs(args: &HandoffArgs) -> HandoffResult<()> {
    create_private_dir(&args.private_root)?;
    create_private_dir(&args.snapstore_data_root)?;
    if let Some(parent) = args.snapstore_uds.parent() {
        create_private_dir(parent)?;
    }
    if let Some(parent) = args.handoff_env.parent() {
        create_private_dir(parent)?;
    }
    if let Some(parent) = args.snapstore_config.parent() {
        create_private_dir(parent)?;
    }
    create_private_dir(&args.private_root.join("rom-bridge-o73").join("evidence"))?;
    create_private_dir(&args.private_root.join("rom-bridge-o73").join("runtime"))?;
    validate_created_private_layout(args)?;
    Ok(())
}

fn create_private_dir(path: &Path) -> HandoffResult<()> {
    fs::create_dir_all(path).map_err(|e| {
        HandoffError::new(
            "create private directory",
            format!("{}: {e}", path.display()),
        )
    })?;
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| {
        HandoffError::new(
            "set private directory mode",
            format!("{}: {e}", path.display()),
        )
    })?;
    Ok(())
}

fn write_snapstore_config(args: &HandoffArgs) -> HandoffResult<()> {
    let config = format!(
        "data_root = \"{}\"\ngrpc_uds_path = \"{}\"\ngrpc_tcp_addr = \"127.0.0.1:0\"\nhttp_addr = \"127.0.0.1:0\"\n",
        toml_escape_path(&args.snapstore_data_root)?,
        toml_escape_path(&args.snapstore_uds)?,
    );
    write_atomic(&args.snapstore_config, config.as_bytes(), 0o600).map_err(|e| {
        HandoffError::new(
            "write snapstore config",
            format!("{}: {e}", args.snapstore_config.display()),
        )
    })
}

async fn start_snapstore(args: &HandoffArgs, uds_path: &Path) -> HandoffResult<ServerHandle> {
    reject_existing_snapstore_uds(uds_path)?;
    let config = snapstore_server_config(args, uds_path);
    let (handle, _uds) = snapstore_server::build_server::serve_for_tests(config)
        .await
        .map_err(|e| HandoffError::new("start snapstore", e.to_string()))?;
    Ok(handle)
}

async fn stop_snapstore(handle: ServerHandle, uds_path: &Path) -> HandoffResult<()> {
    handle.shutdown();
    wait_for_snapstore_listener_closed(uds_path).await
}

async fn wait_for_snapstore_listener_closed(uds_path: &Path) -> HandoffResult<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match tokio::net::UnixStream::connect(uds_path).await {
            Ok(stream) => {
                drop(stream);
                if Instant::now() >= deadline {
                    return Err(HandoffError::new(
                        "stop snapstore",
                        format!(
                            "snapstore UDS still accepts connections: {}",
                            uds_path.display()
                        ),
                    ));
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(e)
                if e.kind() == ErrorKind::NotFound || e.kind() == ErrorKind::ConnectionRefused =>
            {
                return Ok(());
            }
            Err(_) => return Ok(()),
        }
    }
}

fn snapstore_server_config(args: &HandoffArgs, uds_path: &Path) -> ServerConfig {
    ServerConfig {
        data_root: args.snapstore_data_root.clone(),
        grpc_tcp_addr: "127.0.0.1:0".parse().expect("literal TCP address parses"),
        grpc_uds_path: Some(uds_path.to_path_buf()),
        page_channel_path: None,
        http_addr: "127.0.0.1:0".parse().expect("literal HTTP address parses"),
        pagestore: Default::default(),
        meta: Default::default(),
        page_channel: Default::default(),
    }
}

fn restart_snapstore_uds(args: &HandoffArgs) -> PathBuf {
    args.private_root
        .join("rom-bridge-o73")
        .join("runtime")
        .join("snapstore-restart-verify.sock")
}

fn reject_existing_snapstore_uds(uds_path: &Path) -> HandoffResult<()> {
    match fs::symlink_metadata(uds_path) {
        Ok(meta) => Err(HandoffError::new(
            "validate snapstore UDS",
            format!(
                "snapstore UDS already exists as {kind}; refusing to unlink {path}",
                kind = existing_uds_kind(&meta),
                path = uds_path.display()
            ),
        )),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(HandoffError::new(
            "validate snapstore UDS",
            format!("{}: {e}", uds_path.display()),
        )),
    }
}

#[cfg(unix)]
fn existing_uds_kind(meta: &fs::Metadata) -> &'static str {
    if meta.file_type().is_socket() {
        "socket"
    } else {
        "file"
    }
}

#[cfg(not(unix))]
fn existing_uds_kind(_meta: &fs::Metadata) -> &'static str {
    "file"
}

#[derive(Clone, Debug)]
struct ReadySnapshotOutcome {
    snapshot_ref: proto::SnapshotRef,
    state_hash: proto::StateHash,
    slots_total: usize,
    slots_free_before: usize,
}

fn build_worker_service(
    args: &HandoffArgs,
    artifacts: &M9LinuxArtifacts,
    preflight_results: &[crate::preflight::CheckResult],
    snapstore_uds: &Path,
) -> HandoffResult<crate::service::WorkerService> {
    let mut worker_config = crate::service::WorkerConfig::from_host_defaults()
        .map_err(|e| HandoffError::new("build worker config", format!("{e:?}")))?;
    if let Some(slot_cores) = &args.slot_cores {
        worker_config.slot_cores = slot_cores.clone();
    }
    worker_config.preflight = crate::service::PreflightHealth::passed(preflight_results);
    worker_config.image_cache_dir = artifacts.image_cache.clone();
    worker_config.snapstore = Some(Transport::Uds(snapstore_uds.to_path_buf()));

    crate::service::WorkerService::new(worker_config)
        .map_err(|e| HandoffError::new("create worker service", format!("{e:?}")))
}

async fn create_ready_snapshot(
    args: &HandoffArgs,
    artifacts: &M9LinuxArtifacts,
    preflight_results: &[crate::preflight::CheckResult],
    snapstore_uds: &Path,
    config: dh_vmm::config::MachineConfig,
    config_hash: [u8; 32],
) -> HandoffResult<ReadySnapshotOutcome> {
    let svc = build_worker_service(args, artifacts, preflight_results, snapstore_uds)?;
    let slots_before = slot_counts(&svc);
    let mut source_lease: Option<proto::Lease> = None;

    let lifecycle = async {
        let created = svc
            .create_vm(Request::new(proto::CreateVmRequest {
                config: Some(crate::proto_map::machine_config_to_proto(&config)),
                entropy_seed: vec![0x9A; 32],
            }))
            .await
            .map_err(|e| HandoffError::new("CreateVm", e.to_string()))?
            .into_inner();
        let lease = created
            .lease
            .ok_or_else(|| HandoffError::new("CreateVm", "CreateVm returned no lease"))?;
        source_lease = Some(lease.clone());
        if created.icount != 0 {
            return Err(HandoffError::new(
                "CreateVm",
                format!("CreateVm icount {}, expected 0", created.icount),
            ));
        }

        let initial = svc
            .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                lease: Some(lease.clone()),
                seal_input_log: Some(true),
                capture: None,
            }))
            .await
            .map_err(|e| HandoffError::new("initial TakeSnapshot", e.to_string()))?
            .into_inner();
        if initial
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.hash.len())
            != Some(32)
        {
            return Err(HandoffError::new(
                "initial TakeSnapshot",
                "initial snapshot ref missing or malformed",
            ));
        }

        let run = svc
            .run(Request::new(proto::RunRequest {
                lease: Some(lease.clone()),
                until: Some(proto::run_request::Until::NextSdkEvent(
                    proto::NextSdkEvent {
                        stream: Some(detguest_wire::record::EventKind::Ready as u32),
                    },
                )),
                hard_icount_cap: M9_READY_HARD_CAP,
                capture: None,
            }))
            .await
            .map_err(|e| HandoffError::new("Run until Ready", e.to_string()))?
            .into_inner();
        if run.reason != i32::from(proto::StopReason::NextSdkEvent) {
            return Err(HandoffError::new(
                "Run until Ready",
                format!("stop reason {}, expected NextSdkEvent", run.reason),
            ));
        }
        if run.sdk_event.as_ref().map(|event| event.stream)
            != Some(detguest_wire::record::EventKind::Ready as u32)
        {
            return Err(HandoffError::new(
                "Run until Ready",
                "RunResponse.sdk_event was not Ready",
            ));
        }

        let ready_snapshot = svc
            .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                lease: Some(lease.clone()),
                seal_input_log: Some(true),
                capture: None,
            }))
            .await
            .map_err(|e| HandoffError::new("Ready TakeSnapshot", e.to_string()))?
            .into_inner();
        let ready_snapshot_ref = ready_snapshot
            .snapshot
            .clone()
            .ok_or_else(|| HandoffError::new("Ready TakeSnapshot", "returned no snapshot"))?;
        if ready_snapshot_ref.hash.len() != 32 {
            return Err(HandoffError::new(
                "Ready TakeSnapshot",
                "snapshot ref must be 32 bytes",
            ));
        }
        let ready_state_hash = ready_snapshot
            .state_hash
            .clone()
            .ok_or_else(|| HandoffError::new("Ready TakeSnapshot", "returned no state hash"))?;
        if ready_snapshot.machine_config_hash != config_hash.to_vec() {
            return Err(HandoffError::new(
                "Ready TakeSnapshot",
                "machine_config_hash mismatch",
            ));
        }

        svc.destroy_vm(Request::new(proto::DestroyVmRequest {
            lease: source_lease.clone(),
        }))
        .await
        .map_err(|e| HandoffError::new("DestroyVm source", e.to_string()))?;
        source_lease = None;

        Ok(ReadySnapshotOutcome {
            snapshot_ref: ready_snapshot_ref,
            state_hash: ready_state_hash,
            slots_total: slots_before.0,
            slots_free_before: slots_before.1,
        })
    }
    .await;

    if lifecycle.is_err() {
        if let Some(lease) = source_lease.take() {
            let _ = svc
                .destroy_vm(Request::new(proto::DestroyVmRequest { lease: Some(lease) }))
                .await;
        }
    }

    lifecycle
}

async fn restore_ready_snapshot(
    args: &HandoffArgs,
    artifacts: &M9LinuxArtifacts,
    preflight_results: &[crate::preflight::CheckResult],
    snapstore_uds: &Path,
    ready: &ReadySnapshotOutcome,
) -> HandoffResult<usize> {
    verify_snapshot_manifest(snapstore_uds, &ready.snapshot_ref)?;
    let svc = build_worker_service(args, artifacts, preflight_results, snapstore_uds)?;
    let mut restored_lease: Option<proto::Lease> = None;

    let lifecycle = async {
        let restored = svc
            .restore_snapshot(Request::new(proto::RestoreSnapshotRequest {
                snapshot: Some(ready.snapshot_ref.clone()),
                entropy_seed: Vec::new(),
            }))
            .await
            .map_err(|e| HandoffError::new("RestoreSnapshot", e.to_string()))?
            .into_inner();
        let lease = restored
            .lease
            .clone()
            .ok_or_else(|| HandoffError::new("RestoreSnapshot", "returned no lease"))?;
        restored_lease = Some(lease);
        if restored
            .state_hash
            .as_ref()
            .map(|state| state.hash.as_slice())
            != Some(ready.state_hash.hash.as_slice())
        {
            return Err(HandoffError::new(
                "RestoreSnapshot",
                "restored state hash did not match READY state hash",
            ));
        }

        svc.destroy_vm(Request::new(proto::DestroyVmRequest {
            lease: restored_lease.clone(),
        }))
        .await
        .map_err(|e| HandoffError::new("DestroyVm restored", e.to_string()))?;
        restored_lease = None;

        Ok(slot_counts(&svc).1)
    }
    .await;

    if lifecycle.is_err() {
        if let Some(lease) = restored_lease.take() {
            let _ = svc
                .destroy_vm(Request::new(proto::DestroyVmRequest { lease: Some(lease) }))
                .await;
        }
    }

    lifecycle
}

fn slot_counts(svc: &crate::service::WorkerService) -> (usize, usize) {
    let slots = svc.slot_manager().list();
    let total = slots.len();
    let free = slots
        .iter()
        .filter(|slot| slot.state == SlotState::Empty)
        .count();
    (total, free)
}

fn verify_snapshot_manifest(uds: &Path, snapshot: &proto::SnapshotRef) -> HandoffResult<()> {
    let hash: [u8; 32] = snapshot.hash.as_slice().try_into().map_err(|_| {
        HandoffError::new("verify snapshot manifest", "snapshot ref must be 32 bytes")
    })?;
    let store = BlockingSnapstoreClient::connect(Transport::Uds(uds.to_path_buf()))
        .map_err(|e| HandoffError::new("connect snapstore", e.to_string()))?;
    let container = store
        .get_snapshot(snapstore_types::SnapshotRef::from_bytes(hash))
        .map_err(|e| HandoffError::new("read snapshot manifest", e.to_string()))?;
    Manifest::decode(&container)
        .map_err(|e| HandoffError::new("decode snapshot manifest", e.to_string()))?;
    Ok(())
}

fn write_handoff_env(
    args: &HandoffArgs,
    image_cache: &Path,
    snapshot_ref_hex: &str,
) -> HandoffResult<()> {
    validate_hex_ref(snapshot_ref_hex)?;
    let lines = vec![
        (
            "BRIDGE_HYPERVISOR_ENDPOINT",
            args.bridge_hypervisor_endpoint.clone(),
        ),
        (
            "BRIDGE_PRIVATE_ROOT",
            args.bridge_private_root.display().to_string(),
        ),
        (
            "BRIDGE_WORKLOAD_IMAGE_REF",
            args.bridge_workload_image_ref.clone(),
        ),
        (
            "BRIDGE_CAPTURE_SPEC_REF",
            args.bridge_capture_spec_ref.clone(),
        ),
        (
            "BRIDGE_REFERENCE_WORKLOAD_CHECKOUT",
            args.reference_workload_checkout.display().to_string(),
        ),
        ("BRIDGE_REAL_SNAPSHOT_REF", snapshot_ref_hex.to_owned()),
        (
            "SNAPSTORE_DATA_ROOT",
            args.snapstore_data_root.display().to_string(),
        ),
        (
            "SNAPSTORE_CONFIG_PATH",
            args.snapstore_config.display().to_string(),
        ),
        (
            "SNAPSTORE_GRPC_UDS_PATH",
            args.snapstore_uds.display().to_string(),
        ),
        ("DH_M9_IMAGE_CACHE", image_cache.display().to_string()),
    ];
    let mut body = String::new();
    for (key, value) in &lines {
        validate_no_newline(key, value)?;
        body.push_str(key);
        body.push('=');
        body.push_str(&shell_quote_value(value));
        body.push('\n');
    }
    if body.contains("BRIDGE_CREATE_VM_CONFIG_REF") {
        return Err(HandoffError::new(
            "write handoff env",
            "handoff env must not include BRIDGE_CREATE_VM_CONFIG_REF",
        ));
    }
    write_atomic(&args.handoff_env, body.as_bytes(), 0o600).map_err(|e| {
        HandoffError::new(
            "write handoff env",
            format!("{}: {e}", args.handoff_env.display()),
        )
    })
}

fn public_summary(report: &HandoffReport) -> String {
    format!(
        "M9 artifacts present: yes\nimage cache populated: yes\nsnapstore durable data root populated: yes\nREADY TakeSnapshot succeeded: yes\nRestoreSnapshot verification succeeded: yes\nsource/restored leases destroyed: yes\nprivate handoff written: yes\nsnapstore config written: yes\nworker slots before/after: {}/{}\n",
        report.slots_free_before, report.slots_free_after
    )
}

fn ensure_no_private_literals(
    stage: &'static str,
    text: &str,
    private_literals: &[String],
) -> HandoffResult<()> {
    for literal in private_literals {
        if !literal.is_empty() && text.contains(literal) {
            return Err(HandoffError::new(
                stage,
                "public output contained a private literal",
            ));
        }
    }
    Ok(())
}

pub fn hash_file(path: &Path) -> HandoffResult<[u8; 32]> {
    let mut file = File::open(path)
        .map_err(|e| HandoffError::new("hash file", format!("open {}: {e}", path.display())))?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| HandoffError::new("hash file", format!("read {}: {e}", path.display())))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(*hasher.finalize().as_bytes())
}

pub fn ensure_cache_entry(source: &Path, cache_root: &Path) -> HandoffResult<[u8; 32]> {
    let hash = hash_file(source)?;
    let key = crate::image_resolver::cache_key(&hash);
    let dest = cache_root.join(&key);
    if dest.exists() {
        if hash_file(&dest)? == hash {
            return Ok(hash);
        }
        return Err(HandoffError::new(
            "populate image cache",
            format!(
                "existing image cache entry {} does not match key {key}",
                dest.display()
            ),
        ));
    }

    match fs::hard_link(source, &dest) {
        Ok(()) => Ok(hash),
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {
            if hash_file(&dest)? == hash {
                Ok(hash)
            } else {
                Err(HandoffError::new(
                    "populate image cache",
                    format!(
                        "concurrent image cache entry {} does not match key {key}",
                        dest.display()
                    ),
                ))
            }
        }
        Err(_) => {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default();
            let tmp = cache_root.join(format!("{}.{}.{}.tmp", key, std::process::id(), nonce));
            let _ = fs::remove_file(&tmp);
            fs::copy(source, &tmp).map_err(|e| {
                HandoffError::new(
                    "populate image cache",
                    format!(
                        "copy {} to temporary image cache entry {}: {e}",
                        source.display(),
                        tmp.display()
                    ),
                )
            })?;
            if hash_file(&tmp)? != hash {
                let _ = fs::remove_file(&tmp);
                return Err(HandoffError::new(
                    "populate image cache",
                    format!(
                        "temporary image cache entry {} hash mismatch",
                        tmp.display()
                    ),
                ));
            }
            match fs::hard_link(&tmp, &dest) {
                Ok(()) => {}
                Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                    if hash_file(&dest)? != hash {
                        let _ = fs::remove_file(&tmp);
                        return Err(HandoffError::new(
                            "populate image cache",
                            format!(
                                "concurrent image cache entry {} does not match key {key}",
                                dest.display()
                            ),
                        ));
                    }
                }
                Err(e) => {
                    let _ = fs::remove_file(&tmp);
                    return Err(HandoffError::new(
                        "populate image cache",
                        format!(
                            "publish temporary image cache entry {} to {}: {e}",
                            tmp.display(),
                            dest.display()
                        ),
                    ));
                }
            }
            fs::remove_file(&tmp).map_err(|e| {
                HandoffError::new(
                    "populate image cache",
                    format!("remove temporary image cache entry {}: {e}", tmp.display()),
                )
            })?;
            Ok(hash)
        }
    }
}

pub fn populate_m9_image_cache(artifacts: &M9LinuxArtifacts) -> HandoffResult<M9CachedHashes> {
    Ok(M9CachedHashes {
        bzimage: ensure_cache_entry(&artifacts.bzimage, &artifacts.image_cache)?,
        initramfs: ensure_cache_entry(&artifacts.initramfs, &artifacts.image_cache)?,
        base_image: ensure_cache_entry(&artifacts.base_image, &artifacts.image_cache)?,
        game_image: ensure_cache_entry(&artifacts.game_image, &artifacts.image_cache)?,
    })
}

pub fn m9_linux_machine_config(
    hashes: &M9CachedHashes,
    cpuid_table: Vec<dh_vmm::config::CpuidLeaf>,
) -> dh_vmm::config::MachineConfig {
    let mut config = dh_vmm::config::MachineConfig::new(
        M9_LINUX_MEM_BYTES,
        hashes.game_image,
        dh_vmm::config::BootSpec::BzImage {
            kernel_hash: hashes.bzimage,
            initramfs_hash: hashes.initramfs,
            cmdline: dh_vmm::config::canonicalize_bzimage_cmdline_extras(b"quiet")
                .expect("M9 allows quiet as an append-only cmdline extra"),
        },
    );
    config.cpuid_table = cpuid_table;
    config.device_set = vec![
        dh_devices::detchannel::DEVICE_ID_DETCHANNEL,
        dh_devices::clock::DEVICE_ID_PV_CLOCK,
        dh_devices::pad::DEVICE_ID_PV_PAD,
        dh_devices::entropy::DEVICE_ID_PV_ENTROPY,
        dh_devices::blk::DEVICE_ID_PV_BLK,
        dh_devices::serial::DEVICE_ID_DEBUG_SERIAL,
    ];
    config
}

fn masked_cpuid_table() -> HandoffResult<Option<Vec<dh_vmm::config::CpuidLeaf>>> {
    match dh_vmm::kvm::KvmSystem::open() {
        Ok(sys) if sys.dirty_ring => sys
            .masked_cpuid_table()
            .map(Some)
            .map_err(|e| HandoffError::new("probe KVM", format!("{e:?}"))),
        Ok(_) => Ok(None),
        Err(e) => Err(HandoffError::new("probe KVM", format!("{e:?}"))),
    }
}

fn require_regular_file(name: &str, path: &Path) -> HandoffResult<()> {
    let meta = fs::metadata(path).map_err(|e| {
        HandoffError::new(
            "validate path",
            format!("{name}={} is not readable: {e}", path.display()),
        )
    })?;
    if !meta.is_file() {
        return Err(HandoffError::new(
            "validate path",
            format!("{name}={} must name a regular file", path.display()),
        ));
    }
    Ok(())
}

fn require_directory(name: &str, path: &Path) -> HandoffResult<()> {
    let meta = fs::metadata(path).map_err(|e| {
        HandoffError::new(
            "validate path",
            format!("{name}={} is not readable: {e}", path.display()),
        )
    })?;
    if !meta.is_dir() {
        return Err(HandoffError::new(
            "validate path",
            format!("{name}={} must name an existing directory", path.display()),
        ));
    }
    Ok(())
}

fn set_path_once(slot: &mut Option<PathBuf>, flag: &str, value: String) -> HandoffResult<()> {
    if slot.is_some() {
        return Err(HandoffError::new(
            "parse arguments",
            format!("{flag} supplied more than once"),
        ));
    }
    *slot = Some(PathBuf::from(value));
    Ok(())
}

fn set_string_once(slot: &mut Option<String>, flag: &str, value: String) -> HandoffResult<()> {
    if slot.is_some() {
        return Err(HandoffError::new(
            "parse arguments",
            format!("{flag} supplied more than once"),
        ));
    }
    if value.is_empty() {
        return Err(HandoffError::new(
            "parse arguments",
            format!("{flag} must not be empty"),
        ));
    }
    *slot = Some(value);
    Ok(())
}

fn required_path(slot: Option<PathBuf>, flag: &str) -> HandoffResult<PathBuf> {
    slot.ok_or_else(|| HandoffError::new("parse arguments", format!("missing required {flag}")))
}

fn required_string(slot: Option<String>, flag: &str) -> HandoffResult<String> {
    slot.ok_or_else(|| HandoffError::new("parse arguments", format!("missing required {flag}")))
}

fn validate_no_newline(name: &str, value: &str) -> HandoffResult<()> {
    if value.contains('\n') || value.contains('\r') {
        return Err(HandoffError::new(
            "validate private value",
            format!("{name} must not contain a newline"),
        ));
    }
    Ok(())
}

fn shell_quote_value(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(ch);
        }
    }
    quoted.push('\'');
    quoted
}

fn validate_hex_ref(value: &str) -> HandoffResult<()> {
    if value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(HandoffError::new(
            "validate snapshot ref",
            "snapshot ref must be exactly 64 lowercase hex characters",
        ))
    }
}

fn toml_escape_path(path: &Path) -> HandoffResult<String> {
    let value = path.display().to_string();
    validate_no_newline("TOML path", &value)?;
    Ok(value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn reject_existing_symlink_components(name: &str, path: &Path) -> HandoffResult<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(HandoffError::new(
                "validate private layout",
                format!(
                    "{name} must not contain '..' components: {}",
                    path.display()
                ),
            ));
        }
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(HandoffError::new(
                    "validate private layout",
                    format!(
                        "{name} must not contain symlink component: {}",
                        current.display()
                    ),
                ));
            }
            Ok(_) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => {
                return Err(HandoffError::new(
                    "validate private layout",
                    format!("inspect {name} {}: {e}", current.display()),
                ));
            }
        }
    }
    Ok(())
}

fn validate_created_private_layout(args: &HandoffArgs) -> HandoffResult<()> {
    let private_root = fs::canonicalize(&args.private_root).map_err(|e| {
        HandoffError::new(
            "validate private layout",
            format!(
                "canonicalize private root {}: {e}",
                args.private_root.display()
            ),
        )
    })?;
    for (name, path) in [
        ("snapstore data root", args.snapstore_data_root.as_path()),
        (
            "snapstore UDS parent",
            args.snapstore_uds
                .parent()
                .unwrap_or_else(|| args.snapstore_uds.as_path()),
        ),
        (
            "handoff env parent",
            args.handoff_env
                .parent()
                .unwrap_or_else(|| args.handoff_env.as_path()),
        ),
        (
            "snapstore config parent",
            args.snapstore_config
                .parent()
                .unwrap_or_else(|| args.snapstore_config.as_path()),
        ),
    ] {
        let canonical = fs::canonicalize(path).map_err(|e| {
            HandoffError::new(
                "validate private layout",
                format!("canonicalize {name} {}: {e}", path.display()),
            )
        })?;
        if !canonical.starts_with(&private_root) {
            return Err(HandoffError::new(
                "validate private layout",
                format!("{name} must remain under canonical private root"),
            ));
        }
        if is_under_git_checkout(&canonical) {
            return Err(HandoffError::new(
                "validate private layout",
                format!("{name} must not resolve inside a git checkout"),
            ));
        }
    }
    Ok(())
}

fn validate_public_summary_path(args: &HandoffArgs) -> HandoffResult<()> {
    reject_existing_symlink_components("public summary", &args.public_summary)?;
    for (name, private_path) in [
        ("handoff env", args.handoff_env.as_path()),
        ("snapstore config", args.snapstore_config.as_path()),
    ] {
        if paths_equal_lexically(&args.public_summary, private_path) {
            return Err(HandoffError::new(
                "validate private layout",
                format!("public summary must not overlap {name}"),
            ));
        }
    }
    Ok(())
}

fn paths_equal_lexically(left: &Path, right: &Path) -> bool {
    normalize_path_lexically(left) == normalize_path_lexically(right)
}

fn normalize_path_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        if !matches!(component, std::path::Component::CurDir) {
            normalized.push(component.as_os_str());
        }
    }
    normalized
}

fn is_safe_private_root_for_log(private_root: &Path) -> bool {
    private_root.is_absolute()
        && reject_existing_symlink_components("private root", private_root).is_ok()
        && !is_under_git_checkout(private_root)
        && private_root
            .ancestors()
            .find(|ancestor| ancestor.exists())
            .and_then(|ancestor| fs::canonicalize(ancestor).ok())
            .is_some_and(|ancestor| !is_under_git_checkout(&ancestor))
}

fn is_under_git_checkout(path: &Path) -> bool {
    path.ancestors()
        .any(|ancestor| ancestor.join(".git").exists())
}

fn write_atomic(path: &Path, bytes: &[u8], mode: u32) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!(
        "{}.{}.tmp",
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("file"),
        std::process::id()
    ));
    let _ = fs::remove_file(&tmp);
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    opts.mode(mode);
    let mut file = opts.open(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    #[cfg(unix)]
    fs::set_permissions(&tmp, fs::Permissions::from_mode(mode))?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

pub fn usage() -> &'static str {
    "usage: dh-m9-ready-handoff --private-root PATH --snapstore-data-root PATH --snapstore-uds PATH --reference-workload-checkout PATH --workload-manifest PATH --bridge-hypervisor-endpoint ENDPOINT --bridge-private-root PATH --bridge-workload-image-ref REF --bridge-capture-spec-ref REF --handoff-env PATH --snapstore-config PATH --public-summary PATH [--slot-cores LIST]"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_args(root: &Path) -> Vec<String> {
        vec![
            "--private-root".into(),
            root.join("private").display().to_string(),
            "--snapstore-data-root".into(),
            root.join("private/snapstore-data").display().to_string(),
            "--snapstore-uds".into(),
            root.join("private/runtime/snapstore.sock")
                .display()
                .to_string(),
            "--reference-workload-checkout".into(),
            root.join("reference-workload").display().to_string(),
            "--workload-manifest".into(),
            root.join("reference-workload/dist/workload-image.yaml")
                .display()
                .to_string(),
            "--bridge-hypervisor-endpoint".into(),
            "unix:///run/dh/grpc.sock".into(),
            "--bridge-private-root".into(),
            root.join("bridge").display().to_string(),
            "--bridge-workload-image-ref".into(),
            "workload-ref".into(),
            "--bridge-capture-spec-ref".into(),
            "capture-ref".into(),
            "--handoff-env".into(),
            root.join("private/rom-bridge-o73/handoff/env")
                .display()
                .to_string(),
            "--snapstore-config".into(),
            root.join("private/rom-bridge-o73/snapstore/config.toml")
                .display()
                .to_string(),
            "--public-summary".into(),
            root.join("summary.txt").display().to_string(),
        ]
    }

    #[test]
    fn parse_requires_bridge_private_root() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut args = full_args(dir.path());
        let index = args
            .iter()
            .position(|arg| arg == "--bridge-private-root")
            .unwrap();
        args.drain(index..=index + 1);
        let err = parse_args(args).unwrap_err();
        assert!(err.private_detail().contains("--bridge-private-root"));
    }

    #[test]
    fn artifact_lookup_reports_all_missing_vars() {
        let err = M9LinuxArtifacts::from_lookup(|_| None).unwrap_err();
        for name in M9_LINUX_ARTIFACT_ENV_VARS {
            assert!(err.private_detail().contains(name));
        }
    }

    #[test]
    fn redaction_rejects_private_literals() {
        let err = ensure_no_private_literals(
            "public summary",
            "snapshot abc leaked",
            &["abc".to_string()],
        )
        .unwrap_err();
        assert_eq!(err.stage, "public summary");
    }

    #[test]
    fn validate_hex_ref_requires_lowercase_64_hex() {
        validate_hex_ref(&"a".repeat(64)).unwrap();
        assert!(validate_hex_ref(&"A".repeat(64)).is_err());
        assert!(validate_hex_ref(&"g".repeat(64)).is_err());
        assert!(validate_hex_ref(&"a".repeat(63)).is_err());
    }

    #[test]
    fn reference_workload_rejects_parent_dir_escape() {
        let dir = tempfile::TempDir::new().unwrap();
        let checkout = dir.path().join("reference-workload");
        let outside = dir.path().join("outside");
        fs::create_dir_all(&checkout).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let manifest = outside.join("workload-image.yaml");
        fs::write(&manifest, "name: escaped\n").unwrap();

        let mut args = parse_args(full_args(dir.path())).unwrap();
        args.workload_manifest = checkout.join("../outside/workload-image.yaml");
        let err = validate_reference_workload(&args).unwrap_err();
        assert!(err.private_detail().contains(".."));
    }

    #[cfg(unix)]
    #[test]
    fn reference_workload_rejects_manifest_symlink() {
        let dir = tempfile::TempDir::new().unwrap();
        let checkout = dir.path().join("reference-workload");
        let outside = dir.path().join("outside");
        fs::create_dir_all(&checkout).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let real_manifest = outside.join("workload-image.yaml");
        let link_manifest = checkout.join("workload-image.yaml");
        fs::write(&real_manifest, "name: escaped\n").unwrap();
        std::os::unix::fs::symlink(&real_manifest, &link_manifest).unwrap();

        let mut args = parse_args(full_args(dir.path())).unwrap();
        args.workload_manifest = link_manifest;
        let err = validate_reference_workload(&args).unwrap_err();
        assert!(err.private_detail().contains("symlink"));
    }

    #[test]
    fn private_layout_rejects_git_checkout() {
        let dir = tempfile::TempDir::new().unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::create_dir_all(dir.path().join("reference-workload/dist")).unwrap();
        let args = parse_args(full_args(dir.path())).unwrap();
        let err = validate_private_layout(&args).unwrap_err();
        assert!(err.private_detail().contains("git checkout"));
    }

    #[test]
    fn private_layout_requires_snapstore_paths_under_private_root() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut args = parse_args(full_args(dir.path())).unwrap();
        args.snapstore_uds = dir.path().join("outside/snapstore.sock");
        let err = validate_private_layout(&args).unwrap_err();
        assert!(err.private_detail().contains("snapstore UDS"));

        let mut args = parse_args(full_args(dir.path())).unwrap();
        args.snapstore_data_root = dir.path().join("outside/data");
        let err = validate_private_layout(&args).unwrap_err();
        assert!(err.private_detail().contains("snapstore data root"));
    }

    #[test]
    fn public_summary_must_not_overlap_private_outputs() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut args = parse_args(full_args(dir.path())).unwrap();
        args.public_summary = args.handoff_env.clone();
        let err = validate_private_layout(&args).unwrap_err();
        assert!(err.private_detail().contains("public summary"));

        let mut args = parse_args(full_args(dir.path())).unwrap();
        args.public_summary = args.snapstore_config.clone();
        let err = validate_private_layout(&args).unwrap_err();
        assert!(err.private_detail().contains("public summary"));
    }

    #[test]
    fn private_layout_rejects_parent_dir_components() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut args = parse_args(full_args(dir.path())).unwrap();
        args.snapstore_uds = args.private_root.join("../escaped/snapstore.sock");
        let err = validate_private_layout(&args).unwrap_err();
        assert!(err.private_detail().contains(".."));
    }

    #[cfg(unix)]
    #[test]
    fn private_layout_rejects_symlink_component() {
        let dir = tempfile::TempDir::new().unwrap();
        let real = dir.path().join("real-private");
        let link = dir.path().join("private-link");
        fs::create_dir(&real).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let args = parse_args(full_args(&link)).unwrap();
        let err = validate_private_layout(&args).unwrap_err();
        assert!(err.private_detail().contains("symlink"));
    }

    #[test]
    fn reject_existing_snapstore_uds_refuses_to_unlink() {
        let dir = tempfile::TempDir::new().unwrap();
        let uds = dir.path().join("snapstore.sock");
        fs::write(&uds, b"not a socket").unwrap();
        let err = reject_existing_snapstore_uds(&uds).unwrap_err();
        assert!(err.private_detail().contains("refusing to unlink"));
    }

    #[test]
    fn ensure_cache_entry_rejects_mismatched_existing_entry() {
        let dir = tempfile::TempDir::new().unwrap();
        let source = dir.path().join("source.img");
        let cache = dir.path().join("cache");
        fs::create_dir(&cache).unwrap();
        fs::write(&source, b"source bytes").unwrap();
        let hash = hash_file(&source).unwrap();
        let key = crate::image_resolver::cache_key(&hash);
        fs::write(cache.join(key), b"different bytes").unwrap();

        let err = ensure_cache_entry(&source, &cache).unwrap_err();
        assert!(err.private_detail().contains("does not match"));
    }

    #[cfg(unix)]
    #[test]
    fn private_outputs_are_written_with_private_modes() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::TempDir::new().unwrap();
        let private = dir.path().join("private");
        let args = HandoffArgs {
            private_root: private.clone(),
            snapstore_data_root: private.join("snapstore-data"),
            snapstore_uds: private.join("runtime/snapstore.sock"),
            reference_workload_checkout: PathBuf::from("/reference-workload"),
            workload_manifest: PathBuf::from("/reference-workload/dist/workload-image.yaml"),
            bridge_hypervisor_endpoint: "unix:///run/dh/grpc.sock".into(),
            bridge_private_root: private.join("bridge"),
            bridge_workload_image_ref: "workload".into(),
            bridge_capture_spec_ref: "capture".into(),
            handoff_env: private.join("handoff/env"),
            snapstore_config: private.join("snapstore/config.toml"),
            public_summary: dir.path().join("summary.txt"),
            slot_cores: None,
        };

        prepare_private_dirs(&args).unwrap();
        write_snapstore_config(&args).unwrap();
        write_handoff_env(&args, &private.join("image-cache"), &"1".repeat(64)).unwrap();

        assert_eq!(
            fs::metadata(&args.private_root)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&args.snapstore_config)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(&args.handoff_env)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[test]
    fn snapstore_config_parses_with_ephemeral_loopback_ports() {
        let dir = tempfile::TempDir::new().unwrap();
        let private = dir.path().join("private");
        let args = HandoffArgs {
            private_root: private.clone(),
            snapstore_data_root: private.join("snapstore-data"),
            snapstore_uds: private.join("runtime/snapstore.sock"),
            reference_workload_checkout: PathBuf::from("/reference-workload"),
            workload_manifest: PathBuf::from("/reference-workload/dist/workload-image.yaml"),
            bridge_hypervisor_endpoint: "unix:///run/dh/grpc.sock".into(),
            bridge_private_root: private.join("bridge"),
            bridge_workload_image_ref: "workload".into(),
            bridge_capture_spec_ref: "capture".into(),
            handoff_env: private.join("handoff/env"),
            snapstore_config: private.join("snapstore/config.toml"),
            public_summary: dir.path().join("summary.txt"),
            slot_cores: None,
        };

        write_snapstore_config(&args).unwrap();
        let config = snapstore_server::config::load_config(&args.snapstore_config).unwrap();
        assert_eq!(config.data_root, args.snapstore_data_root);
        assert_eq!(config.resolved_uds_path(), args.snapstore_uds);
        assert_eq!(config.grpc_tcp_addr.port(), 0);
        assert_eq!(config.http_addr.port(), 0);
    }

    #[test]
    fn handoff_env_values_are_shell_safe() {
        let dir = tempfile::TempDir::new().unwrap();
        let private = dir.path().join("private");
        let marker_a = dir.path().join("marker-a");
        let marker_b = dir.path().join("marker-b");
        let workload_ref = format!("workload $(touch {}) with spaces", marker_a.display());
        let capture_ref = format!("capture 'quoted' `touch {}` ; #", marker_b.display());
        let args = HandoffArgs {
            private_root: private.clone(),
            snapstore_data_root: private.join("snapstore-data"),
            snapstore_uds: private.join("runtime/snapstore.sock"),
            reference_workload_checkout: PathBuf::from("/reference-workload"),
            workload_manifest: PathBuf::from("/reference-workload/dist/workload-image.yaml"),
            bridge_hypervisor_endpoint: "unix:///run/dh/grpc.sock".into(),
            bridge_private_root: private.join("bridge root"),
            bridge_workload_image_ref: workload_ref.clone(),
            bridge_capture_spec_ref: capture_ref.clone(),
            handoff_env: private.join("handoff/env"),
            snapstore_config: private.join("snapstore/config.toml"),
            public_summary: dir.path().join("summary.txt"),
            slot_cores: None,
        };

        write_handoff_env(&args, &private.join("image cache"), &"1".repeat(64)).unwrap();
        let script = format!(
            ". {}; printf '%s\\n%s\\n' \"$BRIDGE_WORKLOAD_IMAGE_REF\" \"$BRIDGE_CAPTURE_SPEC_REF\"",
            shell_quote_value(&args.handoff_env.display().to_string())
        );
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(script)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(!marker_a.exists());
        assert!(!marker_b.exists());
        let stdout = String::from_utf8(output.stdout).unwrap();
        let mut lines = stdout.lines();
        assert_eq!(lines.next(), Some(workload_ref.as_str()));
        assert_eq!(lines.next(), Some(capture_ref.as_str()));
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn write_handoff_env_omits_create_vm_config_ref() {
        let dir = tempfile::TempDir::new().unwrap();
        let private = dir.path().join("private");
        fs::create_dir_all(&private).unwrap();
        let args = HandoffArgs {
            private_root: private.clone(),
            snapstore_data_root: private.join("snapstore-data"),
            snapstore_uds: private.join("runtime/snapstore.sock"),
            reference_workload_checkout: PathBuf::from("/reference-workload"),
            workload_manifest: PathBuf::from("/reference-workload/dist/workload-image.yaml"),
            bridge_hypervisor_endpoint: "unix:///run/dh/grpc.sock".into(),
            bridge_private_root: private.join("bridge"),
            bridge_workload_image_ref: "workload".into(),
            bridge_capture_spec_ref: "capture".into(),
            handoff_env: private.join("handoff/env"),
            snapstore_config: private.join("snapstore/config.toml"),
            public_summary: dir.path().join("summary.txt"),
            slot_cores: None,
        };
        write_handoff_env(&args, &private.join("image-cache"), &"1".repeat(64)).unwrap();
        let body = fs::read_to_string(args.handoff_env).unwrap();
        assert!(body.contains("BRIDGE_REAL_SNAPSHOT_REF="));
        assert!(body.contains("DH_M9_IMAGE_CACHE="));
        assert!(!body.contains("BRIDGE_CREATE_VM_CONFIG_REF"));
    }
}
