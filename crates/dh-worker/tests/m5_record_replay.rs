//! M5 ACCEPT (bead a5e): a seeded, scripted pad sequence spanning 60
//! guest-seconds of vns, recorded on the pad-echo guest from a real
//! snapshot, then replayed from `(snapshot, DHILOG)` — every replay must
//! reproduce `end_state_hash` AND every EPOCH_HASH, 100 times, with zero
//! divergence. The replay engine verifies each epoch link at the link
//! point, checks `end_vns` and `end_state_hash` at END, and reseals the
//! log byte-identically (the strongest equality the layer states).
//!
//! NON-1:1 CLOCK (load-bearing): the segment runs under a 10_000:1
//! vns/icount ratio, so 100k icount = one guest-second and the 6M-icount
//! run ends at exactly 60e9 vns. The RECORD-side `vns == seconds * 1e9`
//! assert is the independent oracle for the clock arithmetic; on the
//! replay side, end_vns finally flows through replay_engine.rs's check
//! with a value distinct from the icount (its note: "masked by 1:1
//! clocks until a5e") — though once the header pins the clock ratio,
//! that check cannot fail independently of end_icount.
//!
//! GRID: quantum == epoch_len == 100k, so every recorded stop boundary
//! IS an epoch point — the hash chain is a pure function of the absolute
//! epoch grid and the recorded inputs, and replay's by-record
//! quantization links the identical chain. One EPOCH_HASH per
//! guest-second: 60 over the acceptance run.
//!
//! PLACEMENT: the a5e bead said tests/determinism, but the ARCH §1
//! normative rule "nothing depends on dh-worker" (pinned by
//! arch_dependency_rule.rs, dev edges included) forbids that package
//! from importing the replay/snapshot engines — so this acceptance
//! lives beside the bead-39w replay tests, like every M4 engine
//! acceptance before it.
//!
//! SHAPE: the x100 acceptance is `#[ignore]`d like the M4 perf gates —
//! minutes of deliberate, quiesced-box work, not ordinary-sweep load
//! (measured: 100 replays ≈ 10 min release on the lab box):
//!
//!   cargo test -p dh-worker --test m5_record_replay --release \
//!     -- --ignored --nocapture
//!
//! The unignored smoke keeps the same rig (same script source, same
//! non-1:1 clock) in every sweep at 6 guest-seconds x1 replay.
//!
//! HARDWARE-GATED: kvm-intel lane + lab box; self-skips elsewhere.

#![cfg(target_arch = "x86_64")]

mod common;

use std::sync::atomic::AtomicBool;
use std::{collections::BTreeMap, fs, path::Path};

use common::{gettid, kvm_available, spawn_store_blocking, TestResult, VmMem};
use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};
use dh_devices::entropy::{DetEntropy, PvEntropy};
use dh_devices::pad::PvPad;
use dh_devices::MmioBus;
use dh_inputlog::dhilog::{LogWriter, SegmentHeader};
use dh_inputlog::reader::{LogReader, RecordBody};
use dh_proto::v1 as proto;
use dh_proto::v1::hypervisor_worker_server::HypervisorWorker;
use dh_vmm::boundary::BoundaryError;
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::dirty::PAGE_SIZE;
use dh_vmm::hash::StateHashChain;
use dh_vmm::kvm::{KvmSystem, SlotVm};
use dh_vmm::recording::DeviceRail;
use dh_vmm::runctl::{run_segment_with_epochs, Segment, SegmentOutcome, StopReason, Until};
use dh_vmm::vt::ClockRatio;
use dh_vmm::SlotState;
use dh_worker::replay_engine::replay_segment;
use dh_worker::runtime::runtime_hash_device_sections;
use dh_worker::snapshot_engine::{
    take_snapshot, BoundaryState, PageSource, DEVICE_BLOB_FORMAT_DHSNAP,
};
use kvm_ioctls::VcpuExit;
use snapstore_manifest::{input_log::InputLogContainer, DeviceBlob, Manifest};
use snapstore_types::SnapshotRef;
use tonic::Request;
use vm_memory::{Bytes, GuestAddress};

const MEM: u64 = 16 << 20;

/// One run quantum AND the epoch grid (see module docs: keeping them
/// equal makes every stop boundary an epoch point).
const QUANTUM: u64 = 100_000;

/// vns per icount: 100k icount = 1e9 vns = one guest-second.
const CLOCK_NUM: u32 = 10_000;
const VNS_PER_SECOND: u64 = 1_000_000_000;

const ACCEPT_SECONDS: u64 = 60;
const ACCEPT_REPLAYS: usize = 100;
const SMOKE_SECONDS: u64 = 6;

/// The script seed — any fixed value; the sequence is a pure function
/// of it (that is the "seeded, scripted" in the acceptance wording).
const SCRIPT_SEED: u64 = 0x00A5_E060_5EED;
const CORPUS_SECONDS: u64 = SMOKE_SECONDS;
const CORPUS_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/record_replay_corpus/pad_echo_6s"
);
const CORPUS_ROOT_SPARSE: &str = "root-sparse.bin";
const CORPUS_DHSNAP: &str = "root.dhsnap";
const CORPUS_DHILOG: &str = "recording.dhilog";
const CORPUS_EXPECTED: &str = "expected.txt";
const M9_LINUX_CORPUS_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/record_replay_corpus/m9_linux_post_ready"
);
const M9_LINUX_CORPUS_EXPECTED: &str = "expected.txt";
const M9_LINUX_CORPUS_FRAMES: u32 = 5;
const M9_LINUX_CORPUS_HARD_CAP: u64 = 5_000_000;
const M9_LINUX_CORPUS_EPOCH_LEN: u64 = 745_000;
const M9_LINUX_META_IO_MAGIC_OFF: u64 = 32;
const M9_LINUX_META_IO_PROOF_LEN: u64 = 24;
const SPARSE_ROOT_MAGIC: &[u8; 8] = b"DHRRPG01";
const DETERMINISM_CLASS_LOCK: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../ci/determinism-class.lock"
);

#[test]
#[ignore = "M9 Linux acceptance: requires KVM dirty-ring support and staged DH_M9_* artifacts"]
fn linux_m5_record_replay_post_ready_corpus_reverifies() -> TestResult<()> {
    let Some(evidence) = m9_linux_record_replay_evidence(
        "m5_record_replay::linux_m5_record_replay_post_ready_corpus",
    )?
    else {
        return Ok(());
    };

    let expected_bytes = fs::read(m9_linux_fixture_path(M9_LINUX_CORPUS_EXPECTED))
        .map_err(|e| format!("read M9 Linux expected manifest: {e}"))?;
    let expected = expected_map(&expected_bytes);
    assert_m9_linux_expected_matches(&evidence, &expected);

    eprintln!(
        "m9-linux-rr artifacts bzImage={} initramfs={} base={} game={}",
        hex(&evidence.artifact_hashes.bzimage),
        hex(&evidence.artifact_hashes.initramfs),
        hex(&evidence.artifact_hashes.base_image),
        hex(&evidence.artifact_hashes.game_image)
    );
    eprintln!(
        "m9-linux-rr frames={} hard_cap={} end_icount={} epochs={} end_state_hash={} checksum={:#x} dhilog={}",
        M9_LINUX_CORPUS_FRAMES,
        M9_LINUX_CORPUS_HARD_CAP,
        evidence.parsed.end_icount,
        evidence.parsed.epochs.len(),
        hex(&evidence.parsed.end_state_hash),
        evidence.meta_pvblk_checksum,
        evidence.parsed.dhilog_blake3
    );
    Ok(())
}

