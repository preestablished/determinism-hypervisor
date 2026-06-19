//! M9 Linux boot-to-READY determinism gate.
//!
//! Ignored by default: it needs externally supplied M9 Linux artifacts and
//! live KVM. The final gate intentionally fails loud when `DH_M9_ALLOW_SKIP=0`
//! and any artifact or KVM prerequisite is missing.

#![cfg(target_arch = "x86_64")]

mod common;

use std::cell::{Cell, RefCell};
use std::sync::atomic::AtomicBool;

use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};
use dh_inputlog::dhilog::{LogWriter, SegmentHeader};
use dh_inputlog::reader::{LogReader, RecordBody};
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::hash::StateHashChain;
use dh_vmm::runctl::{run_segment, Segment, StopReason, Until};
use kvm_ioctls::VcpuExit;

const TEST_NAME: &str = "linux_boot_to_ready_determinism";
const ENTROPY_SEED: [u8; 32] = [0x9A; 32];
const BASE_SNAPSHOT_REF: [u8; 32] = [0; 32];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ReadyPayload {
    unit: u32,
    region_count: u32,
    manifest_generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReadyIdentity {
    ready_icount: u64,
    ready_payload: ReadyPayload,
    machine_config_hash: [u8; 32],
    state_hash: [u8; 32],
}

struct LinuxReadySetup {
    artifacts: common::M9LinuxArtifacts,
    bzimage: Vec<u8>,
    initramfs: Vec<u8>,
    config: MachineConfig,
    machine_config_hash: [u8; 32],
}

#[test]
#[ignore = "M9 Linux artifact gate: requires DH_M9_* artifacts and KVM"]
fn linux_boot_to_ready_identity_is_deterministic() -> common::TestResult<()> {
    let Some(artifacts) = common::m9_artifacts(TEST_NAME)? else {
        return Ok(());
    };
    let Some(sys) = common::m9_kvm_system(TEST_NAME)? else {
        return Ok(());
    };

    let hashes = common::populate_m9_image_cache(&artifacts)?;
    assert_eq!(
        hashes.base_image,
        common::hash_file(&artifacts.base_image)?,
        "DH_M9_BASE_IMAGE must be readable fixture context"
    );
    let config = common::m9_linux_machine_config(
        &hashes,
        sys.masked_cpuid_table()
            .map_err(|e| format!("{TEST_NAME}: masked CPUID table: {e:?}"))?,
    );
    let machine_config_hash = config
        .config_hash()
        .map_err(|e| format!("{TEST_NAME}: MachineConfig hash: {e:?}"))?;
    let setup = LinuxReadySetup {
        bzimage: std::fs::read(&artifacts.bzimage)
            .map_err(|e| format!("read {}: {e}", artifacts.bzimage.display()))?,
        initramfs: std::fs::read(&artifacts.initramfs)
            .map_err(|e| format!("read {}: {e}", artifacts.initramfs.display()))?,
        artifacts,
        config,
        machine_config_hash,
    };

    let first = cold_boot_to_ready(&sys, &setup, "first")?;
    let second = cold_boot_to_ready(&sys, &setup, "second")?;
    assert_eq!(
        first, second,
        "Linux boot-to-Ready identity diverged across cold boots"
    );
    eprintln!(
        "M9 Linux Ready identity: ready_icount={} unit={} region_count={} manifest_generation={} machine_config_hash={} state_hash={}",
        first.ready_icount,
        first.ready_payload.unit,
        first.ready_payload.region_count,
        first.ready_payload.manifest_generation,
        common::hex(&first.machine_config_hash),
        common::hex(&first.state_hash)
    );

    Ok(())
}

fn cold_boot_to_ready(
    sys: &dh_vmm::kvm::KvmSystem,
    setup: &LinuxReadySetup,
    label: &str,
) -> common::TestResult<ReadyIdentity> {
    let mut slot = sys
        .create_slot_vm(common::M9_LINUX_MEM_BYTES)
        .map_err(|e| format!("{label}: create Linux slot: {e:?}"))?;
    let cmdline = match &setup.config.boot {
        BootSpec::BzImage { cmdline, .. } => cmdline.as_slice(),
        BootSpec::Elf { .. } => return Err("M9 Linux config unexpectedly used ELF boot".into()),
    };
    dh_vmm::boot::load_bzimage_and_enter(&slot, &setup.bzimage, &setup.initramfs, cmdline)
        .map_err(|e| format!("{label}: BzImage boot: {e}"))?;

    dh_vmm::run::install_kick_handler().map_err(|e| format!("{label}: kick handler: {e}"))?;
    let counter =
        InstRetired::open_for_current_thread().map_err(|e| format!("{label}: counter: {e:?}"))?;
    counter
        .route_overflow_to_thread(dh_vmm::run::current_tid(), dh_vmm::run::kick_signal())
        .map_err(|e| format!("{label}: route counter overflow: {e:?}"))?;
    counter
        .reset()
        .map_err(|e| format!("{label}: reset counter: {e:?}"))?;
    counter
        .arm_period(NEVER_FIRES_PERIOD)
        .map_err(|e| format!("{label}: arm counter: {e:?}"))?;
    counter
        .enable()
        .map_err(|e| format!("{label}: enable counter: {e:?}"))?;

    let mem = common::M9VmMem(slot.guest_mem.clone());
    let base_image = dh_vmm::blkfile::FileBase::open(&setup.artifacts.game_image)
        .map_err(|e| format!("{label}: open DH_M9_GAME_IMAGE: {e}"))?;
    let bus = common::m9_linux_bus(&setup.config, base_image, mem.clone())?;
    let log = LogWriter::new(SegmentHeader {
        base_snapshot_id: BASE_SNAPSHOT_REF,
        entropy_seed: ENTROPY_SEED,
        machine_config_hash: setup.machine_config_hash,
        clock_num: setup.config.clock.num(),
        clock_den: setup.config.clock.den(),
        encoder_fingerprint: dh_devices::detchannel::wire_encoder_fingerprint(),
    });
    let rail = RefCell::new(common::M9DeviceRail::new(
        bus,
        dh_devices::entropy::DetEntropy::from_seed(ENTROPY_SEED),
        log,
        mem,
    ));
    let sdk_event_feed = Cell::new(0u64);
    let ready_event = RefCell::new(None::<common::M9GuestEvent>);
    let ready_stream = detguest_wire::record::EventKind::Ready as u32;
    let mut on_exit = |exit: VcpuExit<'_>| {
        let icount = counter.read().map_err(|e| {
            dh_vmm::boundary::BoundaryError::Exit(format!("{label}: counter read: {e:?}"))
        })?;
        let events = common::m9_service_exit_with_detchannel(&mut rail.borrow_mut(), icount, exit)?;
        for event in events {
            if event.stream == ready_stream {
                sdk_event_feed.set(sdk_event_feed.get() + 1);
                if ready_event.borrow().is_none() {
                    *ready_event.borrow_mut() = Some(event.clone());
                }
            }
        }
        Ok(())
    };
    let hash_device_sections = || common::m9_runtime_hash_device_sections(&rail);
    let pause = AtomicBool::new(false);
    let mut chain = StateHashChain::new(&setup.machine_config_hash, &BASE_SNAPSHOT_REF);
    let mut segment = Segment {
        slot: &mut slot,
        counter: &counter,
        chain: &mut chain,
        config: &setup.config,
        start_icount: 0,
        injections: &[],
        timer: None,
        pause: &pause,
        sdk_events: Some(&sdk_event_feed),
        hash_device_sections: Some(&hash_device_sections),
    };
    let outcome = run_segment(
        &mut segment,
        Until::NextSdkEvent {
            hard_cap: common::M9_READY_HARD_CAP,
        },
        &mut || false,
        &mut on_exit,
    )
    .map_err(|e| format!("{label}: Run until Ready: {e}"))?;
    if outcome.reason != StopReason::NextSdkEvent {
        return Err(format!(
            "{label}: only serial output or no Ready observed before hard cap; reason={:?} icount={}",
            outcome.reason, outcome.boundary.icount
        ));
    }
    let ready_event = ready_event
        .into_inner()
        .ok_or_else(|| format!("{label}: NextSdkEvent stop had no Ready event"))?;
    if ready_event.icount != outcome.boundary.icount {
        return Err(format!(
            "{label}: Ready event icount {} != run boundary {}",
            ready_event.icount, outcome.boundary.icount
        ));
    }
    let ready_payload = parse_ready_payload(&ready_event)?;

    let rail = rail.into_inner();
    let sealed_log = rail
        .seal(&outcome, [0; 32])
        .map_err(|e| format!("{label}: seal Ready input log: {e:?}"))?;
    assert_no_host_input_before_ready(&sealed_log, ready_event.icount)?;

    Ok(ReadyIdentity {
        ready_icount: ready_event.icount,
        ready_payload,
        machine_config_hash: setup.machine_config_hash,
        state_hash: outcome.state_hash,
    })
}

