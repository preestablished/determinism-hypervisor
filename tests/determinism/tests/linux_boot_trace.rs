//! M9 Linux bzImage entry smoke. This is ignored by default because it needs
//! externally supplied Linux artifacts and live KVM.

#![cfg(target_arch = "x86_64")]

#[allow(dead_code)]
mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use dh_detclock::counter::InstRetired;
use dh_vmm::config::canonicalize_bzimage_cmdline_extras;
use dh_vmm::kvm::{ExitEvent, KvmSystem};
use dh_vmm::msr::{DeniedMsrClass, MSR_IA32_APIC_BASE, denied_msr_class};
use kvm_bindings::KVM_MAX_CPUID_ENTRIES;
use kvm_ioctls::VcpuExit;
use vm_memory::{Bytes, GuestAddress};

const M9_LINUX_MEM_BYTES: u64 = 512 * 1024 * 1024;
const LINUX_ENTRY_OFFSET: u64 = 0x200;
const TRACE_OUTPUT: &str = "target/m9/linux_boot_trace.json";
const TRACE_BOOT_ENV: &str = "DH_M9_TRACE_BOOT";
const TRACE_EXIT_LIMIT_ENV: &str = "DH_M9_TRACE_EXIT_LIMIT";
const TRACE_ICOUNT_LIMIT_ENV: &str = "DH_M9_TRACE_ICOUNT_LIMIT";
const DEFAULT_TRACE_EXIT_LIMIT: u64 = 4096;
const DEFAULT_TRACE_ICOUNT_LIMIT: u64 = 1_000_000_000;
const SMOKE_TRACE_EXIT_LIMIT: u64 = 1;

#[test]
#[ignore]
fn linux_entry_smoke() {
    if !common::kvm_usable() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }

    let artifacts = common::M9LinuxArtifacts::from_env_required("linux_entry_smoke")
        .expect("M9 Linux artifacts");
    let bzimage = std::fs::read(&artifacts.bzimage).expect("read DH_M9_BZIMAGE");
    let initramfs = std::fs::read(&artifacts.initramfs).expect("read DH_M9_INITRAMFS");
    let cmdline = canonicalize_bzimage_cmdline_extras(b"quiet").expect("canonical cmdline");

    let sys = KvmSystem::open().expect("KVM gate");
    let mut slot = sys
        .create_slot_vm(M9_LINUX_MEM_BYTES)
        .expect("create Linux entry slot");
    let layout = dh_vmm::boot::load_bzimage_and_enter(&slot, &bzimage, &initramfs, &cmdline)
        .expect("load bzImage and program entry");

    let regs = slot.vcpu.get_regs().expect("entry regs");
    assert_eq!(
        regs.rip,
        dh_vmm::boot::linux_bzimage::LINUX_KERNEL_LOAD_GPA + LINUX_ENTRY_OFFSET
    );
    assert_eq!(regs.rsi, dh_vmm::boot::linux_bzimage::LINUX_BOOT_PARAMS_GPA);
    assert_eq!(regs.rflags, 2);

    let sregs = slot.vcpu.get_sregs().expect("entry sregs");
    assert_eq!(sregs.cs.selector, 0x10, "Linux __BOOT_CS");
    assert_eq!(sregs.ds.selector, 0x18, "Linux __BOOT_DS");
    assert_ne!(sregs.cr0 & (1 << 31), 0, "paging enabled");
    assert_ne!(sregs.cr0 & 1, 0, "protected mode enabled");
    assert_ne!(sregs.cr4 & (1 << 5), 0, "PAE enabled");
    assert_ne!(sregs.efer & (1 << 8), 0, "LME enabled");
    assert_ne!(sregs.efer & (1 << 10), 0, "LMA enabled");

    let mut boot_params_magic = [0u8; 4];
    slot.guest_mem
        .read_slice(
            &mut boot_params_magic,
            GuestAddress(dh_vmm::boot::linux_bzimage::LINUX_BOOT_PARAMS_GPA + 0x202),
        )
        .expect("read boot_params magic");
    assert_eq!(&boot_params_magic, b"HdrS");
    assert_eq!(
        layout.kernel_image.start,
        dh_vmm::boot::linux_bzimage::LINUX_KERNEL_LOAD_GPA
    );

    assert_masked_cpuid_surface(&slot);

    let limit = trace_exit_limit();
    let icount_limit = trace_icount_limit();
    let trace = trace_linux_boot(&mut slot, limit, icount_limit);
    let trace_path = trace_output_path();
    write_trace(&trace, &trace_path).expect("write linux boot trace");
    eprintln!(
        "Linux boot trace: {} exits, terminal={}, artifact={}",
        trace.total_exits,
        trace.terminal_reason.as_deref().unwrap_or("unknown"),
        trace_path.display(),
    );
    assert!(
        !fatal_before_serviceable_exit(&trace),
        "Linux entry failed before the first serviceable KVM exit: {}",
        trace.terminal_reason.as_deref().unwrap_or("unknown")
    );

    if trace_required() {
        assert!(
            trace_path.is_file(),
            "{TRACE_BOOT_ENV}=1 must produce {TRACE_OUTPUT}"
        );
        assert_trace_acceptance(&trace);
    }
}

