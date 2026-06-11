//! pv-pad (ARCH §6.4, window base 0xD000_1000): controller input latch +
//! the frame counter.
//!
//! Register map (window-relative, all 4-byte):
//!   0x08..0x14  PAD0..PAD3    RO  current latch values
//!   0x18        IRQ_VECTOR    RW  "pad changed" edge vector; 0 = disabled
//!   0x1C        FRAME_COUNTER RW  guest writes F each emulated frame
//!
//! The latch is the platform's ONLY pad-input path and changes ONLY when a
//! canonical PAD_SET record lands at its icount — run control applies it
//! via [`PvPad::apply_pad_set`] (direct state access, not MMIO). Guest
//! reads are MMIO exits at deterministic icounts returning values that
//! changed only at logged icounts ⇒ fully deterministic.
//!
//! FRAME_COUNTER is lineage-ABSOLUTE (never segment-relative): it is
//! device state snapshotted in PADD and strictly increasing along a
//! lineage. The guest's MMIO write of F each emulated frame IS the
//! frame-boundary exit; the device logs the AUX FRAME_MARK (absolute F at
//! this segment-relative icount) — that record stream is the per-segment
//! frame table. Contract checks (monotonicity, the §6.6 ring-W FrameMark
//! equality rule) belong to run control / the detchannel drain, which can
//! FAULT the slot; a device MMIO handler cannot.

use crate::ctx::DevCtx;
use crate::{DetDevice, RestoreError};

pub const PV_PAD_BASE: u64 = 0xD000_1000;
pub const DEVICE_ID_PV_PAD: u16 = 0x0003;

pub const REG_PAD0: u64 = 0x08;
pub const REG_IRQ_VECTOR: u64 = 0x18;
pub const REG_FRAME_COUNTER: u64 = 0x1C;

pub const NUM_PORTS: usize = 4;

const SECTION_VERSION: u16 = 1;
/// Section: latch[4] u32 || irq_vector u32 || frame_counter u32 (24 bytes).
const SECTION_LEN: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadError {
    /// PAD_SET names a port outside 0..4 — corrupt or hostile log input.
    BadPort,
}

pub struct PvPad {
    latch: [u32; NUM_PORTS],
    /// 0 = edge interrupt disabled (default; the demo harness polls).
    irq_vector: u32,
    /// Absolute FRAME_COUNTER (see module docs).
    frame_counter: u32,
}

impl Default for PvPad {
    fn default() -> Self {
        Self::new()
    }
}

impl PvPad {
    pub fn new() -> Self {
        Self {
            latch: [0; NUM_PORTS],
            irq_vector: 0,
            frame_counter: 0,
        }
    }

    /// Run-control entry point: apply a canonical PAD_SET record landing at
    /// its icount. Returns the edge-interrupt vector to inject (per §3.4)
    /// if the latch value actually changed and the guest enabled it.
    pub fn apply_pad_set(&mut self, port: u8, buttons: u32) -> Result<Option<u8>, PadError> {
        let idx = usize::from(port);
        if idx >= NUM_PORTS {
            return Err(PadError::BadPort);
        }
        let changed = self.latch[idx] != buttons;
        self.latch[idx] = buttons;
        Ok((changed && self.irq_vector != 0).then_some(self.irq_vector as u8))
    }

    /// Current absolute frame counter (host samples it for GetFramebuffer
    /// metadata and at_frame → icount conversion).
    pub fn frame_counter(&self) -> u32 {
        self.frame_counter
    }
}

impl DetDevice for PvPad {
    fn device_id(&self) -> u16 {
        DEVICE_ID_PV_PAD
    }

    fn section_version(&self) -> u16 {
        SECTION_VERSION
    }

    fn mmio_read(&mut self, off: u64, data: &mut [u8], _ctx: &mut DevCtx) {
        if data.len() != 4 {
            data.fill(0); // all pv-pad registers are 4-byte
            return;
        }
        let value = match off {
            0x08 | 0x0C | 0x10 | 0x14 => self.latch[((off - REG_PAD0) / 4) as usize],
            REG_IRQ_VECTOR => self.irq_vector,
            REG_FRAME_COUNTER => self.frame_counter,
            _ => 0,
        };
        data.copy_from_slice(&value.to_le_bytes());
    }