fn parse_ready_payload(event: &common::M9GuestEvent) -> common::TestResult<ReadyPayload> {
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

fn assert_no_host_input_before_ready(log: &[u8], ready_icount: u64) -> common::TestResult<()> {
    let reader = LogReader::parse(log).map_err(|e| format!("DHILOG parse: {e:?}"))?;
    for rec in reader
        .canonical()
        .filter(|rec| rec.icount() <= ready_icount)
    {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ready_event(payload: Vec<u8>) -> common::M9GuestEvent {
        common::M9GuestEvent {
            stream: detguest_wire::record::EventKind::Ready as u32,
            icount: 7,
            vns: 7,
            payload,
        }
    }

    #[test]
    fn ready_payload_parses_identity_fields() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&3u32.to_le_bytes());
        payload.extend_from_slice(&4u32.to_le_bytes());
        payload.extend_from_slice(&8u64.to_le_bytes());

        assert_eq!(
            parse_ready_payload(&test_ready_event(payload)).unwrap(),
            ReadyPayload {
                unit: 3,
                region_count: 4,
                manifest_generation: 8,
            }
        );
    }

    #[test]
    fn ready_payload_rejects_serial_or_wrong_shape() {
        let serial = common::M9GuestEvent {
            stream: 0,
            icount: 1,
            vns: 1,
            payload: vec![0; 16],
        };
        assert!(parse_ready_payload(&serial).unwrap_err().contains("Ready"));
        assert!(parse_ready_payload(&test_ready_event(vec![0; 8]))
            .unwrap_err()
            .contains("16 bytes"));
    }
}
