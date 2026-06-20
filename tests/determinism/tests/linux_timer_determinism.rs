//! M9 Linux timer/IRQ determinism gate.
//!
//! Ignored by default: it needs externally supplied M9 Linux artifacts and
//! live KVM. Each case cold-boots the reference-workload fixture to Ready,
//! delivers a fixed run-control timer cadence in the post-READY workload, then
//! compares delivered icount/vector/source metadata and final state hash across
//! 100 cold boots.

#![cfg(target_arch = "x86_64")]

mod common;

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};
use dh_inputlog::dhilog::{LogWriter, SegmentHeader};
use dh_vmm::boundary::BoundaryError;
use dh_vmm::config::{BootSpec, CpuidLeaf, MachineConfig};
use dh_vmm::hash::StateHashChain;
use dh_vmm::kvm::ExitEvent;
use dh_vmm::runctl::{run_segment, Segment, StopReason, TimerFired, Until};
use kvm_ioctls::VcpuExit;

const TEST_NAME: &str = "linux_timer_determinism";
const ENTROPY_SEED: [u8; 32] = [0xA5; 32];
const BASE_SNAPSHOT_REF: [u8; 32] = [0; 32];
const CASES: usize = 100;
const TIMER_VECTOR: u8 = 0xF1;
const TIMER_TARGETS_AFTER_READY: [u64; 3] = [1_000_000, 2_000_000, 3_000_000];
const TAIL_BUDGET: u64 = 500_000;