    fn mmio_write(&mut self, off: u64, data: &[u8], ctx: &mut DevCtx) {
        if data.len() != 4 {
            return;
        }
        let value = u32::from_le_bytes(data.try_into().unwrap());
        match off {
            // Masked to u8 on write so read-back == injected vector; 0
            // disables.
            REG_IRQ_VECTOR => self.irq_vector = value & 0xFF,
            // The frame-boundary exit (§6.4/§6.6): record absolute F at
            // this segment-relative icount. A log failure sticks in
            // ctx.log_fault(), which the VMM checks after every dispatch
            // and treats as a DATA_LOSS slot fault — never silently
            // absorbed. The counter update stays applied deterministically.
            REG_FRAME_COUNTER => {
                self.frame_counter = value;
                ctx.log_frame_mark(value);
            }
            // PAD0..3 are RO via MMIO (latch changes only through
            // apply_pad_set); unknown offsets ignored.
            _ => {}
        }
    }

    fn snapshot(&self, out: &mut Vec<u8>) {
        for v in self.latch {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&self.irq_vector.to_le_bytes());
        out.extend_from_slice(&self.frame_counter.to_le_bytes());
    }

    fn restore(&mut self, bytes: &[u8], sec_version: u16) -> Result<(), RestoreError> {
        if sec_version != SECTION_VERSION || bytes.len() != SECTION_LEN {
            return Err(RestoreError);
        }
        for (i, slot) in self.latch.iter_mut().enumerate() {
            *slot = u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
        }
        self.irq_vector = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
        self.frame_counter = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
        Ok(())
    }
    /// Run-control downcast seam (recording layer): PAD_SET applications
    /// go through the concrete `apply_pad_set`, keyed by device id.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::test_support::FakeEntropy;
    use crate::ctx::{DevCtx, IrqRequest, VecGuestMem};
    use crate::MmioBus;
    use dh_inputlog::dhilog::{LogWriter, SealParams, SegmentHeader, KIND_FRAME_MARK, RFLAG_AUX};

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

    const B: u64 = PV_PAD_BASE;

    #[test]
    fn latch_changes_only_via_apply_pad_set() {
        let mut pad = PvPad::new();
        // MMIO writes to PAD0..3 are ignored (RO).
        let mut l = log();
        let mut m = VecGuestMem(vec![0u8; 4]);
        let mut e = FakeEntropy(0);
        let mut q: Vec<IrqRequest> = Vec::new();
        let mut ctx = DevCtx::new(1, 0, &mut l, &mut m, &mut e, &mut q);
        pad.mmio_write(0x08, &7u32.to_le_bytes(), &mut ctx);
        let mut v = [0u8; 4];
        pad.mmio_read(0x08, &mut v, &mut ctx);
        assert_eq!(u32::from_le_bytes(v), 0);

        // Canonical path mutates.
        assert_eq!(pad.apply_pad_set(0, 0xA0A0), Ok(None)); // irq disabled
        pad.mmio_read(0x08, &mut v, &mut ctx);
        assert_eq!(u32::from_le_bytes(v), 0xA0A0);
        assert_eq!(pad.apply_pad_set(4, 1), Err(PadError::BadPort));
    }

    #[test]
    fn edge_irq_only_on_change_and_enabled() {
        let mut pad = PvPad::new();
        assert_eq!(pad.apply_pad_set(1, 5), Ok(None)); // disabled
        let mut l = log();
        let mut m = VecGuestMem(vec![0u8; 4]);
        let mut e = FakeEntropy(0);
        let mut q: Vec<IrqRequest> = Vec::new();
        let mut ctx = DevCtx::new(1, 0, &mut l, &mut m, &mut e, &mut q);
        pad.mmio_write(REG_IRQ_VECTOR, &0x45u32.to_le_bytes(), &mut ctx);
        assert_eq!(pad.apply_pad_set(1, 6), Ok(Some(0x45))); // changed
        assert_eq!(pad.apply_pad_set(1, 6), Ok(None)); // unchanged: no edge
        pad.mmio_write(REG_IRQ_VECTOR, &0u32.to_le_bytes(), &mut ctx);
        assert_eq!(pad.apply_pad_set(1, 7), Ok(None)); // disabled again
    }

