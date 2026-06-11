//! pv-net loopback (ARCH §6.7, window 0xD000_5000): TX is an OUTPUT
//! (AUX `NET_TX` digest record); RX is an INPUT that happens only when a
//! canonical `NET_RX` log record lands at its icount. No host networking
//! anywhere — the loopback wiring (TX frame re-landed as NET_RX) is RUN
//! CONTROL's, not this device's.
//!
//! Same one-deep register style as pv-blk: the guest writes TX_BUF_GPA /
//! TX_LEN then rings TX_DOORBELL; completion is synchronous inside the
//! MMIO-write emulation (TX_STATUS valid when the exit returns). The
//! doorbell logs the AUX `NET_TX` record (length + digest8 — the §3.3
//! ENTROPY/SDK_EVENT convention) and nothing else: the device buffers NO
//! frame. Subscribers (run control's loopback path, y78) re-read the
//! frame from guest RAM through the still-live TX regs at the very exit
//! that rang the doorbell — so the NETL section is pure registers and
//! the §4 "pending-RX state must be empty at snapshot" rule holds BY
//! CONSTRUCTION: there is no queue to be non-empty. RX delivery
//! (`apply_net_rx`, the PAD_SET-style run-control entry point) copies
//! the recorded frame into the guest-published RX buffer immediately and
//! returns the edge vector to inject per §3.4.
//!
//! Contracts run control inherits: TX_STATUS is STICKY (valid until the
//! next doorbell — reg writes do not reset it); TX frames must be
//! drained PER EXIT via [`PvNet::tx_regs`]; back-to-back NET_RX
//! deliveries overwrite the RX buffer and RX_LEN silently — frame loss
//! is the GUEST's pacing problem and identical in record and replay.

use crate::ctx::{DevCtx, GuestMem};
use crate::{DetDevice, RestoreError};
use dh_inputlog::dhilog::LogWriter;

pub const PV_NET_BASE: u64 = 0xD000_5000;
/// 0x0007 — pinned by dh-snapshot's device-id↔tag map (NETL); the next
/// free id after debug-serial 0x0006.
pub const DEVICE_ID_PV_NET: u16 = 0x0007;

pub const REG_TX_BUF_GPA: u64 = 0x08; // 8B RW
pub const REG_TX_LEN: u64 = 0x10; // 4B RW
pub const REG_TX_DOORBELL: u64 = 0x14; // 4B WO
pub const REG_TX_STATUS: u64 = 0x18; // 4B RO
pub const REG_RX_BUF_GPA: u64 = 0x20; // 8B RW (guest-published buffer)
pub const REG_RX_CAP: u64 = 0x28; // 4B RW (buffer capacity, bytes)
pub const REG_RX_LEN: u64 = 0x2C; // 4B RW (delivered length; guest clears)
pub const REG_RX_VECTOR: u64 = 0x30; // 4B RW (edge vector; 0 = disabled)

pub const STATUS_IDLE: u32 = 0;
pub const STATUS_OK: u32 = 1;
pub const STATUS_FAULT: u32 = 2;

/// Frame cap — mirrors `dh_inputlog::dhilog::MAX_NET_RX_FRAME` (§3.3:
/// the NET_RX payload IS the raw frame, ≤ 2048).
pub const MAX_FRAME: u32 = 2048;

const SECTION_VERSION: u16 = 1;
/// tx_buf_gpa u64 ‖ tx_len u32 ‖ tx_status u32 ‖ rx_buf_gpa u64 ‖
/// rx_cap u32 ‖ rx_len u32 ‖ rx_vector u32 (36 bytes). Registers only —
/// see the module doc for why no pending state can exist.
const SECTION_LEN: usize = 8 + 4 + 4 + 8 + 4 + 4 + 4;

