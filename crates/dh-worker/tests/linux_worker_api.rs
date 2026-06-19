//! M9 Linux worker API acceptance: BzImage boot, deterministic pv-blk as
//! `/dev/vdb`, detchannel Ready EventKind 14, snapshot/restore, and replay.

#![cfg(target_arch = "x86_64")]

mod common;

use std::collections::BTreeSet;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use common::{M9LinuxArtifacts, DH_M9_GAME_IMAGE};
use dh_inputlog::reader::{LogReader, RecordBody};
use dh_proto::v1 as proto;
use dh_proto::v1::hypervisor_worker_server::HypervisorWorker;
use dh_snapshot::dhsnap::{tag, Container};
use dh_vmm::config::{BootSpec, CpuidLeaf, MachineConfig};
use dh_worker::image_resolver::cache_key;
use dh_worker::proto_map::machine_config_to_proto;
use dh_worker::service::{PreflightHealth, WorkerConfig, WorkerService};
use dh_worker::slot_manager::LeasePolicy;
use snapstore_manifest::input_log::InputLogContainer;
use snapstore_manifest::Manifest;
use snapstore_types::SnapshotRef as StoreSnapshotRef;
use tokio_stream::StreamExt;
use tonic::Request;

const ALLOW_SKIP_ENV: &str = "DH_M9_ALLOW_SKIP";
// Keep this aligned with the canonical reference workload size so the root
// snapshot stays under snapstore's per-message transport limit.
const MEM_BYTES: u64 = 128 * 1024 * 1024;
const READY_HARD_CAP: u64 = 10_000_000_000;

type TestResult<T> = Result<T, String>;

