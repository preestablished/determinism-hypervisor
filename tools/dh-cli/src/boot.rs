//! dh-cli's boot driver: dh-vmm's ELF boot path (loader, page tables,
//! BootInfo, MSR filter, long-mode entry — crates/dh-vmm/src/boot.rs) plus
//! a debug run-until-HLT loop with a serial sink.
//!
//! The loop is the DEBUG one: serial OUT bytes are collected, every IN
//! reads as zeros (answered on the raw exit — the classify_exit IN-FILL
//! contract makes post-classify filling impossible; a 16550 LSR-polling
//! driver would spin here, see bead avm), MMIO errors out (the device-bus
//! run loop is the M1 acceptance bead), HLT ends the run. Denied MSR
//! accesses are emulated deterministically by classify_exit itself.

use dh_vmm::boot::load_and_enter;
use dh_vmm::kvm::{classify_exit, ExitEvent, KvmSystem};
use kvm_ioctls::VcpuExit;

const SERIAL_BASE: u16 = 0x3F8;
const SERIAL_END: u16 = 0x400;

#[derive(Debug)]
pub struct BootOutcome {
    /// Bytes the guest wrote to the debug serial port, in order.
    pub serial: Vec<u8>,
    /// Total VM exits consumed.
    pub exits: u64,
}

#[derive(Debug)]
pub enum BootError {
    Kvm(String),
    Loader(dh_vmm::boot::BootError),
    /// The guest did something this debug loop does not model.
    UnexpectedExit(String),
    /// max_exits consumed without reaching HLT.
    ExitBudget,
}

impl std::fmt::Display for BootError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BootError::Kvm(e) => write!(f, "kvm: {e}"),
            BootError::Loader(e) => write!(f, "{e}"),
            BootError::UnexpectedExit(e) => write!(f, "unexpected exit: {e}"),
            BootError::ExitBudget => write!(f, "exit budget exhausted before HLT"),
        }
    }
}

/// Boot `elf` with `mem_bytes` of RAM and run until HLT (or the exit
/// budget). Returns the serial bytes the guest produced.
pub fn boot(
    elf: &[u8],
    mem_bytes: u64,
    cmdline: &[u8],
    max_exits: u64,
) -> Result<BootOutcome, BootError> {
    let sys = KvmSystem::open().map_err(|e| BootError::Kvm(format!("{e:?}")))?;
    let slot = sys
        .create_slot_vm(mem_bytes)
        .map_err(|e| BootError::Kvm(format!("{e:?}")))?;
    load_and_enter(&slot, elf, cmdline).map_err(BootError::Loader)?;
    run_until_hlt(slot, max_exits)
}

fn run_until_hlt(mut slot: dh_vmm::kvm::SlotVm, max_exits: u64) -> Result<BootOutcome, BootError> {
    let mut serial = Vec::new();
    let mut exits = 0u64;
    while exits < max_exits {
        exits += 1;
        let exit = slot
            .vcpu
            .run()
            .map_err(|e| BootError::Kvm(format!("KVM_RUN: {e}")))?;
        match exit {
            // INs answered on the raw exit (IN-FILL contract): zeros.
            VcpuExit::IoIn(_port, data) => data.fill(0),
            VcpuExit::IoOut(port, data) if (SERIAL_BASE..SERIAL_END).contains(&port) => {
                serial.extend_from_slice(data);
            }
            other => match classify_exit(other) {
                ExitEvent::Hlt => return Ok(BootOutcome { serial, exits }),
                ExitEvent::DetcallOut { .. } | ExitEvent::PioIgnored { .. } => {}
                // Denied MSRs are already deterministically emulated by
                // classify_exit (supply 0 / inject #GP); just continue.
                ExitEvent::MsrReadDenied { .. } | ExitEvent::MsrWriteDenied { .. } => {}
                ExitEvent::MmioRead { gpa, .. } | ExitEvent::MmioWrite { gpa, .. } => {
                    return Err(BootError::UnexpectedExit(format!(
                        "MMIO at {gpa:#x} (debug loop has no device bus; the \
                         M1 acceptance run loop wires dh-devices)"
                    )));
                }
                ExitEvent::Shutdown => {
                    return Err(BootError::UnexpectedExit(
                        "guest shutdown / triple fault".into(),
                    ));
                }
                ev => {
                    return Err(BootError::UnexpectedExit(format!("{ev:?}")));
                }
            },
        }
    }
    Err(BootError::ExitBudget)
}