#[test]
#[ignore = "explicit M9 re-baseline only: set DH_WORKER_REGEN_M9_LINUX_RR_CORPUS=1"]
fn regenerate_m9_rr_corpus_manifest_for_reference_host() -> TestResult<()> {
    if std::env::var_os("DH_WORKER_REGEN_M9_LINUX_RR_CORPUS").is_none() {
        panic!("set DH_WORKER_REGEN_M9_LINUX_RR_CORPUS=1 to rewrite the M9 Linux manifest");
    }
    let Some(evidence) = m9_linux_record_replay_evidence(
        "m5_record_replay::regenerate_m9_rr_corpus_manifest_for_reference_host",
    )?
    else {
        return Ok(());
    };
    fs::create_dir_all(M9_LINUX_CORPUS_DIR)
        .map_err(|e| format!("create M9 Linux corpus dir: {e}"))?;
    fs::write(
        m9_linux_fixture_path(M9_LINUX_CORPUS_EXPECTED),
        expected_m9_linux_text(&evidence),
    )
    .map_err(|e| format!("write M9 Linux expected manifest: {e}"))?;
    Ok(())
}

fn hex(b: &[u8; 32]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn blake3_hex(bytes: &[u8]) -> String {
    hex(blake3::hash(bytes).as_bytes())
}

fn parse_hex32(s: &str) -> [u8; 32] {
    assert_eq!(s.len(), 64, "hex32 length");
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).expect("hex byte");
    }
    out
}

fn fixture_path(name: &str) -> std::path::PathBuf {
    Path::new(CORPUS_DIR).join(name)
}

fn m9_linux_fixture_path(name: &str) -> std::path::PathBuf {
    Path::new(M9_LINUX_CORPUS_DIR).join(name)
}

fn determinism_class_lock_bytes() -> Vec<u8> {
    fs::read(DETERMINISM_CLASS_LOCK).expect("ci/determinism-class.lock")
}

fn arr32(bytes: &[u8], label: &str) -> TestResult<[u8; 32]> {
    bytes
        .try_into()
        .map_err(|_| format!("{label} must be 32 bytes, got {}", bytes.len()))
}

fn reference_host_id() -> TestResult<String> {
    let hostname = fs::read_to_string("/proc/sys/kernel/hostname")
        .map_err(|e| format!("read /proc/sys/kernel/hostname: {e}"))?;
    Ok(hostname.trim().to_string())
}

fn input_log_payload(
    store: &snapstore_client::blocking::SnapstoreClient,
    input_log_id: &[u8],
) -> TestResult<Vec<u8>> {
    let id = arr32(input_log_id, "input log id")?;
    let container = store
        .get_input_log(snapstore_types::LogId::from_bytes(id))
        .map_err(|e| format!("get_input_log: {e}"))?;
    let decoded = InputLogContainer::decode(&container)
        .map_err(|e| format!("input log container decode: {e}"))?;
    Ok(decoded.payload().to_vec())
}

#[derive(Debug)]
struct ParsedM9LinuxLog {
    dhilog_blake3: String,
    base_snapshot_id: [u8; 32],
    end_snapshot_id: [u8; 32],
    machine_config_hash: [u8; 32],
    record_count: u64,
    records_applied: u64,
    end_icount: u64,
    end_vns: u64,
    end_state_hash: [u8; 32],
    epochs: Vec<(u64, u64, [u8; 32])>,
}

#[derive(Debug)]
struct VerifyReplayEvidence {
    done: proto::VerifyDone,
    epoch_ok_count: usize,
}

#[derive(Debug)]
struct M9LinuxRecordReplayEvidence {
    host_id: String,
    artifact_hashes: common::M9CachedHashes,
    config_hash: [u8; 32],
    ready_snapshot_ref: [u8; 32],
    post_snapshot_ref: [u8; 32],
    post_state_hash: [u8; 32],
    frame_counter: u32,
    meta_pvblk_checksum: u64,
    log: Vec<u8>,
    parsed: ParsedM9LinuxLog,
    verify: VerifyReplayEvidence,
}

fn parse_m9_linux_log(log: &[u8]) -> TestResult<ParsedM9LinuxLog> {
    let reader =
        LogReader::parse(log).map_err(|e| format!("M9 Linux DHILOG parse failed: {e:?}"))?;
    let header = reader.header();
    if !header.has_epoch_hashes() {
        return Err("M9 Linux DHILOG header does not advertise EPOCH_HASH records".into());
    }
    let epochs: Vec<(u64, u64, [u8; 32])> = reader
        .aux()
        .filter_map(|rec| match rec.body() {
            RecordBody::EpochHash {
                epoch_index,
                chain_value,
            } => Some((epoch_index, rec.icount(), chain_value)),
            _ => None,
        })
        .collect();
    if epochs.is_empty() {
        return Err("M9 Linux DHILOG contains zero EPOCH_HASH records".into());
    }
    let records_applied = reader.canonical().count() as u64;
    let (_end_reason, end_state_hash) = reader.end();
    if end_state_hash != header.end_state_hash {
        return Err("M9 Linux DHILOG END state hash does not match header".into());
    }
    Ok(ParsedM9LinuxLog {
        dhilog_blake3: blake3_hex(log),
        base_snapshot_id: header.base_snapshot_id,
        end_snapshot_id: header.end_snapshot_id,
        machine_config_hash: header.machine_config_hash,
        record_count: header.record_count,
        records_applied,
        end_icount: header.end_icount,
        end_vns: header.end_vns,
        end_state_hash: header.end_state_hash,
        epochs,
    })
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
    let frame = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    if frame != 0 {
        return Err(format!("M9 Linux meta IO proof frame {frame}, expected 0"));
    }
    let checksum = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
    if checksum == 0 {
        return Err("M9 Linux meta IO proof checksum must be nonzero".into());
    }
    Ok(checksum)
}

async fn verify_replay_evidence(
    svc: &dh_worker::service::WorkerService,
    base: proto::SnapshotRef,
    input_log_id: Vec<u8>,
) -> TestResult<VerifyReplayEvidence> {
    use tokio_stream::StreamExt;

    let mut stream = svc
        .verify_replay(Request::new(proto::VerifyReplayRequest {
            base: Some(base),
            log: Some(proto::verify_replay_request::Log::InputLogId(input_log_id)),
            bisect_on_divergence: Some(false),
        }))
        .await
        .map_err(|e| format!("VerifyReplay: {e}"))?
        .into_inner();

    let mut epoch_ok_count = 0usize;
    let mut done = None;
    while let Some(progress) = stream.as_mut().next().await {
        let progress = progress.map_err(|e| format!("VerifyReplay progress: {e}"))?;
        match progress.msg {
            Some(proto::verify_replay_progress::Msg::EpochOk(_)) => {
                if done.is_some() {
                    return Err("VerifyReplay emitted EpochOk after Done".into());
                }
                epoch_ok_count += 1;
            }
            Some(proto::verify_replay_progress::Msg::Done(msg)) => {
                if done.replace(msg).is_some() {
                    return Err("VerifyReplay emitted duplicate Done".into());
                }
            }
            Some(proto::verify_replay_progress::Msg::Divergence(divergence)) => {
                return Err(format!("VerifyReplay divergence: {divergence:?}"));
            }
            None => return Err("VerifyReplay emitted empty progress message".into()),
        }
    }
    let done = done.ok_or_else(|| "VerifyReplay ended without Done".to_string())?;
    Ok(VerifyReplayEvidence {
        done,
        epoch_ok_count,
    })
}

