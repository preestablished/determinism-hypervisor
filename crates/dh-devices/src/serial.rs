//! debug-serial (ARCH §6.9): 16550-subset, OUTPUT-ONLY. PIO 0x3F8 plus
//! the MMIO mirror window (0xD000_6000, §2.2).
//!
//! Determinism posture: input would be nondeterministic, so there is
//! none — RX reads return 0; LSR always reads transmitter-ready (0x60),
//! so an LSR-polling driver never spins (the iter-29 hazard); config
//! writes (IER/LCR/MCR/...) are swallowed and read back as fixed
//! constants. Serial output NEVER enters the state hash: the snapshot
//! section is EMPTY by design (the pending output buffer is host-side
//! observability — the run loop drains it to the slot's JSON log — not
//! machine state).

use crate::ctx::DevCtx;
use crate::{DetDevice, RestoreError};

pub const DEVICE_ID_DEBUG_SERIAL: u16 = 0x0006;
pub const SERIAL_PIO_BASE: u16 = 0x3F8;
pub const SERIAL_PIO_LEN: u16 = 8;

/// 16550 register indices (byte registers; the MMIO mirror exposes each
/// in its own 4-byte slot at `0x08 + reg * 4` — the bus enforces 4-byte
/// aligned access, so byte-packing them would be unreachable).
/// Register map: 0 THR/RBR, 1 IER, 2 IIR/FCR, 3 LCR, 4 MCR, 5 LSR,
/// 6 MSR, 7 scratch. Only the ones with non-zero/output behavior get
/// named constants; the rest read 0 and swallow writes.
const REG_THR_RBR: u16 = 0; // write: transmit; read: RX (always 0)
const REG_IIR_FCR: u16 = 2;
const REG_LSR: u16 = 5;

/// LSR value: THR empty (bit 5) + transmitter empty (bit 6) — always
/// ready, never busy.
const LSR_ALWAYS_READY: u8 = 0x60;
/// IIR: no interrupt pending.
const IIR_NONE: u8 = 0x01;

#[derive(Debug, Default)]
pub struct DebugSerial {
    /// Pending output since the last drain (host observability only).
    out: Vec<u8>,
}

impl DebugSerial {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drain pending output (the run loop flushes this to the slot's
    /// JSON log / stdout).
    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.out)
    }

    /// Fixed read value for a 16550 register (output-only model).
    ///
    /// Deliberately an associated fn (no `&self`): register reads must
    /// never depend on device state — that invariant is what keeps the
    /// model output-only and its reads replay-identical.
    fn reg_read(reg: u16) -> u8 {
        match reg {
            REG_LSR => LSR_ALWAYS_READY,
            REG_IIR_FCR => IIR_NONE,
            // RBR, IER, LCR, MCR, MSR, scratch: zeros — never host state.
            _ => 0,
        }
    }

    /// PIO write at `port`. Caller contract: `port` is within
    /// `SERIAL_PIO_BASE..SERIAL_PIO_BASE + SERIAL_PIO_LEN` (out-of-range
    /// ports are swallowed). Only THR produces output; a multi-byte OUT
    /// at THR logs EVERY byte (real hardware would spread `out dx, ax`
    /// across THR+IER) — determinism over fidelity, and the JSON log
    /// keeps the guest's full output.
    pub fn pio_write(&mut self, port: u16, data: &[u8]) {
        debug_assert!((SERIAL_PIO_BASE..SERIAL_PIO_BASE + SERIAL_PIO_LEN).contains(&port));
        if port == SERIAL_PIO_BASE + REG_THR_RBR {
            self.out.extend_from_slice(data);
        }
    }

    /// PIO read at `port` (same caller contract as `pio_write`): fill
    /// `data` with the register's fixed value. Every byte of a wider
    /// access reads the SAME register — real hardware would read
    /// consecutive registers; we choose determinism over fidelity, and
    /// no future "fix" should reintroduce position-dependent reads.
    pub fn pio_read(&self, port: u16, data: &mut [u8]) {
        debug_assert!((SERIAL_PIO_BASE..SERIAL_PIO_BASE + SERIAL_PIO_LEN).contains(&port));
        let reg = port.saturating_sub(SERIAL_PIO_BASE);
        data.fill(Self::reg_read(reg));
    }
}

impl DetDevice for DebugSerial {
    fn device_id(&self) -> u16 {
        DEVICE_ID_DEBUG_SERIAL
    }

    fn section_version(&self) -> u16 {
        1
    }

    /// MMIO mirror: register `reg` at window offset `0x08 + reg * 4`.
    /// Only exact 4-byte slot accesses participate; 8-byte accesses
    /// (legal on the bus) are not part of the §6.9 mirror and read 0.
    fn mmio_read(&mut self, off: u64, data: &mut [u8], _ctx: &mut DevCtx) {
        if data.len() != 4 || !(0x08..0x28).contains(&off) || !off.is_multiple_of(4) {
            data.fill(0);
            return;
        }
        let reg = ((off - 0x08) / 4) as u16;
        data.copy_from_slice(&u32::from(Self::reg_read(reg)).to_le_bytes());
    }