#[derive(Clone, Debug)]
struct CachedHashes {
    bzimage: [u8; 32],
    initramfs: [u8; 32],
    base_image: [u8; 32],
    game_image: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileEvidence {
    len: u64,
    modified: Option<std::time::SystemTime>,
    hash: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ReadyPayload {
    unit: u32,
    region_count: u32,
    manifest_generation: u64,
}

fn allow_skip() -> bool {
    std::env::var(ALLOW_SKIP_ENV).as_deref() == Ok("1")
}

fn m9_artifacts() -> TestResult<Option<M9LinuxArtifacts>> {
    match M9LinuxArtifacts::from_env_required("linux_worker_api") {
        Ok(artifacts) => Ok(Some(artifacts)),
        Err(e) if allow_skip() => {
            eprintln!("skipping M9 Linux worker API acceptance because {ALLOW_SKIP_ENV}=1: {e}");
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

fn masked_cpuid_table() -> TestResult<Option<Vec<CpuidLeaf>>> {
    match dh_vmm::kvm::KvmSystem::open() {
        Ok(sys) if sys.dirty_ring => sys
            .masked_cpuid_table()
            .map(Some)
            .map_err(|e| format!("masked CPUID table: {e:?}")),
        Ok(_) if allow_skip() => {
            eprintln!("skipping M9 Linux worker API acceptance: KVM dirty ring unavailable");
            Ok(None)
        }
        Ok(_) => Err("KVM dirty ring unavailable".into()),
        Err(e) if allow_skip() => {
            eprintln!("skipping M9 Linux worker API acceptance: KVM unavailable: {e:?}");
            Ok(None)
        }
        Err(e) => Err(format!("KVM unavailable: {e:?}")),
    }
}

fn hash_file(path: &Path) -> TestResult<[u8; 32]> {
    let mut file = File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("read {}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(*hasher.finalize().as_bytes())
}

fn file_evidence(path: &Path) -> TestResult<FileEvidence> {
    let meta = std::fs::metadata(path).map_err(|e| format!("metadata {}: {e}", path.display()))?;
    Ok(FileEvidence {
        len: meta.len(),
        modified: meta.modified().ok(),
        hash: hash_file(path)?,
    })
}

fn ensure_cache_entry(source: &Path, cache_root: &Path) -> TestResult<[u8; 32]> {
    let hash = hash_file(source)?;
    let dest = cache_root.join(cache_key(&hash));
    if dest.exists() {
        if hash_file(&dest)? == hash {
            return Ok(hash);
        }
        std::fs::remove_file(&dest)
            .map_err(|e| format!("remove stale cache entry {}: {e}", dest.display()))?;
    }

    match std::fs::hard_link(source, &dest) {
        Ok(()) => Ok(hash),
        Err(_) => {
            std::fs::copy(source, &dest).map_err(|e| {
                format!(
                    "copy {} to image cache {}: {e}",
                    source.display(),
                    dest.display()
                )
            })?;
            if hash_file(&dest)? != hash {
                return Err(format!(
                    "image cache entry {} hash mismatch",
                    dest.display()
                ));
            }
            Ok(hash)
        }
    }
}

fn populate_image_cache(artifacts: &M9LinuxArtifacts) -> TestResult<CachedHashes> {
    Ok(CachedHashes {
        bzimage: ensure_cache_entry(&artifacts.bzimage, &artifacts.image_cache)?,
        initramfs: ensure_cache_entry(&artifacts.initramfs, &artifacts.image_cache)?,
        base_image: ensure_cache_entry(&artifacts.base_image, &artifacts.image_cache)?,
        game_image: ensure_cache_entry(&artifacts.game_image, &artifacts.image_cache)?,
    })
}

fn linux_machine_config(hashes: &CachedHashes, cpuid_table: Vec<CpuidLeaf>) -> MachineConfig {
    let mut config = MachineConfig::new(
        MEM_BYTES,
        hashes.game_image,
        BootSpec::BzImage {
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

fn worker_config(image_cache_dir: PathBuf, snapstore: snapstore_client::Transport) -> WorkerConfig {
    WorkerConfig {
        worker_id: "m9-linux-worker-api".into(),
        slot_cores: vec![0],
        lease_policy: LeasePolicy::default(),
        class: proto::DeterminismClass {
            cpu_model: "m9-test-cpu".into(),
            microcode: "m9-test-ucode".into(),
            host_kernel: "m9-test-kernel".into(),
            vmm_version: "m9-test-vmm".into(),
        },
        preflight: PreflightHealth::skipped("m9 Linux worker API acceptance harness"),
        image_cache_dir,
        snapstore: Some(snapstore),
        bisection_checkpoints: dh_worker::service::BisectionCheckpointConfig::default(),
    }
}

fn parse_ready_payload(event: &proto::GuestEvent) -> TestResult<ReadyPayload> {
    if event.stream != detguest_wire::record::EventKind::Ready as u32 {
        return Err(format!("expected Ready stream 14, got {}", event.stream));
    }
    if event.payload.len() != 16 {
        return Err(format!(
            "Ready payload must be 16 bytes, got {}",
            event.payload.len()
        ));
    }
    Ok(ReadyPayload {
        unit: u32::from_le_bytes(event.payload[0..4].try_into().unwrap()),
        region_count: u32::from_le_bytes(event.payload[4..8].try_into().unwrap()),
        manifest_generation: u64::from_le_bytes(event.payload[8..16].try_into().unwrap()),
    })
}

async fn collect_events(
    svc: &WorkerService,
    lease: proto::Lease,
) -> TestResult<Vec<proto::GuestEvent>> {
    let mut stream = svc
        .stream_guest_events(Request::new(proto::StreamGuestEventsRequest {
            lease: Some(lease),
            streams: Vec::new(),
        }))
        .await
        .map_err(|e| format!("StreamGuestEvents: {e}"))?
        .into_inner();
    let mut events = Vec::new();
    while let Some(event) = stream.as_mut().next().await {
        events.push(event.map_err(|e| format!("StreamGuestEvents item: {e}"))?);
    }
    Ok(events)
}

fn event_pos(
    events: &[proto::GuestEvent],
    kind: detguest_wire::record::EventKind,
) -> Option<usize> {
    events.iter().position(|event| event.stream == kind as u32)
}

fn payload_preview(payload: &[u8]) -> String {
    payload
        .iter()
        .take(48)
        .map(|b| {
            if b.is_ascii_graphic() || *b == b' ' {
                char::from(*b)
            } else {
                '.'
            }
        })
        .collect()
}

fn event_summary(events: &[proto::GuestEvent]) -> String {
    if events.is_empty() {
        return "<none>".into();
    }
    let mut parts = events
        .iter()
        .take(16)
        .map(|event| {
            format!(
                "stream={} icount={} len={} payload={:?}",
                event.stream,
                event.icount,
                event.payload.len(),
                payload_preview(&event.payload)
            )
        })
        .collect::<Vec<_>>();
    if events.len() > parts.len() {
        parts.push(format!("... {} more", events.len() - parts.len()));
    }
    parts.join("; ")
}

fn assert_ready_ordering(
    events: &[proto::GuestEvent],
    ready_icount: u64,
    region_count: u32,
) -> TestResult<()> {
    let hello = event_pos(events, detguest_wire::record::EventKind::Hello)
        .ok_or_else(|| "guest events did not contain Hello before Ready".to_string())?;
    let ready = event_pos(events, detguest_wire::record::EventKind::Ready)
        .ok_or_else(|| "guest events did not contain Ready".to_string())?;
    if hello >= ready {
        return Err(format!(
            "guest event order invalid: hello={hello}, ready={ready}"
        ));
    }
    if region_count > 0 {
        let region = event_pos(events, detguest_wire::record::EventKind::RegionRegister)
            .ok_or_else(|| {
                "guest events did not contain RegionRegister before Ready".to_string()
            })?;
        if region >= ready {
            return Err(format!(
                "guest event order invalid: region={region}, ready={ready}"
            ));
        }
    }
    if events[ready].icount != ready_icount {
        return Err(format!(
            "streamed Ready icount {} did not match RunResponse icount {ready_icount}",
            events[ready].icount
        ));
    }
    if region_count > 0
        && !events.iter().any(|event| {
            event
                .payload
                .windows(b"/dev/vdb".len())
                .any(|w| w == b"/dev/vdb")
        })
    {
        return Err("fixture did not expose LoadGame/dev_path evidence containing /dev/vdb".into());
    }
    Ok(())
}

fn snapshot_tags(
    store: &snapstore_client::blocking::SnapstoreClient,
    snapshot: &proto::SnapshotRef,
) -> TestResult<BTreeSet<[u8; 4]>> {
    let dhsnap = snapshot_dhsnap(store, snapshot)?;
    Ok(dhsnap.sections().map(|section| section.tag).collect())
}

fn snapshot_dhsnap(
    store: &snapstore_client::blocking::SnapstoreClient,
    snapshot: &proto::SnapshotRef,
) -> TestResult<Container<'static>> {
    let hash: [u8; 32] = snapshot
        .hash
        .as_slice()
        .try_into()
        .map_err(|_| "snapshot ref must be 32 bytes".to_string())?;
    let container = store
        .get_snapshot(snapstore_types::SnapshotRef::from_bytes(hash))
        .map_err(|e| format!("get_snapshot: {e}"))?;
    let manifest = Manifest::decode(&container).map_err(|e| format!("manifest decode: {e}"))?;
    let bytes: &'static [u8] = Box::leak(manifest.device_blob.bytes.into_boxed_slice());
    Container::parse(bytes).map_err(|e| format!("DHSNAP parse: {e:?}"))
}

fn assert_ready_snapshot_channel_reattaches(
    store: &snapstore_client::blocking::SnapstoreClient,
    snapshot: &proto::SnapshotRef,
) -> TestResult<()> {
    let dhsnap = snapshot_dhsnap(store, snapshot)?;
    let evtc = dhsnap
        .get(tag::EVTC)
        .ok_or_else(|| "Ready snapshot missing EVTC section".to_string())?;
    const EVTC_V1_LEN: usize = 39;
    if evtc.sec_version != 1 || evtc.contents.len() != EVTC_V1_LEN {
        return Err(format!(
            "EVTC shape mismatch: v{} {} bytes",
            evtc.sec_version,
            evtc.contents.len()
        ));
    }
    if evtc.contents[22] != 1 {
        return Err(format!(
            "EVTC does not record an attached channel: flag={} status={}",
            evtc.contents[22],
            u32::from_le_bytes(evtc.contents[8..12].try_into().unwrap())
        ));
    }
    let gpa = u64::from_le_bytes(evtc.contents[23..31].try_into().unwrap());
    let channel_bytes = snapshot_channel_bytes(store, snapshot, gpa)?;
    let mut mem = detguest_host::MockGuestMem::new();
    mem.add_segment(gpa, channel_bytes);
    detguest_host::Channel::attach(mem, gpa)
        .map(|_| ())
        .map_err(|e| format!("Ready snapshot channel attach at {gpa:#x}: {e:?}"))
}

fn snapshot_channel_bytes(
    store: &snapstore_client::blocking::SnapstoreClient,
    snapshot: &proto::SnapshotRef,
    gpa: u64,
) -> TestResult<Vec<u8>> {
    const PAGE_SIZE: u64 = 4096;
    let hash: [u8; 32] = snapshot
        .hash
        .as_slice()
        .try_into()
        .map_err(|_| "snapshot ref must be 32 bytes".to_string())?;
    let channel_len = u64::from(detguest_wire::header::CHANNEL_SIZE_PAGES) * PAGE_SIZE;
    let first_page = gpa / PAGE_SIZE;
    let page_count = channel_len / PAGE_SIZE;
    let mut channel = vec![0u8; channel_len as usize];
    let mut covered = vec![false; page_count as usize];
    let resolved = store
        .resolve_pages(StoreSnapshotRef::from_bytes(hash), None, false)
        .map_err(|e| format!("resolve_pages for detchannel: {e}"))?;
    for (page_idx, _hash, payload) in resolved {
        if !(first_page..first_page + page_count).contains(&page_idx) {
            continue;
        }
        let payload = payload
            .ok_or_else(|| format!("detchannel page {page_idx} resolved without payload bytes"))?;
        if payload.len() != PAGE_SIZE as usize {
            return Err(format!(
                "detchannel page {page_idx} payload len {}",
                payload.len()
            ));
        }
        let offset = ((page_idx - first_page) * PAGE_SIZE) as usize;
        channel[offset..offset + PAGE_SIZE as usize].copy_from_slice(&payload);
        covered[(page_idx - first_page) as usize] = true;
    }
    if let Some(missing) = covered.iter().position(|covered| !covered) {
        return Err(format!(
            "Ready snapshot missing detchannel page {} at gpa {:#x}",
            first_page + missing as u64,
            gpa + missing as u64 * PAGE_SIZE
        ));
    }
    Ok(channel)
}

fn assert_device_sections(tags: &BTreeSet<[u8; 4]>) -> TestResult<()> {
    for tag in [
        tag::EVTC,
        tag::BLKO,
        tag::CLKD,
        tag::PADD,
        tag::ENTR,
        tag::SERL,
    ] {
        if !tags.contains(&tag) {
            return Err(format!(
                "Ready snapshot missing DHSNAP section {}",
                std::str::from_utf8(&tag).unwrap_or("????")
            ));
        }
    }
    Ok(())
}

fn input_log_payload(
    store: &snapstore_client::blocking::SnapstoreClient,
    input_log_id: &[u8],
) -> TestResult<Vec<u8>> {
    let id: [u8; 32] = input_log_id
        .try_into()
        .map_err(|_| "input log id must be 32 bytes".to_string())?;
    let container = store
        .get_input_log(snapstore_types::LogId::from_bytes(id))
        .map_err(|e| format!("get_input_log: {e}"))?;
    let decoded = InputLogContainer::decode(&container)
        .map_err(|e| format!("input log container decode: {e}"))?;
    Ok(decoded.payload().to_vec())
}

fn assert_no_external_input_before_ready(log: &[u8], ready_icount: u64) -> TestResult<()> {
    let reader = LogReader::parse(log).map_err(|e| format!("DHILOG parse: {e:?}"))?;
    for rec in reader.records().filter(|rec| rec.icount() <= ready_icount) {
        match rec.body() {
            RecordBody::PadSet { .. } => {
                return Err(format!(
                    "PAD_SET landed before Ready at icount {}",
                    rec.icount()
                ));
            }
            RecordBody::NetRx { .. } => {
                return Err(format!(
                    "NET_RX landed before Ready at icount {}",
                    rec.icount()
                ));
            }
            RecordBody::DevEvent {
                device_id,
                event_type,
                data,
            } => {
                if device_id != dh_inputlog::dhilog::DEVICE_ID_DETCHANNEL {
                    return Err(format!(
                        "DeviceEvent for device {device_id:#06x} landed before Ready at icount {}",
                        rec.icount()
                    ));
                }
                if event_type == dh_inputlog::dhilog::EVENT_RING_PUSH
                    && data.first().is_some_and(|ring| *ring == 0 || *ring == 1)
                {
                    return Err(format!(
                        "host ring-C/ring-I push landed before Ready at icount {}",
                        rec.icount()
                    ));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

async fn verify_replay_done(
    svc: &WorkerService,
    base: proto::SnapshotRef,
    input_log_id: Vec<u8>,
) -> TestResult<proto::VerifyDone> {
    let mut stream = svc
        .verify_replay(Request::new(proto::VerifyReplayRequest {
            base: Some(base),
            log: Some(proto::verify_replay_request::Log::InputLogId(input_log_id)),
            bisect_on_divergence: Some(false),
        }))
        .await
        .map_err(|e| format!("VerifyReplay: {e}"))?
        .into_inner();
    while let Some(progress) = stream.as_mut().next().await {
        let progress = progress.map_err(|e| format!("VerifyReplay progress: {e}"))?;
        match progress.msg {
            Some(proto::verify_replay_progress::Msg::Done(done)) => return Ok(done),
            Some(proto::verify_replay_progress::Msg::Divergence(divergence)) => {
                return Err(format!("VerifyReplay divergence: {divergence:?}"));
            }
            Some(proto::verify_replay_progress::Msg::EpochOk(_)) => {}
            None => return Err("VerifyReplay emitted empty progress message".into()),
        }
    }
    Err("VerifyReplay ended without Done".into())
}

#[test]
#[ignore = "M9 Linux artifact gate: requires DH_M9_* artifacts and KVM"]
fn pvblk_dev_vdb() {
    let Some(artifacts) = m9_artifacts().expect("M9 artifacts") else {
        return;
    };
    let Some(cpuid_table) = masked_cpuid_table().expect("KVM/masked CPUID table") else {
        return;
    };

    let game_before = file_evidence(&artifacts.game_image)
        .unwrap_or_else(|e| panic!("{DH_M9_GAME_IMAGE} evidence before run: {e}"));
    let hashes = populate_image_cache(&artifacts).expect("populate M9 image cache");
    assert_eq!(
        hashes.game_image, game_before.hash,
        "MachineConfig.base_image_hash must be DH_M9_GAME_IMAGE"
    );
    assert_eq!(
        hashes.base_image,
        hash_file(&artifacts.base_image).expect("DH_M9_BASE_IMAGE hash"),
        "DH_M9_BASE_IMAGE is fixture context; current worker pv-blk backing is DH_M9_GAME_IMAGE"
    );

    let config = linux_machine_config(&hashes, cpuid_table);
    assert_eq!(config.base_image_hash, hashes.game_image);
    assert_eq!(
        config.device_set,
        vec![
            dh_devices::detchannel::DEVICE_ID_DETCHANNEL,
            dh_devices::clock::DEVICE_ID_PV_CLOCK,
            dh_devices::pad::DEVICE_ID_PV_PAD,
            dh_devices::entropy::DEVICE_ID_PV_ENTROPY,
            dh_devices::blk::DEVICE_ID_PV_BLK,
            dh_devices::serial::DEVICE_ID_DEBUG_SERIAL,
        ]
    );
    let config_hash = config.config_hash().expect("MachineConfig hash");
    assert_eq!(
        config_hash,
        linux_machine_config(&hashes, config.cpuid_table.clone())
            .config_hash()
            .expect("repeat MachineConfig hash")
    );

    let store_dir = tempfile::TempDir::new().expect("snapstore data root");
    let store_sock = "snapstore.sock";
    let (_store_rt, _store_handle, store) =
        common::spawn_store_at(store_dir.path().to_path_buf(), store_sock);
    let snapstore = snapstore_client::Transport::Uds(store_dir.path().join(store_sock));
    let svc = WorkerService::new(worker_config(artifacts.image_cache.clone(), snapstore))
        .expect("worker service");
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("test runtime");

    rt.block_on(async {
        let created = svc
            .create_vm(Request::new(proto::CreateVmRequest {
                config: Some(machine_config_to_proto(&config)),
                entropy_seed: vec![0x9A; 32],
            }))
            .await
            .expect("CreateVm BzImage")
            .into_inner();
        let lease = created.lease.expect("CreateVm lease");
        assert_eq!(created.icount, 0);

        let initial = svc
            .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                lease: Some(lease.clone()),
                seal_input_log: Some(true),
                capture: None,
            }))
            .await
            .expect("initial root snapshot")
            .into_inner();
        let initial_snapshot = initial.snapshot.clone().expect("initial snapshot ref");
        assert_eq!(initial_snapshot.hash.len(), 32);
        assert_eq!(initial.icount, 0);
        assert_eq!(initial.machine_config_hash, config_hash.to_vec());

        let run = svc
            .run(Request::new(proto::RunRequest {
                lease: Some(lease.clone()),
                until: Some(proto::run_request::Until::NextSdkEvent(
                    proto::NextSdkEvent {
                        stream: Some(detguest_wire::record::EventKind::Ready as u32),
                    },
                )),
                hard_icount_cap: READY_HARD_CAP,
                capture: None,
            }))
            .await
            .expect("Run until Ready")
            .into_inner();
        let events = collect_events(&svc, lease.clone())
            .await
            .expect("stream guest events");
        assert_eq!(
            run.reason,
            i32::from(proto::StopReason::NextSdkEvent),
            "Ready must be observed through detchannel next_sdk_event; run_icount={} events={}",
            run.icount,
            event_summary(&events)
        );
        let ready_event = run.sdk_event.as_ref().expect("RunResponse.sdk_event");
        let ready = parse_ready_payload(ready_event).expect("Ready payload");
        assert_eq!(
            ready.manifest_generation % 2,
            0,
            "Ready manifest generation must be even/stable"
        );
        assert!(
            ready.region_count > 0,
            "M9 fixture must register expected regions before Ready"
        );

        assert_ready_ordering(&events, run.icount, ready.region_count)
            .expect("Ready ordering and fixture evidence");

        let ready_snapshot = svc
            .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                lease: Some(lease.clone()),
                seal_input_log: Some(true),
                capture: None,
            }))
            .await
            .expect("Ready snapshot")
            .into_inner();
        let ready_snapshot_ref = ready_snapshot.snapshot.clone().expect("Ready snapshot ref");
        let ready_state_hash = ready_snapshot
            .state_hash
            .clone()
            .expect("Ready snapshot state hash");
        assert_eq!(ready_snapshot.icount, run.icount);
        assert_eq!(ready_snapshot.machine_config_hash, config_hash.to_vec());

        let tags = tokio::task::block_in_place(|| snapshot_tags(&store, &ready_snapshot_ref))
            .expect("Ready snapshot DHSNAP sections");
        assert_device_sections(&tags).expect("M9 deterministic device sections");
        tokio::task::block_in_place(|| {
            assert_ready_snapshot_channel_reattaches(&store, &ready_snapshot_ref)
        })
        .expect("Ready snapshot detchannel page reattaches");

        let log =
            tokio::task::block_in_place(|| input_log_payload(&store, &ready_snapshot.input_log_id))
                .expect("Ready input log payload");
        assert_no_external_input_before_ready(&log, run.icount)
            .expect("no external host input before Ready");

        svc.destroy_vm(Request::new(proto::DestroyVmRequest { lease: Some(lease) }))
            .await
            .expect("destroy Ready slot");

        let restored = svc
            .restore_snapshot(Request::new(proto::RestoreSnapshotRequest {
                snapshot: Some(ready_snapshot_ref.clone()),
                entropy_seed: Vec::new(),
            }))
            .await
            .expect("RestoreSnapshot Ready")
            .into_inner();
        assert_eq!(
            restored.state_hash.expect("restore state hash").hash,
            ready_state_hash.hash,
            "restore must preserve Ready state hash"
        );
        assert_eq!(
            restored.config.expect("restored config").base_image_hash,
            hashes.game_image.to_vec()
        );
        svc.destroy_vm(Request::new(proto::DestroyVmRequest {
            lease: restored.lease,
        }))
        .await
        .expect("destroy restored slot");

        let done = verify_replay_done(&svc, initial_snapshot, ready_snapshot.input_log_id.clone())
            .await
            .expect("VerifyReplay Done");
        assert_eq!(done.total_icount, run.icount);
        assert_eq!(
            done.end_state_hash.expect("VerifyReplay end hash").hash,
            ready_state_hash.hash,
            "live run and replay must end with the same lAPIC+bus-device state hash"
        );
    });

    let game_after = file_evidence(&artifacts.game_image)
        .unwrap_or_else(|e| panic!("{DH_M9_GAME_IMAGE} evidence after run: {e}"));
    assert_eq!(
        game_after, game_before,
        "DH_M9_GAME_IMAGE source bytes and mtime must not change"
    );
}