/// `apply_net_rx` failure — corrupt or hostile log input, or a guest
/// that published an unusable RX buffer. Run control faults the slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetRxError {
    /// Frame exceeds MAX_FRAME or the guest-published RX capacity.
    FrameTooBig,
    /// The guest published no RX buffer (RX_BUF_GPA = 0).
    NoRxBuffer,
    /// Copying into the RX buffer faulted (GPA range not guest RAM).
    MemFault,
}

pub struct PvNet {
    tx_buf_gpa: u64,
    tx_len: u32,
    tx_status: u32,
    rx_buf_gpa: u64,
    rx_cap: u32,
    rx_len: u32,
    rx_vector: u32,
}

impl Default for PvNet {
    fn default() -> Self {
        Self::new()
    }
}

impl PvNet {
    pub fn new() -> Self {
        Self {
            tx_buf_gpa: 0,
            tx_len: 0,
            tx_status: STATUS_IDLE,
            rx_buf_gpa: 0,
            rx_cap: 0,
            rx_len: 0,
            rx_vector: 0,
        }
    }

    fn doorbell(&mut self, ctx: &mut DevCtx) {
        if self.tx_len == 0 || self.tx_len > MAX_FRAME {
            self.tx_status = STATUS_FAULT;
            return;
        }
        let mut frame = vec![0u8; self.tx_len as usize];
        if ctx.mem.read(self.tx_buf_gpa, &mut frame).is_err() {
            self.tx_status = STATUS_FAULT;
            return;
        }
        let digest8 = LogWriter::digest8(&frame);
        ctx.log_net_tx(self.tx_len, digest8);
        self.tx_status = STATUS_OK;
    }

    /// Run control's frame-recovery seam (y78): the TX registers as of
    /// the doorbell exit — `(tx_buf_gpa, tx_len)`. The loopback path
    /// re-reads the frame bytes from guest RAM through these AT THE SAME
    /// EXIT that rang the doorbell (drain per exit — the guest may
    /// overwrite its buffer the moment it runs again), then lands them
    /// as a canonical NET_RX record.
    pub fn tx_regs(&self) -> (u64, u32) {
        (self.tx_buf_gpa, self.tx_len)
    }

    /// Run-control entry point: a canonical NET_RX record landed at its
    /// icount. Copies the frame into the guest-published RX buffer, sets
    /// RX_LEN, and returns the edge vector to inject (per §3.4) if the
    /// guest enabled one. Direct state access, not MMIO — mirrors
    /// `PvPad::apply_pad_set`.
    pub fn apply_net_rx(
        &mut self,
        frame: &[u8],
        mem: &mut dyn GuestMem,
    ) -> Result<Option<u8>, NetRxError> {
        // ABI: RX_BUF_GPA = 0 means "unpublished" (the reset default).
        // GPA 0 is real guest RAM, so a guest CANNOT publish an RX buffer
        // at page zero — a deliberate reservation this device makes (the
        // pv-entropy TX-direction doorbell has no such sentinel; its
        // buf_gpa is guest-OUTPUT, not a host-write target gate).
        // Checked FIRST so an unpublished buffer is never masked by a
        // size error.
        if self.rx_buf_gpa == 0 {
            return Err(NetRxError::NoRxBuffer);
        }
        let len = u32::try_from(frame.len()).map_err(|_| NetRxError::FrameTooBig)?;
        // len == 0 rejected here while the DHILOG codec accepts empty
        // NET_RX records — the cross-layer zero-length policy is its own
        // bead (filed iteration 85); until it lands, recording never
        // produces an empty frame so the asymmetry is unreachable.
        if len == 0 || len > MAX_FRAME || len > self.rx_cap {
            return Err(NetRxError::FrameTooBig);
        }
        mem.write(self.rx_buf_gpa, frame)
            .map_err(|_| NetRxError::MemFault)?;
        self.rx_len = len;
        Ok((self.rx_vector != 0).then_some(self.rx_vector as u8))
    }
}

impl DetDevice for PvNet {
    fn device_id(&self) -> u16 {
        DEVICE_ID_PV_NET
    }

