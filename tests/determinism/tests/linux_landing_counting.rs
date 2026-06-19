//! M9 Linux post-READY landing/counting gate.
//!
//! Ignored by default: it needs externally supplied M9 Linux artifacts and
//! live KVM. The test boots the reference-workload fixture to Ready, then lands
//! at 100 post-READY absolute retired-instruction targets and compares the
//! boundary/hash sequence across two cold boots.

#![cfg(target_arch = "x86_64")]

mod common;

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};
use dh_inputlog::dhilog::{LogWriter, SegmentHeader};
use dh_vmm::boundary::{land_at, Boundary, BoundaryError, Margins};
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::hash::StateHashChain;
use dh_vmm::runctl::{run_segment, Segment, SegmentOutcome, StopReason, Until};
use kvm_ioctls::VcpuExit;

const TEST_NAME: &str = "linux_landing_counting";
const ENTROPY_SEED: [u8; 32] = [0x9A; 32];
const BASE_SNAPSHOT_REF: [u8; 32] = [0; 32];
const LANDING_TARGETS: usize = 100;
const TARGET_FLOOR_AFTER_READY: u64 = 1_000_000;
const TARGET_STRIDE: u64 = 500_000;

#[derive(Clone, Debug)]
struct LinuxLandingSetup {
    bzimage: Vec<u8>,
    initramfs: Vec<u8>,
    game_image: PathBuf,
    config: MachineConfig,
    machine_config_hash: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LandingSample {
    icount: u64,
    rip: u64,
    rcx: u64,
    state_hash: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LandingRun {
    ready_icount: u64,
    samples: Vec<LandingSample>,
}

#[test]
#[ignore = "M9 Linux artifact gate: requires DH_M9_* artifacts and KVM"]
fn linux_post_ready_landings_are_exact_and_repeatable() -> common::TestResult<()> {
    let Some(artifacts) = common::m9_artifacts(TEST_NAME)? else {
        return Ok(());
    };
    let Some(sys) = common::m9_kvm_system(TEST_NAME)? else {
        return Ok(());
    };

    let setup = linux_setup(&sys, &artifacts)?;
    let first = cold_boot_landings(&sys, &setup, None, "first")?;
    let targets = landing_targets(first.ready_icount);
    let second = cold_boot_landings(&sys, &setup, Some(&targets), "second")?;
    assert_eq!(
        first, second,
        "Linux post-Ready landing sequence diverged across cold boots"
    );
    assert_eq!(
        first.samples.len(),
        LANDING_TARGETS,
        "landing gate must execute every requested target"
    );
    for (sample, target) in first.samples.iter().zip(targets.iter()) {
        assert_eq!(
            sample.icount, *target,
            "landed icount must equal the target exactly"
        );
    }
    assert_eq!(
        targets.first().copied(),
        Some(first.ready_icount + TARGET_FLOOR_AFTER_READY),
        "first target must start after the post-Ready landing warmup floor"
    );
    eprintln!(
        "M9 Linux landing/counting: ready_icount={} targets={} first_hash={} last_hash={}",
        first.ready_icount,
        first.samples.len(),
        common::hex(&first.samples.first().expect("nonempty samples").state_hash),
        common::hex(&first.samples.last().expect("nonempty samples").state_hash)
    );
    Ok(())
}

fn linux_setup(
    sys: &dh_vmm::kvm::KvmSystem,
    artifacts: &common::M9LinuxArtifacts,
) -> common::TestResult<LinuxLandingSetup> {
    let hashes = common::populate_m9_image_cache(artifacts)?;
    let bzimage_path = common::m9_cache_entry(&artifacts.image_cache, &hashes.bzimage);
    let initramfs_path = common::m9_cache_entry(&artifacts.image_cache, &hashes.initramfs);
    let game_image_path = common::m9_cache_entry(&artifacts.image_cache, &hashes.game_image);
    let config = common::m9_linux_machine_config(
        &hashes,
        sys.masked_cpuid_table()
            .map_err(|e| format!("{TEST_NAME}: masked CPUID table: {e:?}"))?,
    );
    let machine_config_hash = config
        .config_hash()
        .map_err(|e| format!("{TEST_NAME}: MachineConfig hash: {e:?}"))?;
    let bzimage = std::fs::read(&bzimage_path)
        .map_err(|e| format!("read cached BzImage {}: {e}", bzimage_path.display()))?;
    if common::hash_bytes(&bzimage) != hashes.bzimage {
        return Err(format!(
            "cached BzImage {} no longer matches MachineConfig hash",
            bzimage_path.display()
        ));
    }
    let initramfs = std::fs::read(&initramfs_path)
        .map_err(|e| format!("read cached initramfs {}: {e}", initramfs_path.display()))?;
    if common::hash_bytes(&initramfs) != hashes.initramfs {
        return Err(format!(
            "cached initramfs {} no longer matches MachineConfig hash",
            initramfs_path.display()
        ));
    }
    if common::hash_file(&game_image_path)? != hashes.game_image {
        return Err(format!(
            "cached game image {} no longer matches MachineConfig hash",
            game_image_path.display()
        ));
    }
    Ok(LinuxLandingSetup {
        bzimage,
        initramfs,
        game_image: game_image_path,
        config,
        machine_config_hash,
    })
}

fn cold_boot_landings(
    sys: &dh_vmm::kvm::KvmSystem,
    setup: &LinuxLandingSetup,
    expected_targets: Option<&[u64]>,
    label: &str,
) -> common::TestResult<LandingRun> {
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
    let base_image = dh_vmm::blkfile::FileBase::open(&setup.game_image)
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
    let pause = AtomicBool::new(false);
    let mut chain = StateHashChain::new(&setup.machine_config_hash, &BASE_SNAPSHOT_REF);

    let ready = run_until_ready(label, &mut slot, &counter, &rail, &pause, &mut chain, setup)?;
    let targets = expected_targets
        .map(|targets| targets.to_vec())
        .unwrap_or_else(|| landing_targets(ready.boundary.icount));
    if targets.len() != LANDING_TARGETS {
        return Err(format!(
            "{label}: target count {}, expected {LANDING_TARGETS}",
            targets.len()
        ));
    }
    if targets
        .first()
        .is_none_or(|target| *target <= ready.boundary.icount)
    {
        return Err(format!(
            "{label}: landing targets must start after Ready icount {}",
            ready.boundary.icount
        ));
    }

    let samples = land_targets(
        label, &mut slot, &counter, &rail, &mut chain, setup, &targets,
    )?;
    Ok(LandingRun {
        ready_icount: ready.boundary.icount,
        samples,
    })
}

fn run_until_ready(
    label: &str,
    slot: &mut dh_vmm::kvm::SlotVm,
    counter: &InstRetired,
    rail: &RefCell<common::M9DeviceRail>,
    pause: &AtomicBool,
    chain: &mut StateHashChain,
    setup: &LinuxLandingSetup,
) -> common::TestResult<SegmentOutcome> {
    let sdk_event_feed = Cell::new(0u64);
    let ready_stream = detguest_wire::record::EventKind::Ready as u32;
    let mut ready_count = 0u64;
    let mut on_exit = |exit: VcpuExit<'_>| {
        let icount = counter.read().map_err(|e| {
            dh_vmm::boundary::BoundaryError::Exit(format!("{label}: counter read: {e:?}"))
        })?;
        let events = common::m9_service_exit_with_detchannel(&mut rail.borrow_mut(), icount, exit)?;
        for event in events {
            if event.stream == ready_stream {
                ready_count += 1;
                sdk_event_feed.set(sdk_event_feed.get() + 1);
            }
        }
        Ok(())
    };
    let hash_device_sections = || common::m9_runtime_hash_device_sections(rail);
    let mut segment = Segment {
        slot,
        counter,
        chain,
        config: &setup.config,
        start_icount: 0,
        injections: &[],
        timer: None,
        pause,
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
            "{label}: Ready was not observed before hard cap; reason={:?} icount={}",
            outcome.reason, outcome.boundary.icount
        ));
    }
    if ready_count != 1 {
        return Err(format!(
            "{label}: expected exactly one Ready event, saw {ready_count}"
        ));
    }
    Ok(outcome)
}

