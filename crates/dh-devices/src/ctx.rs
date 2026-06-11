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
    /// Sticky: first log failure in this dispatch. MMIO handlers return ()
    /// and cannot fault, so the wrappers record failures here and the VMM
    /// MUST check [`DevCtx::log_fault`] after every dispatch — a dropped
    /// record (e.g. a FRAME_MARK missing from the frame table) is a
    /// DATA_LOSS-class slot fault, never silently absorbed.
    log_fault: Option<dh_inputlog::dhilog::WriteError>,
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
            log_fault: None,
        }
    }

    /// Queue an interrupt for the boundary engine. Append-only: devices
    /// cannot observe, reorder, or cancel the queue.
    pub fn request_irq(&mut self, vector: u8) {
        self.irq_queue.push(IrqRequest { vector });
    }

    /// First log failure of this dispatch, if any. The VMM checks this
    /// after every bus dispatch and faults the slot (DATA_LOSS class) on
    /// `Some` — see the field doc.
    pub fn log_fault(&self) -> Option<dh_inputlog::dhilog::WriteError> {
        self.log_fault
    }

    fn record(&mut self, r: Result<(), dh_inputlog::dhilog::WriteError>) {
        if let (Err(e), None) = (r, self.log_fault) {
            self.log_fault = Some(e);
        }
    }

    /// Log a canonical DEV_EVENT at THIS boundary (icount/rip stamped from
    /// the context — the only way a device may write canonical records).
    /// Failures stick in [`Self::log_fault`]; devices need not handle them.
    pub fn log_dev_event(&mut self, device_id: u16, event_type: u16, data: &[u8]) {
        let r = self
            .log
            .dev_event(self.icount, self.boundary_rip, device_id, event_type, data);
        self.record(r);
    }

    /// Log a DEV_EVENT/PIO_ANSWER at this boundary (detcall IN returns).
    pub fn log_pio_answer(&mut self, port: u16, value: u32) {
        let r = self
            .log
            .pio_answer(self.icount, self.boundary_rip, port, value);
        self.record(r);
    }

    /// Log an AUX ENTROPY record at this boundary (pv-entropy serves).
    pub fn log_entropy(&mut self, len: u32, digest8: u64) {
        let r = self
            .log
            .entropy(self.icount, self.boundary_rip, len, digest8);
        self.record(r);
    }

    /// Log the AUX NET_TX record at THIS boundary (length + digest8 of
    /// the transmitted frame; the frame itself never enters the log).
    pub fn log_net_tx(&mut self, len: u32, digest8: u64) {
        let r = self
            .log
            .net_tx(self.icount, self.boundary_rip, len, digest8);
        self.record(r);
    }

    /// Log an AUX FRAME_MARK at this boundary (pv-pad FRAME_COUNTER write).
    pub fn log_frame_mark(&mut self, frame_index: u32) {
        let r = self
            .log
            .frame_mark(self.icount, self.boundary_rip, frame_index);
        self.record(r);
    }

    /// Log an AUX SDK_EVENT at this boundary (detchannel ring drains —
    /// digest only; the payload travels via the gRPC stream, API.md §3.3).
    pub fn log_sdk_event(&mut self, stream: u16, len: u32, digest8: u64) {
        let r = self
            .log
            .sdk_event(self.icount, self.boundary_rip, stream, len, digest8);
        self.record(r);
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
            encoder_fingerprint: 0,
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
    fn log_failures_stick_in_log_fault() {
        let mut l = log();
        // Pre-log at icount 100 so a later dispatch at icount 50 regresses.
        l.frame_mark(100, 0, 1).unwrap();
        let mut mem = VecGuestMem(vec![0u8; 4]);
        let mut ent = FakeEntropy(0);
        let mut q = Vec::new();
        let mut ctx = DevCtx::new(50, 0, &mut l, &mut mem, &mut ent, &mut q);
        assert_eq!(ctx.log_fault(), None);
        ctx.log_frame_mark(2);
        assert_eq!(
            ctx.log_fault(),
            Some(dh_inputlog::dhilog::WriteError::IcountRegressed)
        );
        // First failure sticks even after a later success.
        ctx.icount = 200;
        ctx.log_frame_mark(3);
        assert_eq!(
            ctx.log_fault(),
            Some(dh_inputlog::dhilog::WriteError::IcountRegressed)
        );
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