    fn section_version(&self) -> u16 {
        SECTION_VERSION
    }

    fn mmio_read(&mut self, off: u64, data: &mut [u8], _ctx: &mut DevCtx) {
        match (off, data.len()) {
            (REG_TX_BUF_GPA, 8) => data.copy_from_slice(&self.tx_buf_gpa.to_le_bytes()),
            (REG_TX_LEN, 4) => data.copy_from_slice(&self.tx_len.to_le_bytes()),
            (REG_TX_STATUS, 4) => data.copy_from_slice(&self.tx_status.to_le_bytes()),
            (REG_RX_BUF_GPA, 8) => data.copy_from_slice(&self.rx_buf_gpa.to_le_bytes()),
            (REG_RX_CAP, 4) => data.copy_from_slice(&self.rx_cap.to_le_bytes()),
            (REG_RX_LEN, 4) => data.copy_from_slice(&self.rx_len.to_le_bytes()),
            (REG_RX_VECTOR, 4) => data.copy_from_slice(&self.rx_vector.to_le_bytes()),
            // TX_DOORBELL is write-only; everything else unknown.
            _ => data.fill(0),
        }
    }

    fn mmio_write(&mut self, off: u64, data: &[u8], ctx: &mut DevCtx) {
        match (off, data.len()) {
            (REG_TX_BUF_GPA, 8) => self.tx_buf_gpa = u64::from_le_bytes(data.try_into().unwrap()),
            (REG_TX_LEN, 4) => self.tx_len = u32::from_le_bytes(data.try_into().unwrap()),
            (REG_TX_DOORBELL, 4) => self.doorbell(ctx),
            (REG_RX_BUF_GPA, 8) => self.rx_buf_gpa = u64::from_le_bytes(data.try_into().unwrap()),
            (REG_RX_CAP, 4) => self.rx_cap = u32::from_le_bytes(data.try_into().unwrap()),
            // Guest acks a delivery by clearing (any write allowed —
            // deterministic; the canonical record stream is the truth).
            (REG_RX_LEN, 4) => self.rx_len = u32::from_le_bytes(data.try_into().unwrap()),
            // Masked to u8 on WRITE so read-back, snapshot, and the
            // injected vector always agree (§3.4 vectors are 0-255).
            (REG_RX_VECTOR, 4) => {
                self.rx_vector = u32::from_le_bytes(data.try_into().unwrap()) & 0xFF;
            }
            // RO registers and unknown offsets: writes ignored.
            _ => {}
        }
    }

    fn snapshot(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.tx_buf_gpa.to_le_bytes());
        out.extend_from_slice(&self.tx_len.to_le_bytes());
        out.extend_from_slice(&self.tx_status.to_le_bytes());
        out.extend_from_slice(&self.rx_buf_gpa.to_le_bytes());
        out.extend_from_slice(&self.rx_cap.to_le_bytes());
        out.extend_from_slice(&self.rx_len.to_le_bytes());
        out.extend_from_slice(&self.rx_vector.to_le_bytes());
    }

    fn restore(&mut self, bytes: &[u8], sec_version: u16) -> Result<(), RestoreError> {
        if sec_version != SECTION_VERSION || bytes.len() != SECTION_LEN {
            return Err(RestoreError);
        }
        self.tx_buf_gpa = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
        self.tx_len = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        self.tx_status = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        self.rx_buf_gpa = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
        self.rx_cap = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
        self.rx_len = u32::from_le_bytes(bytes[28..32].try_into().unwrap());
        self.rx_vector = u32::from_le_bytes(bytes[32..36].try_into().unwrap());
        Ok(())
    }
}