fn m9_linux_record_replay_evidence(
    test_name: &str,
) -> TestResult<Option<M9LinuxRecordReplayEvidence>> {
    let Some(artifacts) = common::m9_artifacts(test_name)? else {
        return Ok(None);
    };
    let artifact_hashes = common::M9CachedHashes {
        bzimage: common::hash_file(&artifacts.bzimage)?,
        initramfs: common::hash_file(&artifacts.initramfs)?,
        base_image: common::hash_file(&artifacts.base_image)?,
        game_image: common::hash_file(&artifacts.game_image)?,
    };
    let Some(ready) = common::m9_linux_ready_snapshot_with_config(test_name, 2, |config| {
        config.epoch_len = M9_LINUX_CORPUS_EPOCH_LEN;
    })?
    else {
        return Ok(None);
    };
    let ready_snapshot_ref = arr32(&ready.ready_snapshot_ref.hash, "READY snapshot ref")?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("test runtime: {e}"))?;
    let (run, post_snapshot, meta_proof) = rt.block_on(async {
        let run = ready
            .svc
            .run(Request::new(proto::RunRequest {
                lease: Some(ready.lease.clone()),
                until: Some(proto::run_request::Until::FrameBudget(
                    M9_LINUX_CORPUS_FRAMES,
                )),
                hard_icount_cap: M9_LINUX_CORPUS_HARD_CAP,
                capture: None,
            }))
            .await
            .map_err(|e| format!("Run M9 Linux post-READY segment: {e}"))?
            .into_inner();
        if run.reason != i32::from(proto::StopReason::BudgetReached) {
            return Err(format!(
                "M9 Linux run stopped with {}, expected BudgetReached",
                run.reason
            ));
        }
        if run.frames_elapsed != u64::from(M9_LINUX_CORPUS_FRAMES) {
            return Err(format!(
                "M9 Linux run frames_elapsed {}, expected {M9_LINUX_CORPUS_FRAMES}",
                run.frames_elapsed
            ));
        }

        let meta = ready
            .svc
            .read_guest_memory(Request::new(proto::ReadGuestMemoryRequest {
                lease: Some(ready.lease.clone()),
                ranges: Vec::new(),
                region_ranges: vec![proto::RegionRange {
                    region: "meta".into(),
                    layout_version: 1,
                    offset: M9_LINUX_META_IO_MAGIC_OFF,
                    len: M9_LINUX_META_IO_PROOF_LEN,
                }],
            }))
            .await
            .map_err(|e| format!("ReadGuestMemory M9 Linux meta IO proof: {e}"))?
            .into_inner();
        let meta_proof = meta
            .chunks
            .first()
            .cloned()
            .ok_or_else(|| "ReadGuestMemory returned no M9 Linux meta IO proof".to_string())?;

        let post_snapshot = ready
            .svc
            .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                lease: Some(ready.lease.clone()),
                seal_input_log: Some(true),
                capture: None,
            }))
            .await
            .map_err(|e| format!("TakeSnapshot M9 Linux post-READY segment: {e}"))?
            .into_inner();
        Ok::<_, String>((run, post_snapshot, meta_proof))
    })?;

    let _run_icount = run.icount;
    let post_snapshot_ref = post_snapshot
        .snapshot
        .as_ref()
        .ok_or_else(|| "M9 Linux post snapshot returned no snapshot ref".to_string())
        .and_then(|snapshot| arr32(&snapshot.hash, "M9 Linux post snapshot ref"))?;
    let post_state_hash = post_snapshot
        .state_hash
        .as_ref()
        .ok_or_else(|| "M9 Linux post snapshot returned no state hash".to_string())
        .and_then(|hash| arr32(&hash.hash, "M9 Linux post snapshot state hash"))?;
    let log = input_log_payload(&ready.store, &post_snapshot.input_log_id)?;
    let parsed = parse_m9_linux_log(&log)?;
    if parsed.base_snapshot_id != ready_snapshot_ref {
        return Err("M9 Linux DHILOG base_snapshot_id does not match READY snapshot".into());
    }
    if parsed.end_snapshot_id != post_snapshot_ref {
        return Err("M9 Linux DHILOG end_snapshot_id does not match post snapshot".into());
    }
    if parsed.machine_config_hash != ready.config_hash {
        return Err("M9 Linux DHILOG machine_config_hash does not match READY config".into());
    }
    if parsed.end_state_hash != post_state_hash {
        return Err("M9 Linux DHILOG END state hash does not match post snapshot".into());
    }
    let meta_pvblk_checksum = assert_m9_meta_io_proof(&meta_proof)?;

    let verify = rt.block_on(async {
        let verify = verify_replay_evidence(
            &ready.svc,
            ready.ready_snapshot_ref.clone(),
            post_snapshot.input_log_id.clone(),
        )
        .await?;
        let _ = ready
            .svc
            .destroy_vm(Request::new(proto::DestroyVmRequest {
                lease: Some(ready.lease.clone()),
            }))
            .await;
        Ok::<_, String>(verify)
    })?;
    if verify.epoch_ok_count == 0 {
        return Err("VerifyReplay emitted no EpochOk progress".into());
    }
    if verify.epoch_ok_count != parsed.epochs.len() {
        return Err(format!(
            "VerifyReplay EpochOk count {} did not match parsed EPOCH_HASH count {}",
            verify.epoch_ok_count,
            parsed.epochs.len()
        ));
    }
    if verify.done.total_icount != parsed.end_icount {
        return Err(format!(
            "VerifyReplay Done total_icount {}, expected parsed end_icount {}",
            verify.done.total_icount, parsed.end_icount
        ));
    }
    let verify_hash = verify
        .done
        .end_state_hash
        .as_ref()
        .ok_or_else(|| "VerifyReplay Done returned no end_state_hash".to_string())
        .and_then(|hash| arr32(&hash.hash, "VerifyReplay Done end_state_hash"))?;
    if verify_hash != post_state_hash {
        return Err("VerifyReplay Done end_state_hash does not match post snapshot".into());
    }

    Ok(Some(M9LinuxRecordReplayEvidence {
        host_id: reference_host_id()?,
        artifact_hashes,
        config_hash: ready.config_hash,
        ready_snapshot_ref,
        post_snapshot_ref,
        post_state_hash,
        frame_counter: post_snapshot.frame_counter,
        meta_pvblk_checksum,
        log,
        parsed,
        verify,
    }))
}

/// SplitMix64 — tiny, dependency-free, and fully determined by the seed.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The scripted pad sequence: one PAD_SET per inter-quantum boundary
/// (`seconds - 1` of them — none after the final quantum; a sealed
/// segment ends on a clean boundary with nothing pending).
///
/// INVARIANT (checked): no value is 0 (the boot latch) and no two
/// adjacent values are equal — `assert_table_eras` compares the
/// guest-observed era sequence EXACTLY, so a seed change that
/// introduced a collapsed era must fail here, loudly, not weaken the
/// era assertion silently.
fn pad_script(seconds: u64) -> Vec<u32> {
    let mut s = SCRIPT_SEED;
    let script: Vec<u32> = (1..seconds).map(|_| splitmix64(&mut s) as u32).collect();
    assert!(
        !script.contains(&0) && script.windows(2).all(|w| w[0] != w[1]),
        "script invariant violated for seed {SCRIPT_SEED:#x}: pick a seed with \
         no zeros and no adjacent-equal values (eras would collapse)"
    );
    script
}

fn snapshot_ref_from_hex(s: &str) -> SnapshotRef {
    SnapshotRef::from_bytes(parse_hex32(s))
}

fn sparse_root_pages(slot: &SlotVm) -> Vec<(u64, Vec<u8>)> {
    let mut nonzero = Vec::new();
    for idx in 0..MEM / PAGE_SIZE {
        let mut page = vec![0u8; PAGE_SIZE as usize];
        slot.guest_mem
            .read_slice(&mut page, GuestAddress(idx * PAGE_SIZE))
            .unwrap();
        if page.iter().any(|&b| b != 0) {
            nonzero.push((idx, page));
        }
    }
    nonzero
}