fn fatal_before_serviceable_exit(trace: &LinuxBootTrace) -> bool {
    trace.total_exits == 1
        && trace.terminal_reason.as_deref().is_some_and(|reason| {
            reason == "shutdown" || reason == "internal_error" || reason.starts_with("fail_entry(")
        })
}

fn assert_trace_acceptance(trace: &LinuxBootTrace) {
    assert!(
        trace.unclassified_denied_msr_indices.is_empty(),
        "unclassified denied MSRs remain before READY: {:?}",
        trace.unclassified_denied_msr_indices
    );
    assert!(
        trace.unclassified_mmio_addresses.is_empty(),
        "unclassified MMIO exits remain before READY: {:?}",
        trace.unclassified_mmio_addresses
    );
    assert!(
        trace.unclassified_irq_timer_exit_counts.is_empty(),
        "unclassified IRQ/timer exits remain before READY: {:?}",
        trace.unclassified_irq_timer_exit_counts
    );
    let reason = trace.terminal_reason.as_deref().unwrap_or("unknown");
    assert!(
        reason.starts_with("icount_limit_reached(")
            || reason.starts_with("first_detchannel_")
            || reason.starts_with("exit_limit_reached("),
        "trace stopped for unexpected reason: {reason}"
    );
}

fn assert_masked_cpuid_surface(slot: &dh_vmm::kvm::SlotVm) {
    let cpuid = slot
        .vcpu
        .get_cpuid2(KVM_MAX_CPUID_ENTRIES)
        .expect("get vcpu cpuid");
    assert!(
        !cpuid
            .as_slice()
            .iter()
            .any(|e| (0x4000_0000..0x4000_0100).contains(&e.function)),
        "KVM paravirt leaves, including kvmclock, must not be exposed"
    );

    for entry in cpuid.as_slice() {
        match (entry.function, entry.index) {
            (1, _) => {
                assert_eq!(entry.ecx & (1 << 21), 0, "x2APIC");
                assert_eq!(entry.ecx & (1 << 24), 0, "TSC-deadline");
                assert_eq!(entry.ecx & (1 << 30), 0, "RDRAND");
            }
            (7, 0) => {
                assert_eq!(entry.ebx & (1 << 18), 0, "RDSEED");
                assert_eq!(entry.edx & (1 << 29), 0, "ARCH_CAPABILITIES");
            }
            (0x8000_0001, _) => assert_eq!(entry.edx & (1 << 27), 0, "RDTSCP"),
            (0x8000_0007, _) => assert_eq!(entry.edx & (1 << 8), 0, "invariant TSC"),
            _ => {}
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct LinuxBootTrace {
    total_exits: u64,
    exit_kind_counts: BTreeMap<&'static str, u64>,
    denied_msr_indices: BTreeSet<u32>,
    denied_rdmsr_indices: BTreeSet<u32>,
    denied_wrmsr_indices: BTreeSet<u32>,
    apic_mmio_addresses: BTreeSet<u64>,
    apic_msr_indices: BTreeSet<u32>,
    linux_cpu_compat_msr_indices: BTreeSet<u32>,
    unclassified_denied_msr_indices: BTreeSet<u32>,
    unclassified_mmio_addresses: BTreeSet<u64>,
    unclassified_irq_timer_exit_counts: BTreeMap<&'static str, u64>,
    irq_window_open_count: u64,
    intr_count: u64,
    ioapic_eoi_vectors: BTreeSet<u8>,
    first_detchannel: Option<DetchannelStatus>,
    terminal_reason: Option<String>,
    exit_limit: u64,
    icount_limit: Option<u64>,
    final_icount: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DetchannelStatus {
    direction: &'static str,
    port: u16,
    len: usize,
}

impl LinuxBootTrace {
    fn bump(&mut self, kind: &'static str) {
        *self.exit_kind_counts.entry(kind).or_insert(0) += 1;
    }

    fn observe_mmio(&mut self, gpa: u64) {
        if is_apic_mmio(gpa) {
            self.apic_mmio_addresses.insert(gpa);
        } else {
            self.unclassified_mmio_addresses.insert(gpa);
        }
    }

    fn observe_denied_msr(&mut self, index: u32, write: bool) {
        self.denied_msr_indices.insert(index);
        if write {
            self.denied_wrmsr_indices.insert(index);
        } else {
            self.denied_rdmsr_indices.insert(index);
        }
        match denied_msr_class(index, write) {
            DeniedMsrClass::LinuxCpuCompat => {
                self.linux_cpu_compat_msr_indices.insert(index);
            }
            DeniedMsrClass::LapicRequired => {
                self.apic_msr_indices.insert(index);
            }
            DeniedMsrClass::Unclassified => {
                self.unclassified_denied_msr_indices.insert(index);
            }
        }
    }

    fn observe_irq_timer(&mut self, kind: &'static str) {
        *self
            .unclassified_irq_timer_exit_counts
            .entry(kind)
            .or_insert(0) += 1;
    }

    fn observe_detchannel(&mut self, direction: &'static str, port: u16, len: usize) {
        if self.first_detchannel.is_none() {
            self.first_detchannel = Some(DetchannelStatus {
                direction,
                port,
                len,
            });
        }
    }

    fn lapic_required(&self) -> bool {
        !self.apic_mmio_addresses.is_empty() || !self.apic_msr_indices.is_empty()
    }
}

fn trace_linux_boot(
    slot: &mut dh_vmm::kvm::SlotVm,
    exit_limit: u64,
    icount_limit: Option<u64>,
) -> LinuxBootTrace {
    let mut trace = LinuxBootTrace {
        exit_limit,
        icount_limit,
        ..LinuxBootTrace::default()
    };

    if let Some(icount_limit) = icount_limit {
        return trace_linux_boot_with_icount(slot, trace, exit_limit, icount_limit);
    }

    for _ in 0..exit_limit {
        let mut exit = match slot.vcpu.run() {
            Ok(exit) => exit,
            Err(e) => {
                trace.terminal_reason = Some(format!("kvm_run_error: {e}"));
                return trace;
            }
        };

        trace.total_exits += 1;
        trace.bump(raw_exit_kind(&exit));
        prepare_exit_for_trace(&mut trace, &mut exit);
        let terminal = terminal_reason(&exit);
        let event = dh_vmm::kvm::classify_exit(exit);
        observe_classified_event(&mut trace, &event);

        if let Some(reason) = terminal_after_classification(&event) {
            trace.terminal_reason = Some(reason);
            return trace;
        }
        if let Some(reason) = terminal {
            trace.terminal_reason = Some(reason);
            return trace;
        }
    }

    trace.terminal_reason = Some(format!("exit_limit_reached({exit_limit})"));
    trace
}

fn trace_linux_boot_with_icount(
    slot: &mut dh_vmm::kvm::SlotVm,
    mut trace: LinuxBootTrace,
    exit_limit: u64,
    icount_limit: u64,
) -> LinuxBootTrace {
    if let Err(e) = dh_vmm::run::install_kick_handler() {
        trace.terminal_reason = Some(format!("icount_setup_failed: install kick handler: {e}"));
        return trace;
    }
    let counter = match InstRetired::open_for_current_thread() {
        Ok(counter) => counter,
        Err(e) => {
            trace.terminal_reason = Some(format!("icount_unavailable: {e:?}"));
            return trace;
        }
    };
    if let Err(e) =
        counter.route_overflow_to_thread(dh_vmm::run::current_tid(), dh_vmm::run::kick_signal())
    {
        trace.terminal_reason = Some(format!("icount_setup_failed: route overflow: {e:?}"));
        return trace;
    }
    if let Err(e) = counter.reset() {
        trace.terminal_reason = Some(format!("icount_setup_failed: reset: {e:?}"));
        return trace;
    }
    if let Err(e) = counter.enable() {
        trace.terminal_reason = Some(format!("icount_setup_failed: enable: {e:?}"));
        return trace;
    }

    let mut guard = dh_vmm::run::KickGuard::register(&mut slot.vcpu);
    for _ in 0..exit_limit {
        let counted = counter.read().expect("read instruction counter");
        trace.final_icount = Some(counted);
        if counted >= icount_limit {
            trace.terminal_reason = Some(format!(
                "icount_limit_reached(limit={icount_limit}, counted={counted})"
            ));
            return trace;
        }

        if let Err(e) = counter.arm_period(icount_limit - counted) {
            trace.terminal_reason = Some(format!("icount_setup_failed: arm period: {e:?}"));
            return trace;
        }
        let mut exit = match guard.run() {
            Ok(exit) => exit,
            Err(e) if e.errno() == libc::EINTR => {
                dh_vmm::run::clear_immediate_exit(&mut guard);
                let counted = counter.read().expect("read instruction counter after kick");
                trace.final_icount = Some(counted);
                if counted >= icount_limit {
                    trace.terminal_reason = Some(format!(
                        "icount_limit_reached(limit={icount_limit}, counted={counted})"
                    ));
                    return trace;
                }
                continue;
            }
            Err(e) => {
                trace.terminal_reason = Some(format!("kvm_run_error: {e}"));
                return trace;
            }
        };

        trace.total_exits += 1;
        trace.bump(raw_exit_kind(&exit));
        prepare_exit_for_trace(&mut trace, &mut exit);
        let terminal = terminal_reason(&exit);
        let event = dh_vmm::kvm::classify_exit(exit);
        observe_classified_event(&mut trace, &event);

        if let Some(reason) = terminal_after_classification(&event) {
            trace.final_icount = counter.read().ok();
            trace.terminal_reason = Some(reason);
            return trace;
        }
        if let Some(reason) = terminal {
            trace.final_icount = counter.read().ok();
            trace.terminal_reason = Some(reason);
            return trace;
        }
    }

    trace.final_icount = counter.read().ok();
    trace.terminal_reason = Some(format!("exit_limit_reached({exit_limit})"));
    trace
}

fn prepare_exit_for_trace(trace: &mut LinuxBootTrace, exit: &mut VcpuExit<'_>) {
    match exit {
        VcpuExit::IoIn(port, _) if is_detchannel_port(*port) || is_serial_port(*port) => {}
        VcpuExit::IoIn(_, data) => data.fill(0),
        VcpuExit::MmioRead(gpa, data) => {
            trace.observe_mmio(*gpa);
            data.fill(0);
        }
        VcpuExit::MmioWrite(gpa, _) => trace.observe_mmio(*gpa),
        VcpuExit::IrqWindowOpen => {
            trace.irq_window_open_count += 1;
            trace.observe_irq_timer("irq_window_open");
        }
        VcpuExit::Intr => {
            trace.intr_count += 1;
            trace.observe_irq_timer("intr");
        }
        VcpuExit::IoapicEoi(vector) => {
            trace.ioapic_eoi_vectors.insert(*vector);
            trace.observe_irq_timer("ioapic_eoi");
        }
        _ => {}
    }
}

fn terminal_after_classification(event: &ExitEvent) -> Option<String> {
    match event {
        ExitEvent::DetcallIn { port, len } => Some(format!(
            "first_detchannel_in_requires_model(port={port:#x}, len={len})"
        )),
        ExitEvent::SerialIn { port, len } => Some(format!(
            "serial_in_requires_model(port={port:#x}, len={len})"
        )),
        _ => None,
    }
}

fn observe_classified_event(trace: &mut LinuxBootTrace, event: &ExitEvent) {
    match event {
        ExitEvent::MsrReadDenied { index } => trace.observe_denied_msr(*index, false),
        ExitEvent::MsrWriteDenied { index, .. } => trace.observe_denied_msr(*index, true),
        ExitEvent::DetcallIn { port, len } => trace.observe_detchannel("in", *port, *len),
        ExitEvent::DetcallOut { port, data } => trace.observe_detchannel("out", *port, data.len()),
        _ => {}
    }
}

fn is_detchannel_port(port: u16) -> bool {
    let end = dh_vmm::kvm::PIO_DETCALL_BASE + dh_vmm::kvm::PIO_DETCALL_LEN;
    (dh_vmm::kvm::PIO_DETCALL_BASE..end).contains(&port)
}

fn is_serial_port(port: u16) -> bool {
    let end = dh_vmm::kvm::PIO_SERIAL_BASE + dh_vmm::kvm::PIO_SERIAL_LEN;
    (dh_vmm::kvm::PIO_SERIAL_BASE..end).contains(&port)
}

fn raw_exit_kind(exit: &VcpuExit<'_>) -> &'static str {
    match exit {
        VcpuExit::IoOut(..) => "io_out",
        VcpuExit::IoIn(..) => "io_in",
        VcpuExit::MmioRead(..) => "mmio_read",
        VcpuExit::MmioWrite(..) => "mmio_write",
        VcpuExit::Unknown => "unknown",
        VcpuExit::Exception => "exception",
        VcpuExit::Hypercall(_) => "hypercall",
        VcpuExit::Debug(_) => "debug",
        VcpuExit::Hlt => "hlt",
        VcpuExit::IrqWindowOpen => "irq_window_open",
        VcpuExit::Shutdown => "shutdown",
        VcpuExit::FailEntry(..) => "fail_entry",
        VcpuExit::Intr => "intr",
        VcpuExit::SetTpr => "set_tpr",
        VcpuExit::TprAccess => "tpr_access",
        VcpuExit::S390Sieic => "s390_sieic",
        VcpuExit::S390Reset => "s390_reset",
        VcpuExit::Dcr => "dcr",
        VcpuExit::Nmi => "nmi",
        VcpuExit::InternalError => "internal_error",
        VcpuExit::Osi => "osi",
        VcpuExit::PaprHcall => "papr_hcall",
        VcpuExit::S390Ucontrol => "s390_ucontrol",
        VcpuExit::Watchdog => "watchdog",
        VcpuExit::S390Tsch => "s390_tsch",
        VcpuExit::Epr => "epr",
        VcpuExit::SystemEvent(..) => "system_event",
        VcpuExit::S390Stsi => "s390_stsi",
        VcpuExit::IoapicEoi(_) => "ioapic_eoi",
        VcpuExit::Hyperv => "hyperv",
        VcpuExit::X86Rdmsr(_) => "x86_rdmsr",
        VcpuExit::X86Wrmsr(_) => "x86_wrmsr",
        VcpuExit::MemoryFault { .. } => "memory_fault",
        VcpuExit::Unsupported(reason) if *reason == kvm_bindings::KVM_EXIT_DIRTY_RING_FULL => {
            "dirty_ring_full"
        }
        VcpuExit::Unsupported(_) => "unsupported",
    }
}

fn terminal_reason(exit: &VcpuExit<'_>) -> Option<String> {
    match exit {
        VcpuExit::Hlt => Some("hlt".to_string()),
        VcpuExit::Shutdown => Some("shutdown".to_string()),
        VcpuExit::FailEntry(reason, cpu) => {
            Some(format!("fail_entry(reason={reason:#x}, cpu={cpu})"))
        }
        VcpuExit::InternalError => Some("internal_error".to_string()),
        VcpuExit::Unknown => Some("unknown_exit".to_string()),
        VcpuExit::Exception => Some("exception_exit".to_string()),
        VcpuExit::Hypercall(_) => Some("hypercall_exit".to_string()),
        VcpuExit::Debug(_) => Some("debug_exit".to_string()),
        VcpuExit::SetTpr => Some("set_tpr_exit".to_string()),
        VcpuExit::TprAccess => Some("tpr_access_exit".to_string()),
        VcpuExit::S390Sieic => Some("s390_sieic_exit".to_string()),
        VcpuExit::S390Reset => Some("s390_reset_exit".to_string()),
        VcpuExit::Dcr => Some("dcr_exit".to_string()),
        VcpuExit::Nmi => Some("nmi_exit".to_string()),
        VcpuExit::Osi => Some("osi_exit".to_string()),
        VcpuExit::PaprHcall => Some("papr_hcall_exit".to_string()),
        VcpuExit::S390Ucontrol => Some("s390_ucontrol_exit".to_string()),
        VcpuExit::Watchdog => Some("watchdog_exit".to_string()),
        VcpuExit::S390Tsch => Some("s390_tsch_exit".to_string()),
        VcpuExit::Epr => Some("epr_exit".to_string()),
        VcpuExit::SystemEvent(kind, data) => {
            Some(format!("system_event(kind={kind}, words={})", data.len()))
        }
        VcpuExit::S390Stsi => Some("s390_stsi_exit".to_string()),
        VcpuExit::Hyperv => Some("hyperv_exit".to_string()),
        VcpuExit::MemoryFault { flags, gpa, size } => Some(format!(
            "memory_fault(flags={flags:#x}, gpa={gpa:#x}, size={size:#x})"
        )),
        VcpuExit::Unsupported(reason) if *reason == kvm_bindings::KVM_EXIT_DIRTY_RING_FULL => {
            Some("dirty_ring_full_unserviced".to_string())
        }
        VcpuExit::Unsupported(reason) => Some(format!("unsupported_exit({reason})")),
        VcpuExit::IoOut(..)
        | VcpuExit::IoIn(..)
        | VcpuExit::MmioRead(..)
        | VcpuExit::MmioWrite(..)
        | VcpuExit::IrqWindowOpen
        | VcpuExit::Intr
        | VcpuExit::IoapicEoi(_)
        | VcpuExit::X86Rdmsr(_)
        | VcpuExit::X86Wrmsr(_) => None,
    }
}

fn trace_required() -> bool {
    std::env::var(TRACE_BOOT_ENV).as_deref() == Ok("1")
}

fn trace_exit_limit() -> u64 {
    if let Ok(raw) = std::env::var(TRACE_EXIT_LIMIT_ENV) {
        return raw
            .parse::<u64>()
            .unwrap_or_else(|_| panic!("{TRACE_EXIT_LIMIT_ENV} must be a u64, got {raw:?}"));
    }
    if trace_required() {
        DEFAULT_TRACE_EXIT_LIMIT
    } else {
        SMOKE_TRACE_EXIT_LIMIT
    }
}

fn trace_icount_limit() -> Option<u64> {
    if let Ok(raw) = std::env::var(TRACE_ICOUNT_LIMIT_ENV) {
        return Some(
            raw.parse::<u64>()
                .unwrap_or_else(|_| panic!("{TRACE_ICOUNT_LIMIT_ENV} must be a u64, got {raw:?}")),
        );
    }
    trace_required().then_some(DEFAULT_TRACE_ICOUNT_LIMIT)
}

fn is_apic_mmio(gpa: u64) -> bool {
    let base = dh_vmm::boot::linux_bzimage::LINUX_APIC_MMIO_BASE;
    let end = base + dh_vmm::boot::linux_bzimage::LINUX_APIC_MMIO_LEN;
    (base..end).contains(&gpa)
}

fn write_trace(trace: &LinuxBootTrace, path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, trace_json(trace))
}

fn trace_output_path() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("tests/determinism must live two levels below the workspace root");
    workspace_root.join(TRACE_OUTPUT)
}

fn trace_json(trace: &LinuxBootTrace) -> String {
    let mut out = String::new();
    writeln!(out, "{{").unwrap();
    writeln!(out, "  \"schema_version\": 2,").unwrap();
    writeln!(out, "  \"total_exits\": {},", trace.total_exits).unwrap();
    writeln!(out, "  \"exit_limit\": {},", trace.exit_limit).unwrap();
    writeln!(
        out,
        "  \"icount_limit\": {},",
        json_optional_u64(trace.icount_limit)
    )
    .unwrap();
    writeln!(
        out,
        "  \"final_icount\": {},",
        json_optional_u64(trace.final_icount)
    )
    .unwrap();
    writeln!(
        out,
        "  \"terminal_reason\": {},",
        json_string(trace.terminal_reason.as_deref().unwrap_or("unknown"))
    )
    .unwrap();
    writeln!(out, "  \"lapic_required\": {},", trace.lapic_required()).unwrap();
    writeln!(
        out,
        "  \"exit_kind_counts\": {},",
        json_count_map(&trace.exit_kind_counts)
    )
    .unwrap();
    writeln!(
        out,
        "  \"denied_msr_indices\": {},",
        json_hex_u32_set(&trace.denied_msr_indices)
    )
    .unwrap();
    writeln!(
        out,
        "  \"denied_rdmsr_indices\": {},",
        json_hex_u32_set(&trace.denied_rdmsr_indices)
    )
    .unwrap();
    writeln!(
        out,
        "  \"denied_wrmsr_indices\": {},",
        json_hex_u32_set(&trace.denied_wrmsr_indices)
    )
    .unwrap();
    writeln!(
        out,
        "  \"apic_mmio_addresses\": {},",
        json_hex_u64_set(&trace.apic_mmio_addresses)
    )
    .unwrap();
    writeln!(
        out,
        "  \"apic_msr_indices\": {},",
        json_hex_u32_set(&trace.apic_msr_indices)
    )
    .unwrap();
    writeln!(
        out,
        "  \"linux_cpu_compat_msr_indices\": {},",
        json_hex_u32_set(&trace.linux_cpu_compat_msr_indices)
    )
    .unwrap();
    writeln!(
        out,
        "  \"unclassified_denied_msr_indices\": {},",
        json_hex_u32_set(&trace.unclassified_denied_msr_indices)
    )
    .unwrap();
    writeln!(
        out,
        "  \"unclassified_mmio_addresses\": {},",
        json_hex_u64_set(&trace.unclassified_mmio_addresses)
    )
    .unwrap();
    writeln!(
        out,
        "  \"unclassified_irq_timer_exit_counts\": {},",
        json_count_map(&trace.unclassified_irq_timer_exit_counts)
    )
    .unwrap();
    writeln!(
        out,
        "  \"irq_timer\": {{\"irq_window_open\": {}, \"intr\": {}, \"ioapic_eoi_vectors\": {}}},",
        trace.irq_window_open_count,
        trace.intr_count,
        json_u8_set(&trace.ioapic_eoi_vectors)
    )
    .unwrap();
    writeln!(
        out,
        "  \"first_detchannel_status\": {}",
        json_detchannel(&trace.first_detchannel)
    )
    .unwrap();
    writeln!(out, "}}").unwrap();
    out
}

fn json_optional_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "null".to_string())
}

fn json_count_map(map: &BTreeMap<&'static str, u64>) -> String {
    let mut out = String::from("{");
    for (i, (key, value)) in map.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write!(out, "{}: {}", json_string(key), value).unwrap();
    }
    out.push('}');
    out
}

