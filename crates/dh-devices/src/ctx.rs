//! DevCtx (ARCH §6): everything a device may touch during an MMIO handler.
//!
//! Devices are plain state machines. The ONLY world they see is this
//! context: current icount, the input-log writer, guest memory, the
//! interrupt-request queue (drained by the boundary engine §3.4 — devices
//! never inject directly), and the entropy PRNG. No host time, no host
//! randomness, no host I/O on the execution path — enforced by the
//! deny-list gate (lib.rs lints + the no_host_ambient_authority test).

use dh_inputlog::dhilog::LogWriter;

/// Guest physical memory access seam. dh-vmm implements this over its
/// GuestMemoryMmap; tests use `VecGuestMem`. Out-of-range access is an
/// error, never a panic — devices translate it to a guest fault.
pub trait GuestMem {
    fn read(&self, gpa: u64, data: &mut [u8]) -> Result<(), MemError>;
    fn write(&mut self, gpa: u64, data: &[u8]) -> Result<(), MemError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemError;

/// Deterministic entropy seam. The pv-entropy bead provides the seeded
/// ChaCha20 implementation; nothing else may produce bytes here.
pub trait EntropySource {
    fn fill(&mut self, buf: &mut [u8]);
}

/// An interrupt request queued by a device. The boundary engine drains the
/// queue and injects per the §3.4 rule at the next boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IrqRequest {
    pub vector: u8,
}

/// The device execution context for one MMIO dispatch.
///
/// Deliberately ABSENT: the clock rational / vns. The rational is per-VM
/// immutable MachineConfig state (ARCH §4); pv-clock owns its own copy from
/// construction and computes vns from `icount` itself. The VMM owes this
/// context exactly one time fact per exit: a correct `icount`.
pub struct DevCtx<'a> {
    pub icount: u64,
    /// Guest RIP at the exit boundary (recorded into log records).
    pub boundary_rip: u64,
    /// Private: devices log through the `log_*` wrappers below, which stamp
    /// `icount`/`boundary_rip` from this context — a device cannot stamp a
    /// canonical record at the wrong boundary (silent replay divergence).
    log: &'a mut LogWriter,
    pub mem: &'a mut dyn GuestMem,
    pub entropy: &'a mut dyn EntropySource,
    irq_queue: &'a mut Vec<IrqRequest>,
}

impl<'a> DevCtx<'a> {
    pub fn new(
        icount: u64,
        boundary_rip: u64,
        log: &'a mut LogWriter,
        mem: &'a mut dyn GuestMem,
        entropy: &'a mut dyn EntropySource,
        irq_queue: &'a mut Vec<IrqRequest>,
    ) -> Self {
        Self {
            icount,
            boundary_rip,
            log,
            mem,
            entropy,
            irq_queue,
        }
    }

    /// Queue an interrupt for the boundary engine. Append-only: devices
    /// cannot observe, reorder, or cancel the queue.
    pub fn request_irq(&mut self, vector: u8) {
        self.irq_queue.push(IrqRequest { vector });
    }

    /// Log a canonical DEV_EVENT at THIS boundary (icount/rip stamped from
    /// the context — the only way a device may write canonical records).
    pub fn log_dev_event(
        &mut self,
        device_id: u16,
        event_type: u16,
        data: &[u8],
    ) -> Result<(), dh_inputlog::dhilog::WriteError> {
        self.log
            .dev_event(self.icount, self.boundary_rip, device_id, event_type, data)
    }

    /// Log a DEV_EVENT/PIO_ANSWER at this boundary (detcall IN returns).
    pub fn log_pio_answer(
        &mut self,
        port: u16,
        value: u32,
    ) -> Result<(), dh_inputlog::dhilog::WriteError> {
        self.log
            .pio_answer(self.icount, self.boundary_rip, port, value)
    }

    /// Log an AUX ENTROPY record at this boundary (pv-entropy serves).
    pub fn log_entropy(
        &mut self,
        len: u32,
        digest8: u64,
    ) -> Result<(), dh_inputlog::dhilog::WriteError> {
        self.log
            .entropy(self.icount, self.boundary_rip, len, digest8)
    }

    /// Log an AUX FRAME_MARK at this boundary (pv-pad FRAME_COUNTER write).
    pub fn log_frame_mark(
        &mut self,
        frame_index: u32,
    ) -> Result<(), dh_inputlog::dhilog::WriteError> {
        self.log
            .frame_mark(self.icount, self.boundary_rip, frame_index)
    }
}

/// Simple Vec-backed guest memory for unit tests (and the nanokernel
/// harness until real guest memory lands).
pub struct VecGuestMem(pub Vec<u8>);

impl GuestMem for VecGuestMem {
    fn read(&self, gpa: u64, data: &mut [u8]) -> Result<(), MemError> {
        let start = usize::try_from(gpa).map_err(|_| MemError)?;
        let end = start.checked_add(data.len()).ok_or(MemError)?;
        data.copy_from_slice(self.0.get(start..end).ok_or(MemError)?);
        Ok(())
    }

    fn write(&mut self, gpa: u64, data: &[u8]) -> Result<(), MemError> {
        let start = usize::try_from(gpa).map_err(|_| MemError)?;
        let end = start.checked_add(data.len()).ok_or(MemError)?;
        self.0
            .get_mut(start..end)
            .ok_or(MemError)?
            .copy_from_slice(data);
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    /// Counter-pattern entropy for tests — deterministic and obviously fake.
    pub struct FakeEntropy(pub u8);
    impl EntropySource for FakeEntropy {
        fn fill(&mut self, buf: &mut [u8]) {
            for b in buf {
                *b = self.0;
                self.0 = self.0.wrapping_add(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::FakeEntropy;
    use super::*;
    use dh_inputlog::dhilog::SegmentHeader;

    fn log() -> LogWriter {
        LogWriter::new(SegmentHeader {
            base_snapshot_id: [0; 32],
            entropy_seed: [0; 32],
            machine_config_hash: [0; 32],
            clock_num: 1,
            clock_den: 1,
        })
    }

    #[test]
    fn vec_guest_mem_bounds_checked() {
        let mut mem = VecGuestMem(vec![0u8; 16]);
        mem.write(8, &[1, 2, 3, 4]).unwrap();
        let mut out = [0u8; 4];
        mem.read(8, &mut out).unwrap();
        assert_eq!(out, [1, 2, 3, 4]);
        assert_eq!(mem.read(13, &mut out), Err(MemError));
        assert_eq!(mem.write(u64::MAX, &[0]), Err(MemError));
    }

    #[test]
    fn irq_requests_are_queued_not_injected() {
        let mut l = log();
        let mut mem = VecGuestMem(vec![0u8; 4]);
        let mut ent = FakeEntropy(0);
        let mut q = Vec::new();
        let mut ctx = DevCtx::new(42, 0x1000, &mut l, &mut mem, &mut ent, &mut q);
        ctx.request_irq(0x30);
        ctx.request_irq(0x31);
        assert_eq!(
            q,
            vec![IrqRequest { vector: 0x30 }, IrqRequest { vector: 0x31 }]
        );
    }
}
