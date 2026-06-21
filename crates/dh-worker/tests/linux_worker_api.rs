//! M9 Linux worker API acceptance: BzImage boot, deterministic pv-blk as
//! `/dev/vdb`, detchannel Ready EventKind 14, snapshot/restore, and replay.

#![cfg(target_arch = "x86_64")]

mod common;

use std::collections::{BTreeMap, BTreeSet};
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
const FORK_CHILD_BUDGET: u64 = 1_000_000;

type TestResult<T> = Result<T, String>;

const REQUIRED_M9_EXPECTED_REGIONS: &[(&str, i64)] =
    &[("wram", 1), ("framebuffer", 1), ("meta", 1)];

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

fn align4(n: usize) -> Option<usize> {
    n.checked_add(3).map(|v| v & !3)
}

fn parse_newc_hex(field: &[u8]) -> TestResult<usize> {
    let text = std::str::from_utf8(field).map_err(|e| format!("newc header utf8: {e}"))?;
    usize::from_str_radix(text, 16).map_err(|e| format!("newc header hex {text:?}: {e}"))
}

fn normalize_cpio_path(path: &str) -> &str {
    path.trim_start_matches("./").trim_start_matches('/')
}

fn initramfs_entry<'a>(archive: &'a [u8], needle: &str) -> TestResult<&'a [u8]> {
    const HEADER_LEN: usize = 110;
    let needle = normalize_cpio_path(needle);
    let mut offset = 0usize;
    while offset
        .checked_add(HEADER_LEN)
        .is_some_and(|end| end <= archive.len())
    {
        let header = &archive[offset..offset + HEADER_LEN];
        if &header[..6] != b"070701" {
            return Err(format!(
                "unsupported initramfs cpio magic at offset {offset}"
            ));
        }
        let file_size = parse_newc_hex(&header[54..62])?;
        let name_size = parse_newc_hex(&header[94..102])?;
        offset += HEADER_LEN;

        let name_end = offset
            .checked_add(name_size)
            .ok_or_else(|| "newc filename offset overflow".to_string())?;
        if name_size == 0 || name_end > archive.len() {
            return Err("truncated initramfs cpio filename".into());
        }
        let raw_name = &archive[offset..name_end - 1];
        let name = std::str::from_utf8(raw_name).map_err(|e| format!("newc filename utf8: {e}"))?;
        let data_start = align4(name_end).ok_or_else(|| "newc data offset overflow".to_string())?;
        let data_end = data_start
            .checked_add(file_size)
            .ok_or_else(|| "newc file size overflow".to_string())?;
        if data_end > archive.len() {
            return Err(format!("truncated initramfs cpio entry {name:?}"));
        }
        if name == "TRAILER!!!" {
            break;
        }
        if normalize_cpio_path(name) == needle {
            return Ok(&archive[data_start..data_end]);
        }
        offset = align4(data_end).ok_or_else(|| "newc next offset overflow".to_string())?;
    }
    Err(format!("initramfs missing {needle}"))
}

fn toml_table<'a>(
    value: &'a toml::Value,
    name: &str,
) -> TestResult<&'a toml::map::Map<String, toml::Value>> {
    value
        .as_table()
        .ok_or_else(|| format!("{name} must be a TOML table"))
}

fn toml_array<'a>(table: &'a toml::Table, key: &str) -> TestResult<&'a Vec<toml::Value>> {
    table
        .get(key)
        .and_then(|value| value.as_array())
        .ok_or_else(|| format!("boot.toml missing array [[{key}]]"))
}