fn land_targets(
    label: &str,
    slot: &mut dh_vmm::kvm::SlotVm,
    counter: &InstRetired,
    rail: &RefCell<common::M9DeviceRail>,
    chain: &mut StateHashChain,
    setup: &LinuxLandingSetup,
    targets: &[u64],
) -> common::TestResult<Vec<LandingSample>> {
    let mut samples = Vec::with_capacity(targets.len());
    for (index, target) in targets.iter().copied().enumerate() {
        let boundary = land_at(
            &mut slot.vcpu,
            counter,
            target,
            &landing_margins(index),
            &mut |exit: VcpuExit<'_>| {
                let icount = counter
                    .read()
                    .map_err(|e| BoundaryError::Exit(format!("{label}: counter read: {e:?}")))?;
                common::m9_service_exit_with_detchannel(&mut rail.borrow_mut(), icount, exit)?;
                Ok(())
            },
        )
        .map_err(|e| format!("{label}: target {target}: {e}"))?;
        if boundary.icount != target {
            return Err(format!(
                "{label}: target {target} landed at {}",
                boundary.icount
            ));
        }
        let state_hash = push_landing_hash(chain, slot, rail, &setup.config, boundary)?;
        samples.push(LandingSample {
            icount: boundary.icount,
            rip: boundary.rip,
            rcx: boundary.rcx,
            state_hash,
        });
    }
    Ok(samples)
}

fn push_landing_hash(
    chain: &mut StateHashChain,
    slot: &dh_vmm::kvm::SlotVm,
    rail: &RefCell<common::M9DeviceRail>,
    config: &MachineConfig,
    boundary: Boundary,
) -> common::TestResult<[u8; 32]> {
    let vns = config
        .clock
        .vns_from_icount(boundary.icount)
        .ok_or_else(|| format!("vns conversion overflow at icount {}", boundary.icount))?;
    let device_sections = common::m9_runtime_hash_device_sections(rail);
    chain
        .push_final_link(slot, &device_sections, boundary.icount, vns)
        .map_err(|e| format!("state hash at icount {}: {e:?}", boundary.icount))?;
    Ok(chain.value())
}

fn landing_targets(ready_icount: u64) -> Vec<u64> {
    (0..LANDING_TARGETS)
        .map(|index| ready_icount + TARGET_FLOOR_AFTER_READY + index as u64 * TARGET_STRIDE)
        .collect()
}

fn landing_margins(index: usize) -> Margins {
    if index < 10 {
        Margins::default()
    } else {
        Margins {
            skid_margin: 512,
            resync_slack: 512,
        }
    }
}