    fn mmio_write(&mut self, off: u64, data: &[u8], _ctx: &mut DevCtx) {
        if data.len() == 4 && off == 0x08 {
            // THR mirror: low byte transmits (one register per 4-byte
            // slot, so unlike a PIO THR burst only data[0] is output).
            self.out.push(data[0]);
        }
        // Everything else, incl. non-4-byte widths: swallowed (§6.9).
    }

    /// EMPTY by design: serial state never enters the state hash.
    fn snapshot(&self, _out: &mut Vec<u8>) {}

    fn restore(&mut self, bytes: &[u8], sec_version: u16) -> Result<(), RestoreError> {
        if sec_version != 1 || !bytes.is_empty() {
            return Err(RestoreError);
        }
        // Pending output is observability, not state: a restored slot
        // starts with an empty buffer.
        self.out.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::MmioBus;
    use crate::ctx::test_support::FakeEntropy;
    use crate::ctx::VecGuestMem;
    use dh_inputlog::dhilog::{LogWriter, SegmentHeader};

    fn ctx_parts() -> (LogWriter, VecGuestMem, FakeEntropy, Vec<crate::IrqRequest>) {
        (
            LogWriter::new(SegmentHeader {
                base_snapshot_id: [0; 32],
                entropy_seed: [0; 32],
                machine_config_hash: [0; 32],
                clock_num: 1,
                clock_den: 1,
            }),
            VecGuestMem(vec![0u8; 16]),
            FakeEntropy(0),
            Vec::new(),
        )
    }

    #[test]
    fn output_accumulates_and_drains() {
        let mut s = DebugSerial::new();
        s.pio_write(0x3F8, b"HE");
        s.pio_write(0x3F8, b"LLO");
        s.pio_write(0x3F9, b"X"); // IER write: swallowed
        assert_eq!(s.take_output(), b"HELLO");
        assert_eq!(s.take_output(), b"", "drained");
    }

    #[test]
    fn wide_pio_access_reads_one_register_and_logs_every_thr_byte() {
        let mut s = DebugSerial::new();
        // Wide IN at LSR: every byte reads the SAME register (determinism
        // over fidelity — real hardware would read LSR then MSR).
        let mut w = [0u8; 2];
        s.pio_read(0x3FD, &mut w);
        assert_eq!(w, [0x60, 0x60]);
        // Wide OUT at THR: every byte is output (not spread to IER).
        s.pio_write(0x3F8, b"AB");
        assert_eq!(s.take_output(), b"AB");
    }

    #[test]
    fn lsr_polls_ready_and_rx_reads_zero() {
        let s = DebugSerial::new();
        let mut b = [0u8; 1];
        s.pio_read(0x3FD, &mut b); // LSR
        assert_eq!(b[0], 0x60, "THR empty + transmitter empty, always");
        s.pio_read(0x3F8, &mut b); // RBR
        assert_eq!(b[0], 0, "no input exists");
        s.pio_read(0x3FA, &mut b); // IIR
        assert_eq!(b[0], 0x01, "no interrupt pending");
    }

    #[test]
    fn mmio_mirror_serves_registers_in_4_byte_slots() {
        let mut bus = MmioBus::new();
        bus.register(0xD000_6000, Box::new(DebugSerial::new()))
            .unwrap();
        let (mut l, mut mem, mut ent, mut q) = ctx_parts();
        let mut ctx = DevCtx::new(1, 0, &mut l, &mut mem, &mut ent, &mut q);
        let mut v4 = [0u8; 4];
        bus.read(0xD000_6000, &mut v4, &mut ctx).unwrap();
        assert_eq!(u32::from_le_bytes(v4), u32::from(DEVICE_ID_DEBUG_SERIAL));
        // LSR mirror at 0x08 + 5*4 = 0x1C.
        bus.read(0xD000_601C, &mut v4, &mut ctx).unwrap();
        assert_eq!(u32::from_le_bytes(v4), 0x60);

        // THR mirror write transmits the low byte (direct on the device:
        // the bus boxes it, and output draining is the run loop's seam).
        let mut s = DebugSerial::new();
        s.mmio_write(0x08, &(b'Z' as u32).to_le_bytes(), &mut ctx);
        s.mmio_write(0x0C, &(b'X' as u32).to_le_bytes(), &mut ctx); // IER: swallowed
        assert_eq!(s.take_output(), b"Z");
    }

    #[test]
    fn snapshot_is_empty_and_restore_clears_pending() {
        let mut s = DebugSerial::new();
        s.pio_write(0x3F8, b"pending");
        let mut section = Vec::new();
        s.snapshot(&mut section);
        assert!(section.is_empty(), "serial never enters the state hash");
        s.restore(&[], 1).unwrap();
        assert_eq!(s.take_output(), b"", "restored slots start clean");
        assert!(s.restore(&[1], 1).is_err());
        assert!(s.restore(&[], 2).is_err());
    }
}