    #[test]
    fn frame_counter_write_logs_frame_mark() {
        let mut bus = MmioBus::new();
        bus.register(B, Box::new(PvPad::new())).unwrap();
        let mut l = log();
        let mut m = VecGuestMem(vec![0u8; 4]);
        let mut e = FakeEntropy(0);
        let mut q: Vec<IrqRequest> = Vec::new();
        {
            let mut ctx = DevCtx::new(777, 0x4242, &mut l, &mut m, &mut e, &mut q);
            bus.write(B + 0x1C, &3u32.to_le_bytes(), &mut ctx).unwrap();
            let mut v = [0u8; 4];
            bus.read(B + 0x1C, &mut v, &mut ctx).unwrap();
            assert_eq!(u32::from_le_bytes(v), 3);
        }
        // The FRAME_MARK record landed with the context's boundary.
        let bytes = l
            .seal(SealParams {
                end_snapshot_id: [0; 32],
                end_icount: 1000,
                end_vns: 1000,
                end_state_hash: [0; 32],
                stop_reason: 1,
            })
            .unwrap();
        let r = &bytes[256..];
        assert_eq!(r[0], KIND_FRAME_MARK);
        assert_eq!(r[1], RFLAG_AUX);
        assert_eq!(u64::from_le_bytes(r[8..16].try_into().unwrap()), 777);
        assert_eq!(u64::from_le_bytes(r[16..24].try_into().unwrap()), 0x4242);
        assert_eq!(u32::from_le_bytes(r[24..28].try_into().unwrap()), 3);
    }

    #[test]
    fn snapshot_restore_roundtrip() {
        let mut a = PvPad::new();
        a.apply_pad_set(0, 1).unwrap();
        a.apply_pad_set(3, 0xFFFF).unwrap();
        a.irq_vector = 0x50;
        a.frame_counter = 42;
        let mut section = Vec::new();
        a.snapshot(&mut section);
        assert_eq!(section.len(), SECTION_LEN);

        let mut b = PvPad::new();
        b.restore(&section, SECTION_VERSION).unwrap();
        assert_eq!(b.latch, [1, 0, 0, 0xFFFF]);
        assert_eq!(b.irq_vector, 0x50);
        assert_eq!(b.frame_counter(), 42);

        assert_eq!(b.restore(&section, 9), Err(RestoreError));
        assert_eq!(
            b.restore(&section[..20], SECTION_VERSION),
            Err(RestoreError)
        );
    }

    #[test]
    fn vector_writes_masked_unknown_offsets_zero() {
        let mut pad = PvPad::new();
        let mut l = log();
        let mut m = VecGuestMem(vec![0u8; 4]);
        let mut e = FakeEntropy(0);
        let mut q: Vec<IrqRequest> = Vec::new();
        let mut ctx = DevCtx::new(1, 0, &mut l, &mut m, &mut e, &mut q);
        pad.mmio_write(REG_IRQ_VECTOR, &0x145u32.to_le_bytes(), &mut ctx);
        let mut v = [0u8; 4];
        pad.mmio_read(REG_IRQ_VECTOR, &mut v, &mut ctx);
        assert_eq!(u32::from_le_bytes(v), 0x45);
        pad.mmio_read(0x30, &mut v, &mut ctx);
        assert_eq!(u32::from_le_bytes(v), 0);
        let mut v8 = [0xFFu8; 8];
        pad.mmio_read(0x08, &mut v8, &mut ctx);
        assert_eq!(v8, [0u8; 8]); // 8B access to 4B registers reads zeros
    }
}