fn assert_initramfs_boot_contract(initramfs: &Path) -> TestResult<()> {
    let archive = std::fs::read(initramfs)
        .map_err(|e| format!("read initramfs {}: {e}", initramfs.display()))?;
    let boot_toml = initramfs_entry(&archive, "etc/detguest/boot.toml")?;
    let boot_toml = std::str::from_utf8(boot_toml)
        .map_err(|e| format!("boot.toml in {} is not UTF-8: {e}", initramfs.display()))?;
    let manifest: toml::Value = boot_toml
        .parse()
        .map_err(|e| format!("parse initramfs boot.toml: {e}"))?;
    let root = toml_table(&manifest, "boot.toml root")?;
    if root
        .get("boot_toml_version")
        .and_then(|value| value.as_integer())
        != Some(1)
    {
        return Err("boot.toml must set boot_toml_version = 1".into());
    }
    let autostart_unit = root
        .get("autostart")
        .and_then(|value| value.as_table())
        .and_then(|table| table.get("unit"))
        .and_then(|value| value.as_integer())
        .ok_or_else(|| "boot.toml must autostart the reference workload unit".to_string())?;
    let units = toml_array(root, "unit")?;
    let unit = units
        .iter()
        .filter_map(|value| value.as_table())
        .find(|table| table.get("id").and_then(|value| value.as_integer()) == Some(autostart_unit))
        .ok_or_else(|| format!("boot.toml autostart unit {autostart_unit} has no [[unit]]"))?;
    let control = unit
        .get("control")
        .and_then(|value| value.as_table())
        .ok_or_else(|| "autostart unit must declare [unit.control]".to_string())?;
    if control.get("protocol").and_then(|value| value.as_str()) != Some("refwork-ctl") {
        return Err("autostart unit.control must use protocol = \"refwork-ctl\"".into());
    }
    if control
        .get("proto_version")
        .and_then(|value| value.as_integer())
        != Some(1)
    {
        return Err("autostart unit.control must use proto_version = 1".into());
    }
    if control.get("game_dev").and_then(|value| value.as_str()) != Some("/dev/vdb") {
        return Err("autostart unit.control must set game_dev = \"/dev/vdb\"".into());
    }
    let expected_regions = toml_array(root, "expected_region")?;
    if expected_regions.is_empty() {
        return Err("boot.toml must list expected regions for the Ready gate".into());
    }
    for (idx, region) in expected_regions.iter().enumerate() {
        let region = region
            .as_table()
            .ok_or_else(|| format!("expected_region[{idx}] must be a table"))?;
        let name = region
            .get("name")
            .and_then(|value| value.as_str())
            .ok_or_else(|| format!("expected_region[{idx}] must name a region"))?;
        if name.is_empty() {
            return Err(format!("expected_region[{idx}] has an empty name"));
        }
        if region
            .get("layout_version")
            .and_then(|value| value.as_integer())
            .filter(|version| *version > 0)
            .is_none()
        {
            return Err(format!(
                "expected_region[{idx}] ({name}) must pin a positive layout_version"
            ));
        }
    }
    for (name, layout_version) in REQUIRED_M9_EXPECTED_REGIONS {
        let found = expected_regions.iter().any(|region| {
            region.as_table().and_then(|table| {
                Some((
                    table.get("name")?.as_str()?,
                    table.get("layout_version")?.as_integer()?,
                ))
            }) == Some((*name, *layout_version))
        });
        if !found {
            return Err(format!(
                "boot.toml must list expected_region {name:?} with layout_version {layout_version}"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
fn append_test_newc_entry(out: &mut Vec<u8>, name: &str, data: &[u8]) {
    let namesize = name.len() + 1;
    let header = format!(
        "070701{ino:08x}{mode:08x}{uid:08x}{gid:08x}{nlink:08x}{mtime:08x}{filesize:08x}{devmajor:08x}{devminor:08x}{rdevmajor:08x}{rdevminor:08x}{namesize:08x}{check:08x}",
        ino = 1u32,
        mode = 0o100644u32,
        uid = 0u32,
        gid = 0u32,
        nlink = 1u32,
        mtime = 0u32,
        filesize = data.len(),
        devmajor = 0u32,
        devminor = 0u32,
        rdevmajor = 0u32,
        rdevminor = 0u32,
        check = 0u32,
    );
    assert_eq!(header.len(), 110);
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(name.as_bytes());
    out.push(0);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
    out.extend_from_slice(data);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

#[cfg(test)]
fn test_newc_with_boot_toml(boot_toml: &str) -> Vec<u8> {
    let mut archive = Vec::new();
    append_test_newc_entry(
        &mut archive,
        "./etc/detguest/boot.toml",
        boot_toml.as_bytes(),
    );
    append_test_newc_entry(&mut archive, "TRAILER!!!", &[]);
    archive
}

#[test]
fn initramfs_contract_accepts_refwork_boot_toml() -> TestResult<()> {
    let boot_toml = r#"
boot_toml_version = 1

[autostart]
unit = 0

[[unit]]
id = 0
exec = "/usr/bin/refwork-harness"

[unit.control]
protocol = "refwork-ctl"
proto_version = 1
game_dev = "/dev/vdb"

[[expected_region]]
name = "wram"
layout_version = 1

[[expected_region]]
name = "framebuffer"
layout_version = 1

[[expected_region]]
name = "meta"
layout_version = 1
"#;
    let tmp = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    std::fs::write(tmp.path(), test_newc_with_boot_toml(boot_toml)).map_err(|e| e.to_string())?;

    assert_initramfs_boot_contract(tmp.path())
}

#[test]
fn initramfs_contract_rejects_smoke_boot_toml_without_control() -> TestResult<()> {
    let boot_toml = r#"
boot_toml_version = 1

[autostart]
unit = 0

[[unit]]
id = 0
exec = "/opt/autostart-trivial"
"#;
    let tmp = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    std::fs::write(tmp.path(), test_newc_with_boot_toml(boot_toml)).map_err(|e| e.to_string())?;

    let err = assert_initramfs_boot_contract(tmp.path()).expect_err("smoke manifest rejected");
    assert!(
        err.contains("[unit.control]"),
        "unexpected contract error: {err}"
    );
    Ok(())
}

#[test]
fn initramfs_contract_rejects_refwork_manifest_missing_required_region() -> TestResult<()> {
    let boot_toml = r#"
boot_toml_version = 1

[autostart]
unit = 0

[[unit]]
id = 0
exec = "/usr/bin/refwork-harness"

[unit.control]
protocol = "refwork-ctl"
proto_version = 1
game_dev = "/dev/vdb"

[[expected_region]]
name = "wram"
layout_version = 1

[[expected_region]]
name = "framebuffer"
layout_version = 1
"#;
    let tmp = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    std::fs::write(tmp.path(), test_newc_with_boot_toml(boot_toml)).map_err(|e| e.to_string())?;

    let err = assert_initramfs_boot_contract(tmp.path()).expect_err("missing meta rejected");
    assert!(err.contains("\"meta\""), "unexpected contract error: {err}");
    Ok(())
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
        slot_cores: vec![0, 1],
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
        bisection_checkpoints: dh_worker::service::BisectionCheckpointConfig::every_epoch(),
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

fn payload_u32(payload: &[u8], offset: usize, what: &str) -> TestResult<u32> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| format!("{what} offset overflow"))?;
    let bytes = payload
        .get(offset..end)
        .ok_or_else(|| format!("{what} payload too short: {} bytes", payload.len()))?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}

fn name_intern_payload(payload: &[u8]) -> TestResult<(u32, String)> {
    if payload.len() < 8 {
        return Err(format!(
            "NameIntern payload too short: {} bytes",
            payload.len()
        ));
    }
    let name_id = payload_u32(payload, 0, "NameIntern.name_id")?;
    let name_len = u16::from_le_bytes(payload[4..6].try_into().unwrap()) as usize;
    let name_end = 8usize
        .checked_add(name_len)
        .ok_or_else(|| "NameIntern.name_len overflow".to_string())?;
    let name_bytes = payload.get(8..name_end).ok_or_else(|| {
        format!(
            "NameIntern.name_len {name_len} exceeds payload {}",
            payload.len()
        )
    })?;
    if payload[name_end..].iter().any(|&b| b != 0) {
        return Err("NameIntern payload padding must be zero".into());
    }
    let name = std::str::from_utf8(name_bytes)
        .map_err(|e| format!("NameIntern.name is not UTF-8: {e}"))?
        .to_owned();
    Ok((name_id, name))
}

fn region_register_payload(payload: &[u8]) -> TestResult<(u32, u32, u32, u32)> {
    if payload.len() != 16 {
        return Err(format!(
            "RegionRegister payload must be 16 bytes, got {}",
            payload.len()
        ));
    }
    Ok((
        payload_u32(payload, 0, "RegionRegister.region_id")?,
        payload_u32(payload, 4, "RegionRegister.name_id")?,
        payload_u32(payload, 8, "RegionRegister.layout_version")?,
        payload_u32(payload, 12, "RegionRegister.manifest_generation")?,
    ))
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
    if region_count < REQUIRED_M9_EXPECTED_REGIONS.len() as u32 {
        return Err(format!(
            "Ready region_count {region_count} is smaller than required M9 regions {}",
            REQUIRED_M9_EXPECTED_REGIONS.len()
        ));
    }
    let mut names = BTreeMap::<u32, String>::new();
    let mut registered = BTreeMap::<String, u32>::new();
    for (idx, event) in events.iter().enumerate().take(ready) {
        if event.stream == detguest_wire::record::EventKind::NameIntern as u32 {
            let (name_id, name) = name_intern_payload(&event.payload)?;
            names.insert(name_id, name);
        } else if event.stream == detguest_wire::record::EventKind::RegionRegister as u32 {
            let (_region_id, name_id, layout_version, _generation) =
                region_register_payload(&event.payload)?;
            let name = names.get(&name_id).ok_or_else(|| {
                format!(
                    "RegionRegister before Ready at event {idx} referenced unknown name_id {name_id}"
                )
            })?;
            registered.insert(name.clone(), layout_version);
        }
    }
    for (name, layout_version) in REQUIRED_M9_EXPECTED_REGIONS {
        match registered.get(*name) {
            Some(got) if i64::from(*got) == *layout_version => {}
            Some(got) => {
                return Err(format!(
                    "RegionRegister for {name:?} had layout_version {got}, expected {layout_version}"
                ));
            }
            None => {
                return Err(format!(
                    "guest events did not contain RegionRegister for required region {name:?} before Ready"
                ));
            }
        }
    }
    if events[ready].icount != ready_icount {
        return Err(format!(
            "streamed Ready icount {} did not match RunResponse icount {ready_icount}",
            events[ready].icount
        ));
    }
    Ok(())
}

#[cfg(test)]
fn test_event(
    kind: detguest_wire::record::EventKind,
    icount: u64,
    payload: Vec<u8>,
) -> proto::GuestEvent {
    proto::GuestEvent {
        stream: kind as u32,
        icount,
        vns: icount,
        payload,
    }
}

#[cfg(test)]
fn name_intern_event(icount: u64, name_id: u32, name: &str) -> proto::GuestEvent {
    let payload = canonical_payload(&detguest_wire::events::EventPayload::NameIntern {
        name_id,
        name: name.as_bytes(),
    });
    test_event(
        detguest_wire::record::EventKind::NameIntern,
        icount,
        payload,
    )
}

#[cfg(test)]
fn region_register_event(
    icount: u64,
    region_id: u32,
    name_id: u32,
    layout_version: u32,
) -> proto::GuestEvent {
    let payload = canonical_payload(&detguest_wire::events::EventPayload::RegionRegister(
        detguest_wire::events::RegionEvent {
            region_id,
            name_id,
            layout_version,
            manifest_generation: 2,
        },
    ));
    test_event(
        detguest_wire::record::EventKind::RegionRegister,
        icount,
        payload,
    )
}

#[cfg(test)]
fn ready_event(icount: u64, region_count: u32) -> proto::GuestEvent {
    let payload = canonical_payload(&detguest_wire::events::EventPayload::Ready {
        unit: 0,
        region_count,
        manifest_generation: 2,
    });
    test_event(detguest_wire::record::EventKind::Ready, icount, payload)
}

#[cfg(test)]
fn canonical_payload(ev: &detguest_wire::events::EventPayload<'_>) -> Vec<u8> {
    let mut buf = vec![0u8; detguest_wire::record::MAX_RECORD_LEN];
    let n = detguest_wire::events::encode_event(&mut buf, 0, 0, 0, ev).unwrap();
    buf[detguest_wire::record::RECORD_HEADER_LEN..n].to_vec()
}

#[test]
fn ready_ordering_requires_named_expected_regions_before_ready() -> TestResult<()> {
    let events = vec![
        test_event(detguest_wire::record::EventKind::Hello, 1, vec![1; 12]),
        name_intern_event(2, 10, "wram"),
        region_register_event(3, 0, 10, 1),
        name_intern_event(4, 11, "framebuffer"),
        region_register_event(5, 1, 11, 1),
        name_intern_event(6, 12, "meta"),
        region_register_event(7, 2, 12, 1),
        ready_event(8, 3),
    ];

    assert_ready_ordering(&events, 8, 3)
}

#[test]
fn ready_ordering_rejects_missing_required_region_event() {
    let events = vec![
        test_event(detguest_wire::record::EventKind::Hello, 1, vec![1; 12]),
        name_intern_event(2, 10, "wram"),
        region_register_event(3, 0, 10, 1),
        name_intern_event(4, 11, "framebuffer"),
        region_register_event(5, 1, 11, 1),
        ready_event(6, 3),
    ];

    let err = assert_ready_ordering(&events, 6, 3).expect_err("missing meta rejected");
    assert!(err.contains("\"meta\""), "unexpected error: {err}");
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
    const EVTC_V2_BASE_LEN: usize = EVTC_V1_LEN + 4;
    let evtc_shape_ok = match evtc.sec_version {
        1 => evtc.contents.len() == EVTC_V1_LEN,
        2 => {
            if evtc.contents.len() < EVTC_V2_BASE_LEN {
                false
            } else {
                let pending_count = u32::from_le_bytes(
                    evtc.contents[EVTC_V1_LEN..EVTC_V2_BASE_LEN]
                        .try_into()
                        .unwrap(),
                ) as usize;
                let mut at = EVTC_V2_BASE_LEN;
                let mut ok = true;
                for _ in 0..pending_count {
                    let Some(header_end) = at.checked_add(12) else {
                        ok = false;
                        break;
                    };
                    if header_end > evtc.contents.len() {
                        ok = false;
                        break;
                    }
                    let name_len =
                        u32::from_le_bytes(evtc.contents[at + 8..header_end].try_into().unwrap());
                    at = header_end;
                    if name_len != u32::MAX {
                        let Ok(name_len) = usize::try_from(name_len) else {
                            ok = false;
                            break;
                        };
                        let Some(name_end) = at.checked_add(name_len) else {
                            ok = false;
                            break;
                        };
                        if name_end > evtc.contents.len() {
                            ok = false;
                            break;
                        }
                        at = name_end;
                    }
                }
                ok && at == evtc.contents.len()
            }
        }
        _ => false,
    };
    if !evtc_shape_ok {
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
            bisect_on_divergence: Some(true),
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
fn pvblk_dev_vdb_phase_a_fixture_ready_regions() {
    run_pvblk_dev_vdb(false);
}

#[test]
#[ignore = "M9 Linux artifact gate: requires DH_M9_* artifacts and KVM"]
fn pvblk_dev_vdb() {
    run_pvblk_dev_vdb(true);
}

fn run_pvblk_dev_vdb(verify_replay: bool) {
    let Some(artifacts) = m9_artifacts().expect("M9 artifacts") else {
        return;
    };
    assert_initramfs_boot_contract(&artifacts.initramfs).expect("M9 initramfs boot.toml contract");
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

        let region_memory = svc
            .read_guest_memory(Request::new(proto::ReadGuestMemoryRequest {
                lease: Some(lease.clone()),
                ranges: Vec::new(),
                region_ranges: REQUIRED_M9_EXPECTED_REGIONS
                    .iter()
                    .map(|(region, layout_version)| proto::RegionRange {
                        region: (*region).into(),
                        layout_version: (*layout_version)
                            .try_into()
                            .expect("required M9 layout_version fits u32"),
                        offset: 0,
                        len: 16,
                    })
                    .collect(),
            }))
            .await
            .expect("ReadGuestMemory expected region_ranges")
            .into_inner();
        assert_eq!(region_memory.icount, run.icount);
        assert_eq!(
            region_memory.chunks.len(),
            REQUIRED_M9_EXPECTED_REGIONS.len(),
            "ReadGuestMemory must return one chunk per expected M9 region"
        );
        for ((region, _layout_version), chunk) in REQUIRED_M9_EXPECTED_REGIONS
            .iter()
            .zip(region_memory.chunks.iter())
        {
            assert_eq!(chunk.len(), 16, "region {region:?} read length");
        }

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

        let forked = svc
            .fork(Request::new(proto::ForkRequest {
                parent: Some(lease.clone()),
                count: 1,
                entropy_seeds: Vec::new(),
            }))
            .await
            .expect("Fork Ready parent")
            .into_inner();
        assert_eq!(forked.children.len(), 1, "Fork must return one child");
        let child = forked.children[0].clone();
        assert_ne!(
            child.slot_id, lease.slot_id,
            "Fork child must use a new slot"
        );

        let child_run = svc
            .run(Request::new(proto::RunRequest {
                lease: Some(child.clone()),
                until: Some(proto::run_request::Until::IcountBudget(FORK_CHILD_BUDGET)),
                hard_icount_cap: 0,
                capture: None,
            }))
            .await
            .expect("Run fork child")
            .into_inner();
        assert!(
            child_run.reason == i32::from(proto::StopReason::BudgetReached)
                || child_run.reason == i32::from(proto::StopReason::GuestHalted),
            "fork child run stopped with unexpected reason {}",
            child_run.reason
        );
        assert!(
            child_run.icount > ready_snapshot.icount,
            "fork child must advance beyond the Ready boundary"
        );

        let child_snapshot = svc
            .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                lease: Some(child.clone()),
                seal_input_log: Some(true),
                capture: None,
            }))
            .await
            .expect("fork child snapshot")
            .into_inner();
        assert_eq!(child_snapshot.machine_config_hash, config_hash.to_vec());
        assert_eq!(
            child_snapshot.input_log_id.len(),
            32,
            "fork child snapshot must seal a DHILOG segment"
        );
        svc.destroy_vm(Request::new(proto::DestroyVmRequest { lease: Some(child) }))
            .await
            .expect("destroy fork child");

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

        if verify_replay {
            let done =
                verify_replay_done(&svc, initial_snapshot, ready_snapshot.input_log_id.clone())
                    .await
                    .expect("VerifyReplay Done");
            assert_eq!(done.total_icount, run.icount);
            assert_eq!(
                done.end_state_hash.expect("VerifyReplay end hash").hash,
                ready_state_hash.hash,
                "live run and replay must end with the same lAPIC+bus-device state hash"
            );
        }
    });

    let game_after = file_evidence(&artifacts.game_image)
        .unwrap_or_else(|e| panic!("{DH_M9_GAME_IMAGE} evidence after run: {e}"));
    assert_eq!(
        game_after, game_before,
        "DH_M9_GAME_IMAGE source bytes and mtime must not change"
    );
}