fn json_hex_u32_set(set: &BTreeSet<u32>) -> String {
    let values: Vec<String> = set
        .iter()
        .map(|value| json_string(&format!("{value:#x}")))
        .collect();
    format!("[{}]", values.join(", "))
}

fn json_hex_u64_set(set: &BTreeSet<u64>) -> String {
    let values: Vec<String> = set
        .iter()
        .map(|value| json_string(&format!("{value:#x}")))
        .collect();
    format!("[{}]", values.join(", "))
}

fn json_u8_set(set: &BTreeSet<u8>) -> String {
    let values: Vec<String> = set.iter().map(|value| value.to_string()).collect();
    format!("[{}]", values.join(", "))
}

fn json_detchannel(status: &Option<DetchannelStatus>) -> String {
    match status {
        Some(status) => format!(
            "{{\"reached\": true, \"direction\": {}, \"port\": {}, \"port_hex\": {}, \"len\": {}}}",
            json_string(status.direction),
            status.port,
            json_string(&format!("{:#x}", status.port)),
            status.len
        ),
        None => "{\"reached\": false}".to_string(),
    }
}

fn json_string(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => write!(out, "\\u{:04x}", c as u32).unwrap(),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod trace_tests {
    use super::*;

    #[test]
    fn trace_json_reports_required_m9_fields() {
        let mut trace = LinuxBootTrace {
            total_exits: 2,
            exit_limit: 3,
            icount_limit: Some(10),
            final_icount: Some(8),
            terminal_reason: Some("shutdown".to_string()),
            ..LinuxBootTrace::default()
        };
        trace.bump("x86_rdmsr");
        trace.bump("x86_rdmsr");
        trace.observe_denied_msr(MSR_IA32_APIC_BASE, false);
        trace.observe_mmio(dh_vmm::boot::linux_bzimage::LINUX_APIC_MMIO_BASE + 0x30);
        trace.irq_window_open_count = 1;
        trace.observe_irq_timer("irq_window_open");
        trace.observe_detchannel("out", dh_vmm::kvm::PIO_DETCALL_BASE, 4);

        let json = trace_json(&trace);
        assert!(json.contains("\"icount_limit\": 10"));
        assert!(json.contains("\"final_icount\": 8"));
        assert!(json.contains("\"exit_kind_counts\": {\"x86_rdmsr\": 2}"));
        assert!(json.contains("\"denied_msr_indices\": [\"0x1b\"]"));
        assert!(json.contains("\"apic_mmio_addresses\": [\"0xfee00030\"]"));
        assert!(json.contains("\"apic_msr_indices\": [\"0x1b\"]"));
        assert!(json.contains("\"unclassified_denied_msr_indices\": []"));
        assert!(json.contains("\"unclassified_mmio_addresses\": []"));
        assert!(json.contains("\"unclassified_irq_timer_exit_counts\": {\"irq_window_open\": 1}"));
        assert!(json.contains("\"lapic_required\": true"));
        assert!(json.contains("\"first_detchannel_status\": {\"reached\": true"));
    }

    #[test]
    fn fatal_before_serviceable_exit_preserves_smoke_contract() {
        let trace = LinuxBootTrace {
            total_exits: 1,
            terminal_reason: Some("shutdown".to_string()),
            ..LinuxBootTrace::default()
        };
        assert!(fatal_before_serviceable_exit(&trace));

        let trace = LinuxBootTrace {
            total_exits: 1,
            terminal_reason: Some("icount_limit_reached(limit=1, counted=1)".to_string()),
            ..LinuxBootTrace::default()
        };
        assert!(!fatal_before_serviceable_exit(&trace));
    }

    #[test]
    fn trace_acceptance_rejects_unclassified_surfaces() {
        let mut trace = LinuxBootTrace {
            terminal_reason: Some("icount_limit_reached(limit=10, counted=10)".to_string()),
            ..LinuxBootTrace::default()
        };
        assert_trace_acceptance(&trace);

        trace.unclassified_denied_msr_indices.insert(0x10);
        let panic = std::panic::catch_unwind(|| assert_trace_acceptance(&trace));
        assert!(panic.is_err(), "unclassified MSRs must fail acceptance");
        trace.unclassified_denied_msr_indices.clear();

        trace.unclassified_mmio_addresses.insert(0xdead_beef);
        let panic = std::panic::catch_unwind(|| assert_trace_acceptance(&trace));
        assert!(panic.is_err(), "unclassified MMIO must fail acceptance");
        trace.unclassified_mmio_addresses.clear();

        trace.observe_irq_timer("intr");
        let panic = std::panic::catch_unwind(|| assert_trace_acceptance(&trace));
        assert!(
            panic.is_err(),
            "unclassified IRQ/timer exits must fail acceptance"
        );
    }
}