fn encode_sparse_root(nonzero_pages: &[(u64, Vec<u8>)]) -> Vec<u8> {
    let mut out =
        Vec::with_capacity(8 + 8 + 8 + 8 + nonzero_pages.len() * (8 + PAGE_SIZE as usize));
    out.extend_from_slice(SPARSE_ROOT_MAGIC);
    out.extend_from_slice(&MEM.to_le_bytes());
    out.extend_from_slice(&PAGE_SIZE.to_le_bytes());
    out.extend_from_slice(&(nonzero_pages.len() as u64).to_le_bytes());
    let mut prev = None;
    for (idx, page) in nonzero_pages {
        assert_eq!(page.len(), PAGE_SIZE as usize);
        if let Some(prev) = prev {
            assert!(*idx > prev, "sparse root pages must be strictly sorted");
        }
        prev = Some(*idx);
        out.extend_from_slice(&idx.to_le_bytes());
        out.extend_from_slice(page);
    }
    out
}

fn u64_at(bytes: &[u8], at: &mut usize) -> Result<u64, String> {
    let end = at.checked_add(8).ok_or("offset overflow")?;
    let raw = bytes
        .get(*at..end)
        .ok_or_else(|| format!("sparse root truncated at byte {at}"))?;
    *at = end;
    Ok(u64::from_le_bytes(raw.try_into().unwrap()))
}

fn decode_sparse_root(bytes: &[u8]) -> Result<Vec<(u64, Vec<u8>)>, String> {
    if bytes.get(0..8) != Some(SPARSE_ROOT_MAGIC) {
        return Err("bad sparse-root magic".into());
    }
    let mut at = 8;
    let mem = u64_at(bytes, &mut at)?;
    let page_size = u64_at(bytes, &mut at)?;
    let count = u64_at(bytes, &mut at)?;
    if mem != MEM || page_size != PAGE_SIZE {
        return Err(format!(
            "sparse root shape mismatch: mem={mem}, page_size={page_size}"
        ));
    }
    if count > MEM / PAGE_SIZE {
        return Err(format!("sparse page count out of range: {count}"));
    }
    let expected_len = 32usize
        .checked_add(
            usize::try_from(count)
                .map_err(|_| "sparse page count does not fit usize".to_string())?
                .checked_mul(8 + PAGE_SIZE as usize)
                .ok_or_else(|| "sparse root length overflow".to_string())?,
        )
        .ok_or_else(|| "sparse root length overflow".to_string())?;
    if bytes.len() != expected_len {
        return Err(format!(
            "sparse root length mismatch: got {}, want {expected_len}",
            bytes.len()
        ));
    }
    let mut pages = Vec::with_capacity(count as usize);
    let mut prev = None;
    for _ in 0..count {
        let idx = u64_at(bytes, &mut at)?;
        if idx >= MEM / PAGE_SIZE {
            return Err(format!("sparse page index out of range: {idx}"));
        }
        if let Some(prev) = prev {
            if idx <= prev {
                return Err("sparse page indices are not strictly ascending".into());
            }
        }
        prev = Some(idx);
        let end = at
            .checked_add(PAGE_SIZE as usize)
            .ok_or_else(|| "page offset overflow".to_string())?;
        let page = bytes
            .get(at..end)
            .ok_or_else(|| format!("sparse page {idx} truncated"))?
            .to_vec();
        if page.iter().all(|&b| b == 0) {
            return Err(format!("sparse page {idx} is all zero"));
        }
        at = end;
        pages.push((idx, page));
    }
    if at != bytes.len() {
        return Err(format!(
            "sparse root has {} trailing bytes",
            bytes.len() - at
        ));
    }
    Ok(pages)
}

fn expand_sparse_root(nonzero: Vec<(u64, Vec<u8>)>) -> Vec<(u64, Vec<u8>)> {
    let mut sparse = BTreeMap::new();
    for (idx, page) in nonzero {
        assert!(sparse.insert(idx, page).is_none(), "duplicate sparse page");
    }
    (0..MEM / PAGE_SIZE)
        .map(|idx| {
            let page = sparse
                .remove(&idx)
                .unwrap_or_else(|| vec![0u8; PAGE_SIZE as usize]);
            (idx, page)
        })
        .collect()
}

fn expected_map(bytes: &[u8]) -> BTreeMap<String, String> {
    let text = std::str::from_utf8(bytes).expect("expected manifest is utf8");
    let mut out = BTreeMap::new();
    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .unwrap_or_else(|| panic!("expected.txt:{} missing '='", lineno + 1));
        assert!(
            out.insert(key.to_string(), value.to_string()).is_none(),
            "duplicate expected key {key}"
        );
    }
    out
}