#[cfg(test)]
impl PvNet {
    fn snapshot_bytes(&self) -> Vec<u8> {
        let mut v = Vec::new();
        self.snapshot(&mut v);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::test_support::FakeEntropy;
    use crate::ctx::VecGuestMem;
    use dh_inputlog::dhilog::{LogWriter, SegmentHeader};
    use dh_inputlog::reader::LogReader;

    fn harness() -> (LogWriter, VecGuestMem, FakeEntropy, Vec<crate::IrqRequest>) {
        (
            LogWriter::new(SegmentHeader {
                base_snapshot_id: [0; 32],
                entropy_seed: [0; 32],
                machine_config_hash: [0; 32],
                clock_num: 1,
                clock_den: 1,
                encoder_fingerprint: 0,
            }),
            VecGuestMem(vec![0u8; 8192]),
            FakeEntropy(0),
            Vec::new(),
        )
    }

    fn ctx<'a>(
        log: &'a mut LogWriter,
        mem: &'a mut VecGuestMem,
        ent: &'a mut FakeEntropy,
        irqs: &'a mut Vec<crate::IrqRequest>,
    ) -> DevCtx<'a> {
        DevCtx::new(700, 0x1234, log, mem, ent, irqs)
    }

    #[test]
    fn device_id_is_the_dhsnap_pinned_0x0007() {
        assert_eq!(DEVICE_ID_PV_NET, 0x0007);
        assert_eq!(PvNet::new().device_id(), 0x0007);
    }

    #[test]
    fn tx_doorbell_logs_the_aux_net_tx_digest_and_completes_ok() {
        let (mut log, mut mem, mut ent, mut irqs) = harness();
        let frame = [0xAB; 64];
        mem.0[0x100..0x140].copy_from_slice(&frame);
        let mut dev = PvNet::new();
        {
            let mut c = ctx(&mut log, &mut mem, &mut ent, &mut irqs);
            dev.mmio_write(REG_TX_BUF_GPA, &0x100u64.to_le_bytes(), &mut c);
            dev.mmio_write(REG_TX_LEN, &64u32.to_le_bytes(), &mut c);
            dev.mmio_write(REG_TX_DOORBELL, &1u32.to_le_bytes(), &mut c);
            let mut st = [0u8; 4];
            dev.mmio_read(REG_TX_STATUS, &mut st, &mut c);
            assert_eq!(u32::from_le_bytes(st), STATUS_OK);
            assert!(c.log_fault().is_none());
        }
        // The AUX record carries (len, digest8 of the exact guest bytes).
        let sealed = log
            .seal(dh_inputlog::dhilog::SealParams {
                end_snapshot_id: [0; 32],
                end_icount: 1000,
                end_vns: 1000,
                end_state_hash: [0; 32],
                stop_reason: 0,
            })
            .unwrap();
        let r = LogReader::parse(&sealed).unwrap();
        let aux: Vec<_> = r
            .aux()
            .filter(|rec| matches!(rec.body(), dh_inputlog::reader::RecordBody::NetTx { .. }))
            .collect();
        assert_eq!(aux.len(), 1, "exactly one NET_TX record");
        match aux[0].body() {
            dh_inputlog::reader::RecordBody::NetTx { len, digest8 } => {
                assert_eq!(len, 64);
                assert_eq!(digest8, LogWriter::digest8(&frame));
            }
            other => panic!("wrong record: {other:?}"),
        }
    }

    #[test]
    fn tx_faults_are_loud_and_logged_nothing() {
        let (mut log, mut mem, mut ent, mut irqs) = harness();
        let mut dev = PvNet::new();
        let mut c = ctx(&mut log, &mut mem, &mut ent, &mut irqs);
        // Zero length.
        dev.mmio_write(REG_TX_DOORBELL, &1u32.to_le_bytes(), &mut c);
        let mut st = [0u8; 4];
        dev.mmio_read(REG_TX_STATUS, &mut st, &mut c);
        assert_eq!(u32::from_le_bytes(st), STATUS_FAULT);
        // Oversize.
        dev.mmio_write(REG_TX_LEN, &(MAX_FRAME + 1).to_le_bytes(), &mut c);
        dev.mmio_write(REG_TX_DOORBELL, &1u32.to_le_bytes(), &mut c);
        dev.mmio_read(REG_TX_STATUS, &mut st, &mut c);
        assert_eq!(u32::from_le_bytes(st), STATUS_FAULT);
        // Unmapped buffer.
        dev.mmio_write(REG_TX_LEN, &16u32.to_le_bytes(), &mut c);
        dev.mmio_write(REG_TX_BUF_GPA, &0xFFFF_0000u64.to_le_bytes(), &mut c);
        dev.mmio_write(REG_TX_DOORBELL, &1u32.to_le_bytes(), &mut c);
        dev.mmio_read(REG_TX_STATUS, &mut st, &mut c);
        assert_eq!(u32::from_le_bytes(st), STATUS_FAULT);
    }

    #[test]
    fn apply_net_rx_copies_sets_len_and_returns_the_enabled_vector() {
        let (mut log, mut mem, mut ent, mut irqs) = harness();
        let mut dev = PvNet::new();
        {
            let mut c = ctx(&mut log, &mut mem, &mut ent, &mut irqs);
            dev.mmio_write(REG_RX_BUF_GPA, &0x200u64.to_le_bytes(), &mut c);
            dev.mmio_write(REG_RX_CAP, &128u32.to_le_bytes(), &mut c);
            dev.mmio_write(REG_RX_VECTOR, &0x41u32.to_le_bytes(), &mut c);
        }
        let frame = [0x77u8; 48];
        let v = dev.apply_net_rx(&frame, &mut mem).unwrap();
        assert_eq!(v, Some(0x41));
        assert_eq!(&mem.0[0x200..0x230], &frame[..]);
        let (mut log2, mut mem2, mut ent2, mut irqs2) = harness();
        let mut c = ctx(&mut log2, &mut mem2, &mut ent2, &mut irqs2);
        let mut len = [0u8; 4];
        dev.mmio_read(REG_RX_LEN, &mut len, &mut c);
        assert_eq!(u32::from_le_bytes(len), 48);

        // Vector disabled: delivery still lands, no injection.
        let mut dev2 = PvNet::new();
        {
            let mut c2 = ctx(&mut log2, &mut mem2, &mut ent2, &mut irqs2);
            dev2.mmio_write(REG_RX_BUF_GPA, &0x300u64.to_le_bytes(), &mut c2);
            dev2.mmio_write(REG_RX_CAP, &128u32.to_le_bytes(), &mut c2);
        }
        assert_eq!(dev2.apply_net_rx(&frame, &mut mem2).unwrap(), None);
    }

    #[test]
    fn apply_net_rx_rejects_bad_frames_loudly() {
        let (_log, mut mem, _ent, _irqs) = harness();
        let mut dev = PvNet::new();
        // No RX buffer published — reported as such, never masked by
        // the (also-zero) cap.
        assert_eq!(
            dev.apply_net_rx(&[1, 2, 3], &mut mem),
            Err(NetRxError::NoRxBuffer)
        );
        // Buffer published, frame over cap.
        let (mut log, mut mem, mut ent, mut irqs) = harness();
        {
            let mut c = ctx(&mut log, &mut mem, &mut ent, &mut irqs);
            dev.mmio_write(REG_RX_BUF_GPA, &0x200u64.to_le_bytes(), &mut c);
            dev.mmio_write(REG_RX_CAP, &8u32.to_le_bytes(), &mut c);
        }
        assert_eq!(
            dev.apply_net_rx(&[0; 9], &mut mem),
            Err(NetRxError::FrameTooBig)
        );
        // Empty frame.
        assert_eq!(
            dev.apply_net_rx(&[], &mut mem),
            Err(NetRxError::FrameTooBig)
        );
        // Cap OK but buffer unmapped.
        let (mut log3, mut mem3, mut ent3, mut irqs3) = harness();
        let mut dev3 = PvNet::new();
        {
            let mut c = ctx(&mut log3, &mut mem3, &mut ent3, &mut irqs3);
            dev3.mmio_write(REG_RX_BUF_GPA, &0xFFFF_0000u64.to_le_bytes(), &mut c);
            dev3.mmio_write(REG_RX_CAP, &64u32.to_le_bytes(), &mut c);
        }
        assert_eq!(
            dev3.apply_net_rx(&[1; 16], &mut mem3),
            Err(NetRxError::MemFault)
        );
    }

    #[test]
    fn bus_dispatch_at_the_canonical_window_works() {
        let (mut log, mut mem, mut ent, mut irqs) = harness();
        let frame = [0x3C; 32];
        mem.0[0x400..0x420].copy_from_slice(&frame);
        let mut bus = crate::MmioBus::new();
        bus.register(PV_NET_BASE, Box::new(PvNet::new())).unwrap();
        let mut c = ctx(&mut log, &mut mem, &mut ent, &mut irqs);
        bus.write(
            PV_NET_BASE + REG_TX_BUF_GPA,
            &0x400u64.to_le_bytes(),
            &mut c,
        )
        .unwrap();
        bus.write(PV_NET_BASE + REG_TX_LEN, &32u32.to_le_bytes(), &mut c)
            .unwrap();
        bus.write(PV_NET_BASE + REG_TX_DOORBELL, &1u32.to_le_bytes(), &mut c)
            .unwrap();
        let mut st = [0u8; 4];
        bus.read(PV_NET_BASE + REG_TX_STATUS, &mut st, &mut c)
            .unwrap();
        assert_eq!(u32::from_le_bytes(st), STATUS_OK);
    }

    #[test]
    fn snapshot_restore_roundtrip_is_byte_identical() {
        let (mut log, mut mem, mut ent, mut irqs) = harness();
        let mut a = PvNet::new();
        {
            let mut c = ctx(&mut log, &mut mem, &mut ent, &mut irqs);
            a.mmio_write(REG_TX_BUF_GPA, &0xAAAAu64.to_le_bytes(), &mut c);
            a.mmio_write(REG_TX_LEN, &99u32.to_le_bytes(), &mut c);
            a.mmio_write(REG_RX_BUF_GPA, &0xBBBBu64.to_le_bytes(), &mut c);
            a.mmio_write(REG_RX_CAP, &256u32.to_le_bytes(), &mut c);
            a.mmio_write(REG_RX_VECTOR, &0x55u32.to_le_bytes(), &mut c);
        }
        let mut sect = Vec::new();
        a.snapshot(&mut sect);
        assert_eq!(sect.len(), SECTION_LEN);

        let mut b = PvNet::new();
        b.restore(&sect, SECTION_VERSION).unwrap();
        let mut sect_b = Vec::new();
        b.snapshot(&mut sect_b);
        assert_eq!(sect, sect_b, "restore then snapshot must be identity");

        // Wrong version / wrong length are refused.
        assert_eq!(b.restore(&sect, 2), Err(RestoreError));
        assert_eq!(b.restore(&sect[1..], SECTION_VERSION), Err(RestoreError));
    }

    #[test]
    fn unknown_offsets_read_zero_and_ignore_writes() {
        let (mut log, mut mem, mut ent, mut irqs) = harness();
        let mut dev = PvNet::new();
        let mut c = ctx(&mut log, &mut mem, &mut ent, &mut irqs);
        let mut buf = [0xFFu8; 4];
        dev.mmio_read(0x40, &mut buf, &mut c);
        assert_eq!(buf, [0; 4]);
        dev.mmio_write(0x40, &[1, 2, 3, 4], &mut c);
        let mut sect = Vec::new();
        dev.snapshot(&mut sect);
        assert_eq!(sect, PvNet::new().snapshot_bytes());
    }
}