#[derive(Clone, Debug)]
struct LinuxTimerSetup {
    bzimage: Vec<u8>,
    initramfs: Vec<u8>,
    game_image: PathBuf,
    config: MachineConfig,
    machine_config_hash: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TimerDelivery {
    source: TimerSource,
    vector: u8,
    armed_deadline_vns: u64,
    delivered_icount: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimerSource {
    RunControlTimer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TimerRun {
    ready_icount: u64,
    deliveries: Vec<TimerDelivery>,
    final_state_hash: [u8; 32],
}

#[test]
#[ignore = "M9 Linux artifact gate: requires DH_M9_* artifacts and KVM"]
fn linux_timer_irq_delivery_is_repeatable_across_100_cold_boots() -> common::TestResult<()> {
    let Some(artifacts) = common::m9_artifacts(TEST_NAME)? else {
        return Ok(());
    };
    let Some(sys) = common::m9_kvm_system(TEST_NAME)? else {
        return Ok(());
    };

    assert_no_in_kernel_irqchip_pit_or_ioapic(&sys)?;
    let setup = linux_setup(&sys, &artifacts)?;
    assert_no_host_time_timer_surface(&setup.config)?;

    let baseline = cold_boot_timer_run(&sys, &setup, 0)?;
    assert_eq!(
        baseline.deliveries.len(),
        TIMER_TARGETS_AFTER_READY.len(),
        "timer gate must record the full timer cadence"
    );
    for case in 1..CASES {
        let run = cold_boot_timer_run(&sys, &setup, case)?;
        assert_eq!(run, baseline, "Linux timer case {case} diverged");
    }
    let delivered: Vec<u64> = baseline
        .deliveries
        .iter()
        .map(|delivery| delivery.delivered_icount)
        .collect();
    eprintln!(
        "M9 Linux timer determinism: cases={} ready_icount={} vector={} delivered_icounts={:?} final_hash={}",
        CASES,
        baseline.ready_icount,
        TIMER_VECTOR,
        delivered,
        common::hex(&baseline.final_state_hash)
    );
    Ok(())
}

fn linux_setup(
    sys: &dh_vmm::kvm::KvmSystem,
    artifacts: &common::M9LinuxArtifacts,
) -> common::TestResult<LinuxTimerSetup> {
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
    Ok(LinuxTimerSetup {
        bzimage,
        initramfs,
        game_image: game_image_path,
        config,
        machine_config_hash,
    })
}

fn cold_boot_timer_run(
    sys: &dh_vmm::kvm::KvmSystem,
    setup: &LinuxTimerSetup,
    case: usize,
) -> common::TestResult<TimerRun> {
    let label = format!("case {case}");
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

    let ready = run_until_ready(
        &label, &mut slot, &counter, &rail, &pause, &mut chain, setup,
    )?;
    let mut current_icount = ready.boundary.icount;
    let mut deliveries = Vec::with_capacity(TIMER_TARGETS_AFTER_READY.len());
    for offset in TIMER_TARGETS_AFTER_READY {
        let target = ready
            .boundary
            .icount
            .checked_add(offset)
            .ok_or_else(|| format!("{label}: timer target overflow from Ready"))?;
        let fired = run_timer_segment(
            &label,
            &mut slot,
            &counter,
            &rail,
            &pause,
            &mut chain,
            setup,
            current_icount,
            target,
        )?;
        deliveries.push(fired);
        current_icount = target;
    }
    let final_target = current_icount
        .checked_add(TAIL_BUDGET)
        .ok_or_else(|| format!("{label}: final timer tail target overflow"))?;
    let final_state_hash = run_tail_segment(
        &label,
        &mut slot,
        &counter,
        &rail,
        &pause,
        &mut chain,
        setup,
        current_icount,
        final_target,
    )?;

    Ok(TimerRun {
        ready_icount: ready.boundary.icount,
        deliveries,
        final_state_hash,
    })
}

fn run_until_ready(
    label: &str,
    slot: &mut dh_vmm::kvm::SlotVm,
    counter: &InstRetired,
    rail: &RefCell<common::M9DeviceRail>,
    pause: &AtomicBool,
    chain: &mut StateHashChain,
    setup: &LinuxTimerSetup,
) -> common::TestResult<dh_vmm::runctl::SegmentOutcome> {
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

fn run_timer_segment(
    label: &str,
    slot: &mut dh_vmm::kvm::SlotVm,
    counter: &InstRetired,
    rail: &RefCell<common::M9DeviceRail>,
    pause: &AtomicBool,
    chain: &mut StateHashChain,
    setup: &LinuxTimerSetup,
    start_icount: u64,
    target: u64,
) -> common::TestResult<TimerDelivery> {
    let deadline_vns = setup
        .config
        .clock
        .vns_from_icount(target)
        .ok_or_else(|| format!("{label}: timer target {target} overflows vns conversion"))?;
    let outcome = run_timer_or_tail_segment(
        label,
        slot,
        counter,
        rail,
        pause,
        chain,
        setup,
        start_icount,
        target,
        Some(dh_vmm::runctl::TimerArm {
            deadline_vns,
            vector: TIMER_VECTOR,
        }),
    )?;
    if outcome.injections_delivered != 1 {
        return Err(format!(
            "{label}: timer target {target} delivered {} injections, expected 1",
            outcome.injections_delivered
        ));
    }
    let fired = outcome
        .timer_fired
        .ok_or_else(|| format!("{label}: timer target {target} did not fire"))?;
    timer_delivery(label, target, deadline_vns, fired)
}

fn run_tail_segment(
    label: &str,
    slot: &mut dh_vmm::kvm::SlotVm,
    counter: &InstRetired,
    rail: &RefCell<common::M9DeviceRail>,
    pause: &AtomicBool,
    chain: &mut StateHashChain,
    setup: &LinuxTimerSetup,
    start_icount: u64,
    target: u64,
) -> common::TestResult<[u8; 32]> {
    let outcome = run_timer_or_tail_segment(
        label,
        slot,
        counter,
        rail,
        pause,
        chain,
        setup,
        start_icount,
        target,
        None,
    )?;
    if outcome.injections_delivered != 0 || outcome.timer_fired.is_some() {
        return Err(format!(
            "{label}: timer tail unexpectedly delivered an injection"
        ));
    }
    Ok(outcome.state_hash)
}

#[allow(clippy::too_many_arguments)]
fn run_timer_or_tail_segment(
    label: &str,
    slot: &mut dh_vmm::kvm::SlotVm,
    counter: &InstRetired,
    rail: &RefCell<common::M9DeviceRail>,
    pause: &AtomicBool,
    chain: &mut StateHashChain,
    setup: &LinuxTimerSetup,
    start_icount: u64,
    target: u64,
    timer: Option<dh_vmm::runctl::TimerArm>,
) -> common::TestResult<dh_vmm::runctl::SegmentOutcome> {
    let mut on_exit = |exit: VcpuExit<'_>| {
        let icount = counter
            .read()
            .map_err(|e| BoundaryError::Exit(format!("{label}: counter read: {e:?}")))?;
        common::m9_service_exit_with_detchannel(&mut rail.borrow_mut(), icount, exit)?;
        Ok(())
    };
    let hash_device_sections = || common::m9_runtime_hash_device_sections(rail);
    let mut segment = Segment {
        slot,
        counter,
        chain,
        config: &setup.config,
        start_icount,
        injections: &[],
        timer,
        pause,
        sdk_events: None,
        hash_device_sections: Some(&hash_device_sections),
    };
    let budget = target.checked_sub(start_icount).ok_or_else(|| {
        format!("{label}: segment target {target} is before start {start_icount}")
    })?;
    let outcome = run_segment(
        &mut segment,
        Until::IcountBudget(budget),
        &mut || false,
        &mut on_exit,
    )
    .map_err(|e| format!("{label}: timer segment {start_icount}->{target}: {e}"))?;
    if outcome.reason != StopReason::BudgetReached {
        return Err(format!(
            "{label}: timer segment {start_icount}->{target} stopped {:?}",
            outcome.reason
        ));
    }
    if outcome.boundary.icount != target {
        return Err(format!(
            "{label}: timer segment target {target} landed at {}",
            outcome.boundary.icount
        ));
    }
    Ok(outcome)
}

fn timer_delivery(
    label: &str,
    target: u64,
    deadline_vns: u64,
    fired: TimerFired,
) -> common::TestResult<TimerDelivery> {
    if fired.vector != TIMER_VECTOR {
        return Err(format!(
            "{label}: timer vector {}, expected {TIMER_VECTOR}",
            fired.vector
        ));
    }
    if fired.armed_deadline_vns != deadline_vns {
        return Err(format!(
            "{label}: timer armed_deadline_vns {}, expected {deadline_vns}",
            fired.armed_deadline_vns
        ));
    }
    if fired.delivered_icount != target {
        return Err(format!(
            "{label}: timer delivered at {}, expected selected target {target}",
            fired.delivered_icount
        ));
    }
    Ok(TimerDelivery {
        source: TimerSource::RunControlTimer,
        vector: fired.vector,
        armed_deadline_vns: fired.armed_deadline_vns,
        delivered_icount: fired.delivered_icount,
    })
}

fn assert_no_host_time_timer_surface(config: &MachineConfig) -> common::TestResult<()> {
    if config
        .cpuid_table
        .iter()
        .any(|leaf| (0x4000_0000..0x4000_0100).contains(&leaf.function))
    {
        return Err("masked CPUID advertised KVM paravirt/kvmclock leaves".into());
    }
    for leaf in &config.cpuid_table {
        assert_no_forbidden_cpuid_bits(*leaf)?;
    }
    Ok(())
}

fn assert_no_forbidden_cpuid_bits(leaf: CpuidLeaf) -> common::TestResult<()> {
    const L1_ECX_TSC_DEADLINE: u32 = 1 << 24;
    const L1_ECX_X2APIC: u32 = 1 << 21;
    if leaf.function == 1 {
        if leaf.ecx & L1_ECX_TSC_DEADLINE != 0 {
            return Err("masked CPUID advertised TSC-deadline timer".into());
        }
        if leaf.ecx & L1_ECX_X2APIC != 0 {
            return Err("masked CPUID advertised x2APIC without in-kernel irqchip".into());
        }
    }
    Ok(())
}

fn assert_no_in_kernel_irqchip_pit_or_ioapic(
    sys: &dh_vmm::kvm::KvmSystem,
) -> common::TestResult<()> {
    let mut slot = sys
        .create_slot_vm(2 * 1024 * 1024)
        .map_err(|e| format!("forbidden timer surface probe VM: {e:?}"))?;
    use vm_memory::{Bytes, GuestAddress};
    slot.guest_mem
        .write_slice(&[0xF4], GuestAddress(0))
        .map_err(|e| format!("write HLT probe guest code: {e}"))?;
    let mut sregs = slot
        .vcpu
        .get_sregs()
        .map_err(|e| format!("HLT probe get_sregs: {e}"))?;
    sregs.cs.base = 0;
    sregs.cs.selector = 0;
    slot.vcpu
        .set_sregs(&sregs)
        .map_err(|e| format!("HLT probe set_sregs: {e}"))?;
    let mut regs = slot
        .vcpu
        .get_regs()
        .map_err(|e| format!("HLT probe get_regs: {e}"))?;
    regs.rip = 0;
    regs.rflags = 2;
    slot.vcpu
        .set_regs(&regs)
        .map_err(|e| format!("HLT probe set_regs: {e}"))?;
    let exit = slot
        .vcpu
        .run()
        .map_err(|e| format!("HLT probe KVM_RUN: {e}"))?;
    let classified = dh_vmm::kvm::classify_exit(exit);
    if classified != ExitEvent::Hlt {
        return Err(format!(
            "HLT probe did not exit to userspace; forbidden in-kernel irqchip/PIT/IOAPIC surface may exist: {classified:?}"
        ));
    }
    Ok(())
}