fn assert_expected_key_set(m: &BTreeMap<String, String>) {
    let epoch_count = expected_u64(m, "epoch_hashes_verified");
    let mut allowed = [
        "name",
        "seconds",
        "mem_bytes",
        "page_size",
        "nonzero_pages",
        "determinism_class_lock_blake3",
        "snapshot_ref",
        "machine_config_hash",
        "root_sparse_blake3",
        "root_dhsnap_blake3",
        "dhilog_blake3",
        "dhilog_records",
        "end_icount",
        "end_vns",
        "records_applied",
        "epoch_hashes_verified",
        "end_state_hash",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<std::collections::BTreeSet<_>>();
    for i in 1..=epoch_count {
        allowed.insert(format!("epoch_{i}"));
    }
    let actual = m.keys().cloned().collect::<std::collections::BTreeSet<_>>();
    assert_eq!(actual, allowed, "expected.txt key set changed");
}

fn assert_m9_linux_expected_key_set(m: &BTreeMap<String, String>) {
    let epoch_count = expected_u64(m, "epoch_hashes_verified");
    let epoch_keys = m
        .keys()
        .filter_map(|key| {
            let suffix = key.strip_prefix("epoch_")?;
            suffix.parse::<u64>().ok().map(|_| key.clone())
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        epoch_keys.len() as u64,
        epoch_count,
        "M9 Linux expected epoch key count changed"
    );
    for key in &epoch_keys {
        key.strip_prefix("epoch_")
            .expect("epoch prefix")
            .parse::<u64>()
            .unwrap_or_else(|_| panic!("invalid M9 Linux epoch key {key}"));
    }

    let mut allowed = [
        "name",
        "host_id",
        "determinism_class_lock_blake3",
        "bzimage_blake3",
        "initramfs_blake3",
        "base_image_blake3",
        "game_image_blake3",
        "mem_bytes",
        "epoch_len",
        "machine_config_hash",
        "ready_snapshot_ref",
        "end_snapshot_ref",
        "run_until",
        "hard_icount_cap",
        "dhilog_blake3",
        "dhilog_records",
        "records_applied",
        "epoch_hashes_verified",
        "end_icount",
        "end_vns",
        "end_state_hash",
        "frame_counter",
        "meta_pvblk_checksum",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<std::collections::BTreeSet<_>>();
    allowed.extend(epoch_keys);
    let actual = m.keys().cloned().collect::<std::collections::BTreeSet<_>>();
    assert_eq!(actual, allowed, "M9 Linux expected.txt key set changed");
}

fn expected_m9_linux_text(evidence: &M9LinuxRecordReplayEvidence) -> String {
    let mut out = String::new();
    out.push_str("# M9 Linux post-READY record/replay corpus manifest.\n");
    out.push_str("# Re-baseline only with reviewed M9 fixture, host, or hash-contract changes.\n");
    out.push_str("name=m9_linux_post_ready\n");
    out.push_str(&format!("host_id={}\n", evidence.host_id));
    out.push_str(&format!(
        "determinism_class_lock_blake3={}\n",
        blake3_hex(&determinism_class_lock_bytes())
    ));
    out.push_str(&format!(
        "bzimage_blake3={}\n",
        hex(&evidence.artifact_hashes.bzimage)
    ));
    out.push_str(&format!(
        "initramfs_blake3={}\n",
        hex(&evidence.artifact_hashes.initramfs)
    ));
    out.push_str(&format!(
        "base_image_blake3={}\n",
        hex(&evidence.artifact_hashes.base_image)
    ));
    out.push_str(&format!(
        "game_image_blake3={}\n",
        hex(&evidence.artifact_hashes.game_image)
    ));
    out.push_str(&format!("mem_bytes={}\n", common::M9_LINUX_MEM_BYTES));
    out.push_str(&format!("epoch_len={M9_LINUX_CORPUS_EPOCH_LEN}\n"));
    out.push_str(&format!(
        "machine_config_hash={}\n",
        hex(&evidence.config_hash)
    ));
    out.push_str(&format!(
        "ready_snapshot_ref={}\n",
        hex(&evidence.ready_snapshot_ref)
    ));
    out.push_str(&format!(
        "end_snapshot_ref={}\n",
        hex(&evidence.post_snapshot_ref)
    ));
    out.push_str(&format!(
        "run_until=frame_budget:{M9_LINUX_CORPUS_FRAMES}\n"
    ));
    out.push_str(&format!("hard_icount_cap={M9_LINUX_CORPUS_HARD_CAP}\n"));
    out.push_str(&format!("dhilog_blake3={}\n", blake3_hex(&evidence.log)));
    out.push_str(&format!(
        "dhilog_records={}\n",
        evidence.parsed.record_count
    ));
    out.push_str(&format!(
        "records_applied={}\n",
        evidence.parsed.records_applied
    ));
    out.push_str(&format!(
        "epoch_hashes_verified={}\n",
        evidence.parsed.epochs.len()
    ));
    out.push_str(&format!("end_icount={}\n", evidence.parsed.end_icount));
    out.push_str(&format!("end_vns={}\n", evidence.parsed.end_vns));
    out.push_str(&format!(
        "end_state_hash={}\n",
        hex(&evidence.parsed.end_state_hash)
    ));
    out.push_str(&format!("frame_counter={}\n", evidence.frame_counter));
    out.push_str(&format!(
        "meta_pvblk_checksum={}\n",
        evidence.meta_pvblk_checksum
    ));
    for (epoch_index, icount, chain_value) in &evidence.parsed.epochs {
        out.push_str(&format!(
            "epoch_{epoch_index}={icount}:{}\n",
            hex(chain_value)
        ));
    }
    out
}

fn assert_m9_linux_expected_matches(
    evidence: &M9LinuxRecordReplayEvidence,
    expected: &BTreeMap<String, String>,
) {
    assert_m9_linux_expected_key_set(expected);
    assert_eq!(expected_value(expected, "name"), "m9_linux_post_ready");
    assert_eq!(expected_value(expected, "host_id"), evidence.host_id);
    assert_eq!(
        expected_value(expected, "determinism_class_lock_blake3"),
        blake3_hex(&determinism_class_lock_bytes())
    );
    assert_eq!(
        expected_value(expected, "bzimage_blake3"),
        hex(&evidence.artifact_hashes.bzimage)
    );
    assert_eq!(
        expected_value(expected, "initramfs_blake3"),
        hex(&evidence.artifact_hashes.initramfs)
    );
    assert_eq!(
        expected_value(expected, "base_image_blake3"),
        hex(&evidence.artifact_hashes.base_image)
    );
    assert_eq!(
        expected_value(expected, "game_image_blake3"),
        hex(&evidence.artifact_hashes.game_image)
    );
    assert_eq!(
        expected_u64(expected, "mem_bytes"),
        common::M9_LINUX_MEM_BYTES
    );
    assert_eq!(
        expected_u64(expected, "epoch_len"),
        M9_LINUX_CORPUS_EPOCH_LEN
    );
    assert_eq!(
        expected_value(expected, "machine_config_hash"),
        hex(&evidence.config_hash)
    );
    assert_eq!(
        expected_value(expected, "ready_snapshot_ref"),
        hex(&evidence.ready_snapshot_ref)
    );
    assert_eq!(
        expected_value(expected, "end_snapshot_ref"),
        hex(&evidence.post_snapshot_ref)
    );
    assert_eq!(
        expected_value(expected, "run_until"),
        format!("frame_budget:{M9_LINUX_CORPUS_FRAMES}")
    );
    assert_eq!(
        expected_u64(expected, "hard_icount_cap"),
        M9_LINUX_CORPUS_HARD_CAP
    );
    assert_eq!(
        expected_value(expected, "dhilog_blake3"),
        blake3_hex(&evidence.log)
    );
    assert_eq!(
        evidence.parsed.dhilog_blake3,
        blake3_hex(&evidence.log),
        "parsed DHILOG hash must match the stored payload"
    );
    assert_eq!(
        expected_u64(expected, "dhilog_records"),
        evidence.parsed.record_count
    );
    assert_eq!(
        expected_u64(expected, "records_applied"),
        evidence.parsed.records_applied
    );
    assert_eq!(
        expected_u64(expected, "epoch_hashes_verified") as usize,
        evidence.parsed.epochs.len()
    );
    assert_eq!(
        evidence.verify.epoch_ok_count,
        evidence.parsed.epochs.len(),
        "VerifyReplay EpochOk count must match parsed EPOCH_HASH records"
    );
    assert_eq!(
        evidence.verify.done.total_icount, evidence.parsed.end_icount,
        "VerifyReplay Done total_icount must match the DHILOG END icount"
    );
    let verify_done_hash: [u8; 32] = evidence
        .verify
        .done
        .end_state_hash
        .as_ref()
        .expect("VerifyReplay Done end_state_hash")
        .hash
        .as_slice()
        .try_into()
        .expect("VerifyReplay Done end_state_hash must be 32 bytes");
    assert_eq!(
        verify_done_hash, evidence.post_state_hash,
        "VerifyReplay Done hash must match live snapshot hash"
    );
    assert_eq!(
        expected_u64(expected, "end_icount"),
        evidence.parsed.end_icount
    );
    assert_eq!(expected_u64(expected, "end_vns"), evidence.parsed.end_vns);
    assert_eq!(
        expected_value(expected, "end_state_hash"),
        hex(&evidence.parsed.end_state_hash)
    );
    assert_eq!(
        evidence.parsed.end_state_hash, evidence.post_state_hash,
        "parsed END hash must match live snapshot hash"
    );
    assert_eq!(
        expected_u64(expected, "frame_counter"),
        u64::from(evidence.frame_counter)
    );
    assert_eq!(
        expected_u64(expected, "meta_pvblk_checksum"),
        evidence.meta_pvblk_checksum
    );
    for (epoch_index, icount, chain_value) in &evidence.parsed.epochs {
        assert_eq!(
            format!("{icount}:{}", hex(chain_value)),
            expected_value(expected, &format!("epoch_{epoch_index}"))
        );
    }
}

fn expected_value<'a>(m: &'a BTreeMap<String, String>, key: &str) -> &'a str {
    m.get(key)
        .unwrap_or_else(|| panic!("expected.txt missing key {key}"))
}

fn expected_u64(m: &BTreeMap<String, String>, key: &str) -> u64 {
    expected_value(m, key).parse().unwrap()
}

fn config() -> MachineConfig {
    let mut c = MachineConfig::new(
        MEM,
        [0x5A; 32],
        BootSpec::Elf {
            kernel_hash: [0x5A; 32],
            cmdline: Vec::new(),
        },
    );
    c.epoch_len = QUANTUM;
    c.clock = ClockRatio::new(CLOCK_NUM, 1).expect("nonzero ratio");
    c
}

/// The pad-echo bus: the pad the guest polls plus the entropy device the
/// snapshot engines require on every bus. 0xD000_3000 is the joint-test
/// entropy base (replay_engine.rs, common::test_bus) — deliberately NOT
/// `entropy::PV_ENTROPY_BASE` (0xD000_2000), which collides with the
/// canonical clock base in that layout.
fn record_bus() -> MmioBus {
    let mut bus = MmioBus::new();
    bus.register(dh_devices::pad::PV_PAD_BASE, Box::new(PvPad::new()))
        .unwrap();
    bus.register(0xD000_3000, Box::new(PvEntropy::new()))
        .unwrap();
    bus
}

fn header_for(snap: &snapstore_types::SnapshotRef, cfg_hash: [u8; 32]) -> SegmentHeader {
    SegmentHeader {
        base_snapshot_id: snap.to_bytes(),
        entropy_seed: [0; 32], // zero ⇒ replay continues the snapshot PRNG
        machine_config_hash: cfg_hash,
        clock_num: CLOCK_NUM,
        clock_den: 1,
        encoder_fingerprint: 0,
    }
}

struct Recording {
    snapshot_ref: snapstore_types::SnapshotRef,
    log: Vec<u8>,
    cfg: MachineConfig,
    end_state_hash: [u8; 32],
    root_nonzero_pages: Vec<(u64, Vec<u8>)>,
    root_dhsnap: Vec<u8>,
}

/// Boot pad_echo, snapshot at the boot boundary, then record `seconds`
/// one-guest-second quanta with the scripted PAD_SET at every
/// inter-quantum boundary; seal from the final outcome.
fn record(store: &snapstore_client::blocking::SnapstoreClient, seconds: u64) -> Recording {
    let script = pad_script(seconds);

    dh_vmm::run::install_kick_handler().unwrap();
    let sys = KvmSystem::open().unwrap();
    let mut slot = sys.create_slot_vm(MEM).unwrap();
    dh_vmm::boot::load_and_enter(&slot, nanokernel::pad_echo_elf(), b"").unwrap();
    let counter = InstRetired::open_for_current_thread().unwrap();
    counter
        .route_overflow_to_thread(gettid(), dh_vmm::run::kick_signal())
        .unwrap();
    counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
    counter.reset().unwrap();
    counter.enable().unwrap();

    let cfg = config();
    let cfg_hash = cfg.config_hash().unwrap();
    let mut chain = StateHashChain::new(&cfg_hash, &[0; 32]);

    // The base snapshot at the boot boundary (counter at 0).
    let bus = record_bus();
    let entropy = DetEntropy::from_seed([0x42; 32]);
    let snap = take_snapshot(
        &slot,
        SlotState::Paused,
        &bus,
        &entropy,
        &cfg,
        BoundaryState {
            icount: 0,
            vns: 0,
            epoch_index: 0,
            hash_chain: chain.value(),
            agenda_empty: true,
        },
        PageSource::Full,
        store,
    )
    .expect("base snapshot")
    .snapshot_ref;
    let root_nonzero_pages = sparse_root_pages(&slot);
    let root_container = store
        .get_snapshot(snap.clone())
        .expect("read back base snapshot");
    let root_dhsnap = Manifest::decode(&root_container)
        .expect("decode base snapshot manifest")
        .device_blob
        .bytes;

    let rail = std::cell::RefCell::new(DeviceRail::new(
        bus,
        entropy,
        LogWriter::new(header_for(&snap, cfg_hash)),
        VmMem(slot.guest_mem.clone()),
    ));
    let pause = AtomicBool::new(false);

    let run_one = |slot: &mut SlotVm, chain: &mut StateHashChain| -> SegmentOutcome {
        let start = counter.read().unwrap();
        let out = {
            let mut seg = Segment {
                slot,
                counter: &counter,
                chain,
                config: &cfg,
                start_icount: start,
                injections: &[],
                timer: None,
                pause: &pause,
                sdk_events: None,
                hash_device_sections: Some(&|| {
                    let rail_ref = rail.borrow();
                    runtime_hash_device_sections(&rail_ref.bus, &rail_ref.lapic)
                }),
            };
            let counter_ref = &counter;
            run_segment_with_epochs(
                &mut seg,
                Until::IcountBudget(QUANTUM),
                &mut || false,
                &mut |exit: VcpuExit| {
                    let icount = counter_ref
                        .read()
                        .map_err(|e| BoundaryError::Exit(format!("{e:?}")))?;
                    rail.borrow_mut().service_exit(icount, exit)
                },
                &mut |idx, boundary, value, _slot| {
                    rail.borrow_mut()
                        .log_epoch_hash(idx, boundary.icount, value)
                        .map_err(|e| BoundaryError::Exit(format!("epoch log: {e:?}")))
                },
            )
            .unwrap()
        };
        assert_eq!(out.reason, StopReason::BudgetReached);
        out
    };

    let mut last = None;
    for i in 1..=seconds {
        let out = run_one(&mut slot, &mut chain);
        assert_eq!(
            out.boundary.icount,
            i * QUANTUM,
            "quantum {i} must land exactly on the grid"
        );
        if i < seconds {
            // The script: pad i lands at the end-of-second-i boundary.
            // frame_hint = i is not independently asserted here — it
            // matters because the reseal hammer round-trips it
            // byte-identically through record and replay.
            rail.borrow_mut()
                .apply_pad_set(
                    out.boundary.icount,
                    out.boundary.rip,
                    0,
                    script[(i - 1) as usize],
                    i as u32,
                )
                .unwrap();
        }
        last = Some(out);
    }
    let last = last.expect("at least one quantum");

    // The 60-guest-second identity: vns is exact, not approximate.
    assert_eq!(
        last.vns,
        seconds * VNS_PER_SECOND,
        "the run must end at exactly {seconds} guest-seconds of vns"
    );
    assert!(rail.borrow().irqs.is_empty(), "pad vector disabled");
    let end_state_hash = last.state_hash;
    let log = rail.into_inner().seal(&last, [0; 32]).expect("seal");

    // Sanity-parse the sealed log: the full epoch grid and the full
    // script landed.
    let r = LogReader::parse(&log).expect("parse own log");
    assert!(r.header().has_epoch_hashes());
    let epochs = r
        .aux()
        .filter(|rec| matches!(rec.body(), RecordBody::EpochHash { .. }))
        .count() as u64;
    assert_eq!(epochs, seconds, "one EPOCH_HASH per guest-second");
    let pads: Vec<u32> = r
        .canonical()
        .filter_map(|rec| match rec.body() {
            RecordBody::PadSet { buttons, .. } => Some(buttons),
            _ => None,
        })
        .collect();
    assert_eq!(pads, script, "the recorded sequence IS the script");
    let (_, log_end_hash) = r.end();
    assert_eq!(log_end_hash, end_state_hash);

    Recording {
        snapshot_ref: snap,
        log,
        cfg,
        end_state_hash,
        root_nonzero_pages,
        root_dhsnap,
    }
}

/// One replay leg on a fresh slot: full verification happens inside
/// `replay_segment` (epoch links at the link point, end_vns,
/// end_state_hash, reseal); the assertions here pin the counts and the
/// byte-identity to THIS recording. Returns the slot SO THE CALLER CAN
/// READ THE GUEST OBSERVATION TABLE (`assert_table_eras`) — do not
/// simplify the return to `()`.
fn replay_once(
    sys: &KvmSystem,
    counter: &InstRetired,
    store: &snapstore_client::blocking::SnapstoreClient,
    rec: &Recording,
    seconds: u64,
) -> SlotVm {
    let mut slot = sys.create_slot_vm(MEM).unwrap();
    let rail = DeviceRail::new(
        record_bus(),
        DetEntropy::from_seed([0; 32]), // replaced by the restored PRNG
        LogWriter::new(header_for(
            &rec.snapshot_ref,
            rec.cfg.config_hash().unwrap(),
        )),
        VmMem(slot.guest_mem.clone()),
    );

    let outcome = replay_segment(
        &mut slot,
        rail,
        &rec.cfg,
        rec.snapshot_ref.clone(),
        counter,
        store,
        &rec.log,
    )
    .expect("replay must not diverge");

    assert_eq!(outcome.records_applied, seconds - 1, "every scripted pad");
    assert_eq!(
        outcome.epoch_hashes_verified, seconds,
        "every EPOCH_HASH verified"
    );
    assert_eq!(outcome.end_icount, seconds * QUANTUM);
    assert_eq!(outcome.end_state_hash, rec.end_state_hash);
    // DELIBERATE wire-format lock: replay_segment already enforces the
    // reseal internally, so this adds no divergence-detection power —
    // but the M5 gate also pins DHILOG v1.0 bytes (the golden-fixtures
    // milestone sibling): a log-format change SHOULD redden this
    // acceptance and force a re-bless.
    assert_eq!(outcome.resealed, rec.log, "the reseal hammer");
    slot
}

/// The guest-side proof: pad-echo's RAM table observed the latch eras in
/// script order — consecutive-dedup of the per-frame pad0 column must be
/// exactly `[0 (boot latch), script...]` (exact because of pad_script's
/// checked invariant). The strong "every PAD_SET landed with the right
/// bytes" proof is the host-side `pads == script` compare in `record`;
/// this one proves the values reached the GUEST.
fn assert_table_eras(slot: &SlotVm, seconds: u64) {
    let mut head = [0u8; 8];
    slot.guest_mem
        .read_slice(&mut head, GuestAddress(nanokernel::PAD_ECHO_TABLE_GPA))
        .unwrap();
    let count = u64::from_le_bytes(head);
    assert!(count > 0, "guest recorded no frames");
    assert!(
        count < nanokernel::PAD_ECHO_TABLE_CAPACITY,
        "ring wrapped ({count} frames) — era reconstruction would be lossy"
    );
    let mut eras: Vec<u32> = Vec::new();
    for i in 0..count {
        let mut e = [0u8; 8];
        slot.guest_mem
            .read_slice(
                &mut e,
                GuestAddress(
                    nanokernel::PAD_ECHO_TABLE_GPA
                        + 8
                        + (i & (nanokernel::PAD_ECHO_TABLE_CAPACITY - 1))
                            * nanokernel::PAD_ECHO_ENTRY_BYTES,
                ),
            )
            .unwrap();
        let pad0 = u32::from_le_bytes(e[4..8].try_into().unwrap());
        if eras.last() != Some(&pad0) {
            eras.push(pad0);
        }
    }
    // Exact compare, no dedup on the expected side: pad_script's checked
    // invariant (no zeros, no adjacent-equal values) guarantees every
    // scripted value opens a distinct era.
    let mut expected = vec![0u32];
    expected.extend(pad_script(seconds));
    assert_eq!(eras, expected, "guest-observed latch eras == the script");
}

fn expected_corpus_text(rec: &Recording, root_sparse: &[u8]) -> String {
    let reader = LogReader::parse(&rec.log).expect("parse generated log");
    let header = reader.header();
    let cfg_hash = rec.cfg.config_hash().unwrap();
    let epochs: Vec<(u64, u64, [u8; 32])> = reader
        .aux()
        .filter_map(|rec| match rec.body() {
            RecordBody::EpochHash {
                epoch_index,
                chain_value,
            } => Some((epoch_index, rec.icount(), chain_value)),
            _ => None,
        })
        .collect();

    let mut out = String::new();
    out.push_str("# Pad-echo 6 guest-second record/replay corpus.\n");
    out.push_str(
        "# Re-baseline only with ci/determinism-class.lock or a reviewed hash-input contract change.\n",
    );
    out.push_str("name=pad_echo_6s\n");
    out.push_str(&format!("seconds={CORPUS_SECONDS}\n"));
    out.push_str(&format!("mem_bytes={MEM}\n"));
    out.push_str(&format!("page_size={PAGE_SIZE}\n"));
    out.push_str(&format!("nonzero_pages={}\n", rec.root_nonzero_pages.len()));
    out.push_str(&format!(
        "determinism_class_lock_blake3={}\n",
        blake3_hex(&determinism_class_lock_bytes())
    ));
    out.push_str(&format!(
        "snapshot_ref={}\n",
        hex(&rec.snapshot_ref.to_bytes())
    ));
    out.push_str(&format!("machine_config_hash={}\n", hex(&cfg_hash)));
    out.push_str(&format!("root_sparse_blake3={}\n", blake3_hex(root_sparse)));
    out.push_str(&format!(
        "root_dhsnap_blake3={}\n",
        blake3_hex(&rec.root_dhsnap)
    ));
    out.push_str(&format!("dhilog_blake3={}\n", blake3_hex(&rec.log)));
    out.push_str(&format!("dhilog_records={}\n", header.record_count));
    out.push_str(&format!("end_icount={}\n", header.end_icount));
    out.push_str(&format!("end_vns={}\n", header.end_vns));
    out.push_str(&format!("records_applied={}\n", CORPUS_SECONDS - 1));
    out.push_str(&format!("epoch_hashes_verified={}\n", epochs.len()));
    out.push_str(&format!("end_state_hash={}\n", hex(&header.end_state_hash)));
    for (epoch_index, icount, chain_value) in epochs {
        out.push_str(&format!(
            "epoch_{epoch_index}={icount}:{}\n",
            hex(&chain_value)
        ));
    }
    out
}

fn assert_log_matches_expected(log: &[u8], expected: &BTreeMap<String, String>) -> [u8; 32] {
    assert_eq!(blake3_hex(log), expected_value(expected, "dhilog_blake3"));
    let reader = LogReader::parse(log).expect("corpus log parses");
    let header = reader.header();
    assert!(header.has_epoch_hashes());
    assert_eq!(
        hex(&header.base_snapshot_id),
        expected_value(expected, "snapshot_ref")
    );
    assert_eq!(
        hex(&header.machine_config_hash),
        expected_value(expected, "machine_config_hash")
    );
    assert_eq!(u64::from(header.clock_num), u64::from(CLOCK_NUM));
    assert_eq!(u64::from(header.clock_den), 1);
    assert_eq!(
        header.record_count,
        expected_u64(expected, "dhilog_records")
    );
    assert_eq!(header.end_icount, expected_u64(expected, "end_icount"));
    assert_eq!(header.end_vns, expected_u64(expected, "end_vns"));
    assert_eq!(
        hex(&header.end_state_hash),
        expected_value(expected, "end_state_hash")
    );

    let epochs: Vec<(u64, u64, [u8; 32])> = reader
        .aux()
        .filter_map(|rec| match rec.body() {
            RecordBody::EpochHash {
                epoch_index,
                chain_value,
            } => Some((epoch_index, rec.icount(), chain_value)),
            _ => None,
        })
        .collect();
    assert_eq!(
        epochs.len() as u64,
        expected_u64(expected, "epoch_hashes_verified")
    );
    for (epoch_index, icount, chain_value) in epochs {
        assert_eq!(
            format!("{icount}:{}", hex(&chain_value)),
            expected_value(expected, &format!("epoch_{epoch_index}"))
        );
    }
    header.end_state_hash
}

fn load_corpus_snapshot(
    store: &snapstore_client::blocking::SnapstoreClient,
    expected: &BTreeMap<String, String>,
) -> SnapshotRef {
    let root_sparse = fs::read(fixture_path(CORPUS_ROOT_SPARSE)).expect("root sparse fixture");
    let root_dhsnap = fs::read(fixture_path(CORPUS_DHSNAP)).expect("root dhsnap fixture");
    assert_eq!(
        blake3_hex(&root_sparse),
        expected_value(expected, "root_sparse_blake3")
    );
    assert_eq!(
        blake3_hex(&root_dhsnap),
        expected_value(expected, "root_dhsnap_blake3")
    );
    let nonzero = decode_sparse_root(&root_sparse).expect("decode sparse root");
    assert_eq!(
        nonzero.len() as u64,
        expected_u64(expected, "nonzero_pages")
    );
    let snapshot_ref = store
        .put_snapshot_from_parts(
            None,
            MEM,
            expand_sparse_root(nonzero),
            DeviceBlob {
                format: DEVICE_BLOB_FORMAT_DHSNAP,
                zstd: false,
                raw_len: root_dhsnap.len() as u64,
                bytes: root_dhsnap,
            },
        )
        .expect("load corpus root snapshot");
    assert_eq!(
        hex(&snapshot_ref.to_bytes()),
        expected_value(expected, "snapshot_ref")
    );
    snapshot_ref
}

#[test]
fn record_replay_corpus_pad_echo_6s_reverifies() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let expected_bytes = fs::read(fixture_path(CORPUS_EXPECTED)).expect("expected corpus manifest");
    let expected = expected_map(&expected_bytes);
    assert_expected_key_set(&expected);
    assert_eq!(expected_value(&expected, "name"), "pad_echo_6s");
    assert_eq!(expected_u64(&expected, "seconds"), CORPUS_SECONDS);
    assert_eq!(expected_u64(&expected, "mem_bytes"), MEM);
    assert_eq!(expected_u64(&expected, "page_size"), PAGE_SIZE);
    assert_eq!(
        expected_value(&expected, "determinism_class_lock_blake3"),
        blake3_hex(&determinism_class_lock_bytes())
    );
    assert_eq!(
        expected_value(&expected, "machine_config_hash"),
        hex(&config().config_hash().unwrap())
    );

    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let snapshot_ref = load_corpus_snapshot(&store, &expected);
    assert_eq!(
        snapshot_ref,
        snapshot_ref_from_hex(expected_value(&expected, "snapshot_ref"))
    );

    let log = fs::read(fixture_path(CORPUS_DHILOG)).expect("dhilog fixture");
    let end_state_hash = assert_log_matches_expected(&log, &expected);
    let rec = Recording {
        snapshot_ref,
        log,
        cfg: config(),
        end_state_hash,
        root_nonzero_pages: Vec::new(),
        root_dhsnap: Vec::new(),
    };

    let sys = KvmSystem::open().unwrap();
    dh_vmm::run::install_kick_handler().unwrap();
    let counter = InstRetired::open_for_current_thread().unwrap();
    counter
        .route_overflow_to_thread(gettid(), dh_vmm::run::kick_signal())
        .unwrap();
    counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
    counter.reset().unwrap();
    counter.enable().unwrap();

    let slot = replay_once(
        &sys,
        &counter,
        &store,
        &rec,
        expected_u64(&expected, "records_applied") + 1,
    );
    assert_table_eras(&slot, CORPUS_SECONDS);
}

#[test]
#[ignore = "explicit re-baseline only: DH_WORKER_REGEN_RR_CORPUS=1 cargo test -p dh-worker --test m5_record_replay regenerate_record_replay_corpus_pad_echo_6s -- --ignored --nocapture"]
fn regenerate_record_replay_corpus_pad_echo_6s() {
    if std::env::var_os("DH_WORKER_REGEN_RR_CORPUS").is_none() {
        panic!("set DH_WORKER_REGEN_RR_CORPUS=1 to rewrite corpus fixtures");
    }
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let rec = record(&store, CORPUS_SECONDS);
    let root_sparse = encode_sparse_root(&rec.root_nonzero_pages);
    let expected = expected_corpus_text(&rec, &root_sparse);
    fs::create_dir_all(CORPUS_DIR).expect("create corpus dir");
    fs::write(fixture_path(CORPUS_ROOT_SPARSE), &root_sparse).expect("write root sparse");
    fs::write(fixture_path(CORPUS_DHSNAP), &rec.root_dhsnap).expect("write root dhsnap");
    fs::write(fixture_path(CORPUS_DHILOG), &rec.log).expect("write dhilog");
    fs::write(fixture_path(CORPUS_EXPECTED), expected).expect("write expected");
}

/// The every-sweep smoke: the identical rig at 6 guest-seconds x1 replay
/// keeps the non-1:1 clock path (end_vns) continuously covered.
#[test]
fn m5_smoke_record_replay_6s_vns_nonunit_clock() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let rec = record(&store, SMOKE_SECONDS);

    let sys = KvmSystem::open().unwrap();
    let counter = InstRetired::open_for_current_thread().unwrap();
    counter
        .route_overflow_to_thread(gettid(), dh_vmm::run::kick_signal())
        .unwrap();
    counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
    counter.reset().unwrap();
    counter.enable().unwrap();

    let slot = replay_once(&sys, &counter, &store, &rec, SMOKE_SECONDS);
    assert_table_eras(&slot, SMOKE_SECONDS);
}

/// THE M5 ACCEPTANCE (bead a5e): 60 guest-seconds of scripted pads,
/// replayed 100x from `(snapshot, DHILOG)` with zero divergence — every
/// replay reproduces end_state_hash, every EPOCH_HASH, end_vns, and the
/// exact log bytes. See the module docs for the invocation.
#[test]
#[ignore = "M5 acceptance gate: minutes of deliberate work — \
            cargo test -p dh-worker --test m5_record_replay --release -- --ignored --nocapture"]
fn m5_accept_record_replay_60s_vns_pad_sequence_x100() {
    if !kvm_available() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    let (_rt, _handle, store, _dir) = spawn_store_blocking();
    let rec = record(&store, ACCEPT_SECONDS);
    eprintln!(
        "recorded: {} bytes of DHILOG, end_state_hash {}",
        rec.log.len(),
        hex(&rec.end_state_hash)
    );

    let sys = KvmSystem::open().unwrap();
    let counter = InstRetired::open_for_current_thread().unwrap();
    counter
        .route_overflow_to_thread(gettid(), dh_vmm::run::kick_signal())
        .unwrap();
    counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
    counter.reset().unwrap();
    counter.enable().unwrap();

    for i in 1..=ACCEPT_REPLAYS {
        // A fresh slot per replay (the restore engine's precondition);
        // the counter is reset by each restore. The slot drops at scope
        // end — 100 concurrent 16 MiB guests would be pointless load.
        let slot = replay_once(&sys, &counter, &store, &rec, ACCEPT_SECONDS);
        if i == 1 || i == ACCEPT_REPLAYS {
            // The guest-visible script check: dispositive on the first
            // and last legs; the replay engine's reseal byte-identity
            // covers every leg in between.
            assert_table_eras(&slot, ACCEPT_SECONDS);
        }
        if i % 10 == 0 {
            eprintln!("replay {i}/{ACCEPT_REPLAYS}: zero divergence");
        }
    }
}
