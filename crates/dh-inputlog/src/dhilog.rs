//! DHILOG v1 writer — the Phase-1 subset of API.md §3 (byte-level, normative).
//!
//! One DHILOG describes one segment; `base snapshot + DHILOG ⇒ bit-identical
//! re-execution`. This module writes the versioned 256-byte header, the
//! canonical records (PAD_SET, DEV_EVENT incl. the PIO_ANSWER detchannel
//! encoding, NET_RX) and AUX records (ENTROPY, TIMER_FIRE, SDK_EVENT,
//! FRAME_MARK), and seals the log (END record, body_hash). The validating
//! reader lives in [`crate::reader`]; the v1.0 layout is FROZEN by the
//! golden-bytes fixtures (tests/golden.rs, bead bp9). NET_TX/EPOCH_HASH
//! emission is M5; the format version field is how M5 extends without
//! breaking us.
//!
//! Code is no_std-compatible by construction (core + alloc idioms only);
//! flipping the crate to `#![no_std]` later only needs blake3's `std`
//! feature dropped.
//!
//! ORDERING CONTRACT (§3.2, deliberate): records MUST be appended in icount
//! order — one global watermark across canonical AND AUX records, because
//! file order is the normative order. Emitters that derive AUX data late
//! (e.g. a digest computed at a pause) must buffer and append it in order
//! themselves; the writer will reject regressions loudly rather than write
//! a spec-violating file.
//!
//! Header field validity (clock_den != 0 etc.) is the MachineConfig layer's
//! contract; this writer serializes the fields verbatim.

/// Format version, major.minor in the high/low byte (§3.1): 1.0.
pub const FORMAT_VERSION: u16 = 0x0100;
pub const HEADER_LEN: usize = 256;
pub const MAX_PAYLOAD: usize = 4096;
/// DEV_EVENT caller data cap: the 8-byte device_id/event_type/data_len
/// preamble lives inside the §3.2 payload, so data is bounded by 4088, not
/// 4096. (NET_RX in M5 is framed raw — payload IS the frame, ≤ 2048 — and
/// will not inherit this preamble or this limit.)
pub const MAX_DEV_EVENT_DATA: usize = MAX_PAYLOAD - 8;

pub const FLAG_SEALED: u32 = 1 << 0;
pub const FLAG_HAS_AUX: u32 = 1 << 1;
pub const FLAG_EPOCH_HASHES: u32 = 1 << 2;

// Record kinds (§3.3). The writer emits the Phase-1 subset; NET_RX,
// EPOCH_HASH and NET_TX are spec-defined v1 kinds the reader must accept.
pub const KIND_PAD_SET: u8 = 0x01;
pub const KIND_DEV_EVENT: u8 = 0x02;
pub const KIND_NET_RX: u8 = 0x03;
pub const KIND_ENTROPY: u8 = 0x40;
pub const KIND_TIMER_FIRE: u8 = 0x41;
pub const KIND_EPOCH_HASH: u8 = 0x42;
pub const KIND_SDK_EVENT: u8 = 0x43;
pub const KIND_NET_TX: u8 = 0x44;
pub const KIND_FRAME_MARK: u8 = 0x45;
pub const KIND_END: u8 = 0x7F;

/// NET_RX payload IS the raw frame, capped at 2048 (§3.3).
pub const MAX_NET_RX_FRAME: usize = 2048;

pub const RFLAG_AUX: u8 = 1 << 0;

/// detchannel device id and its DEV_EVENT event types (§3.3, frozen with v1).
pub const DEVICE_ID_DETCHANNEL: u16 = 0x0001;
pub const EVENT_RING_PUSH: u16 = 0x0001;
pub const EVENT_CONS_BUMP: u16 = 0x0002;
pub const EVENT_PIO_ANSWER: u16 = 0x0003;

/// `PAD_SET.frame_hint` when the input was not frame-scheduled.
pub const FRAME_HINT_NONE: u32 = 0xFFFF_FFFF;

/// Header fields fixed at segment start. `end_*` fields are supplied at seal
/// time; `clock_*` is the VM's fixed rational (u32 wire width, API.md §2.1).
#[derive(Clone, Debug)]
pub struct SegmentHeader {
    pub base_snapshot_id: [u8; 32],
    /// Zeros ⇒ continue the base snapshot's PRNG stream (§3.1).
    pub entropy_seed: [u8; 32],
    pub machine_config_hash: [u8; 32],
    pub clock_num: u32,
    pub clock_den: u32,
    /// The detguest-wire ENCODER FINGERPRINT (bead 4ld): a property of
    /// the writer, carried per segment in the header (NOT a device
    /// record — restore/fork segments re-attach without a CHANNEL_INIT
    /// and must still carry their writer's fingerprint). AUX SDK_EVENT
    /// digests are computed by re-encoding drained payloads; a verifier
    /// compares fingerprints before digests to detect encoder skew
    /// (HEAD-wins sibling dep). Zero ⇒ no SDK digests in this segment.
    pub encoder_fingerprint: u64,
}

/// Everything known only at the end boundary.
#[derive(Clone, Debug)]
pub struct SealParams {
    /// Zeros if no end snapshot was taken.
    pub end_snapshot_id: [u8; 32],
    pub end_icount: u64,
    pub end_vns: u64,
    pub end_state_hash: [u8; 32],
    /// Mirrors proto StopReason (END record payload).
    pub stop_reason: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteError {
    /// Payload exceeds the §3.2 limit (4096) or a layout-specific bound.
    PayloadTooLong,
    /// Records MUST be ordered by icount (§3.2).
    IcountRegressed,
    /// `seq` is u32 and exhausted.
    SeqOverflow,
}

/// Append-only DHILOG writer. Records go in in execution order; `seal`
/// consumes the writer and returns the complete, sealed byte image.
#[derive(Debug)]
pub struct LogWriter {
    buf: Vec<u8>,
    header: SegmentHeader,
    record_count: u64,
    last_icount: u64,
    has_aux: bool,
}

impl LogWriter {
    pub fn new(header: SegmentHeader) -> Self {
        Self {
            buf: vec![0u8; HEADER_LEN], // header back-filled at seal
            header,
            record_count: 0,
            last_icount: 0,
            has_aux: false,
        }
    }

    pub fn record_count(&self) -> u64 {
        self.record_count
    }

    /// First 8 bytes of BLAKE3 of `bytes`, little-endian u64 — the digest8
    /// convention shared by ENTROPY / SDK_EVENT / NET_TX AUX records.
    pub fn digest8(bytes: &[u8]) -> u64 {
        let hash = blake3::hash(bytes);
        u64::from_le_bytes(hash.as_bytes()[..8].try_into().unwrap())
    }

    // ---- canonical records ------------------------------------------------

    /// PAD_SET (§3.3): pad input applied at `icount`.
    pub fn pad_set(
        &mut self,
        icount: u64,
        boundary_rip: u64,
        port: u8,
        buttons: u32,
        frame_hint: u32,
    ) -> Result<(), WriteError> {
        let mut payload = [0u8; 12];
        payload[0] = port;
        payload[4..8].copy_from_slice(&buttons.to_le_bytes());
        payload[8..12].copy_from_slice(&frame_hint.to_le_bytes());
        self.record(KIND_PAD_SET, 0, icount, boundary_rip, &payload)
    }

    /// DEV_EVENT (§3.3): host-side device mutation that is an input to
    /// execution. `data` is the event-type-specific layout.
    pub fn dev_event(
        &mut self,
        icount: u64,
        boundary_rip: u64,
        device_id: u16,
        event_type: u16,
        data: &[u8],
    ) -> Result<(), WriteError> {
        if data.len() > MAX_DEV_EVENT_DATA {
            return Err(WriteError::PayloadTooLong);
        }
        let data_len = data.len() as u32;
        let mut payload = Vec::with_capacity(8 + data.len());
        payload.extend_from_slice(&device_id.to_le_bytes());
        payload.extend_from_slice(&event_type.to_le_bytes());
        payload.extend_from_slice(&data_len.to_le_bytes());
        payload.extend_from_slice(data);
        self.record(KIND_DEV_EVENT, 0, icount, boundary_rip, &payload)
    }

    /// NET_RX (§3.3): a received raw frame; the payload IS the frame bytes
    /// (no preamble), capped at 2048. Landed with the bp9 format freeze so
    /// the golden fixtures cover every canonical v1 kind.
    pub fn net_rx(
        &mut self,
        icount: u64,
        boundary_rip: u64,
        frame: &[u8],
    ) -> Result<(), WriteError> {
        if frame.len() > MAX_NET_RX_FRAME {
            return Err(WriteError::PayloadTooLong);
        }
        self.record(KIND_NET_RX, 0, icount, boundary_rip, frame)
    }

    /// DEV_EVENT/PIO_ANSWER (§3.3): the value returned by an `IN` detcall.
    pub fn pio_answer(
        &mut self,
        icount: u64,
        boundary_rip: u64,
        port: u16,
        value: u32,
    ) -> Result<(), WriteError> {
        let mut data = [0u8; 8];
        data[0..2].copy_from_slice(&port.to_le_bytes());
        data[4..8].copy_from_slice(&value.to_le_bytes());
        self.dev_event(
            icount,
            boundary_rip,
            DEVICE_ID_DETCHANNEL,
            EVENT_PIO_ANSWER,
            &data,
        )
    }

    // ---- AUX records --------------------------------------------------------

    /// ENTROPY (§3.3): digest of bytes served by pv-entropy.
    pub fn entropy(
        &mut self,
        icount: u64,
        boundary_rip: u64,
        len: u32,
        digest8: u64,
    ) -> Result<(), WriteError> {
        let mut payload = [0u8; 16];
        payload[0..4].copy_from_slice(&len.to_le_bytes());
        payload[8..16].copy_from_slice(&digest8.to_le_bytes());
        self.record(KIND_ENTROPY, RFLAG_AUX, icount, boundary_rip, &payload)
    }

    /// AUX NET_TX (§3.3): the guest transmitted a frame — length +
    /// digest8 of the exact frame bytes, the ENTROPY/SDK_EVENT payload
    /// convention. The frame itself never enters the log (output, not
    /// input); subscribers read it from guest RAM at the doorbell exit.
    pub fn net_tx(
        &mut self,
        icount: u64,
        boundary_rip: u64,
        len: u32,
        digest8: u64,
    ) -> Result<(), WriteError> {
        let mut payload = [0u8; 16];
        payload[0..4].copy_from_slice(&len.to_le_bytes());
        payload[8..16].copy_from_slice(&digest8.to_le_bytes());
        self.record(KIND_NET_TX, RFLAG_AUX, icount, boundary_rip, &payload)
    }

    /// TIMER_FIRE (§3.3): delivery may defer past the arm target per the
    /// §3.4 injectability rule, hence both the armed deadline and the
    /// actually-delivered icount.
    pub fn timer_fire(
        &mut self,
        icount: u64,
        boundary_rip: u64,
        vector: u8,
        armed_deadline_vns: u64,
        delivered_icount: u64,
    ) -> Result<(), WriteError> {
        let mut payload = [0u8; 20];
        payload[0] = vector;
        payload[4..12].copy_from_slice(&armed_deadline_vns.to_le_bytes());
        payload[12..20].copy_from_slice(&delivered_icount.to_le_bytes());
        self.record(KIND_TIMER_FIRE, RFLAG_AUX, icount, boundary_rip, &payload)
    }

    /// SDK_EVENT (§3.3): digest of a guest→host SDK event; payload bytes
    /// live in the gRPC stream, not the log.
    pub fn sdk_event(
        &mut self,
        icount: u64,
        boundary_rip: u64,
        stream: u16,
        len: u32,
        digest8: u64,
    ) -> Result<(), WriteError> {
        let mut payload = [0u8; 16];
        payload[0..2].copy_from_slice(&stream.to_le_bytes());
        payload[4..8].copy_from_slice(&len.to_le_bytes());
        payload[8..16].copy_from_slice(&digest8.to_le_bytes());
        self.record(KIND_SDK_EVENT, RFLAG_AUX, icount, boundary_rip, &payload)
    }

    /// FRAME_MARK (§3.3): absolute FRAME_COUNTER value F at this segment-
    /// relative icount — the frame table resolving at_frame scheduling.
    pub fn frame_mark(
        &mut self,
        icount: u64,
        boundary_rip: u64,
        frame_index: u32,
    ) -> Result<(), WriteError> {
        let mut payload = [0u8; 8];
        payload[0..4].copy_from_slice(&frame_index.to_le_bytes());
        self.record(KIND_FRAME_MARK, RFLAG_AUX, icount, boundary_rip, &payload)
    }

    // ---- sealing ------------------------------------------------------------

    /// Write the END record, back-fill the header, compute `body_hash`, and
    /// return the complete sealed image. END is always the last record and
    /// always present in sealed logs (§3.3).
    pub fn seal(mut self, params: SealParams) -> Result<Vec<u8>, WriteError> {
        let mut payload = [0u8; 40];
        payload[0] = params.stop_reason;
        payload[8..40].copy_from_slice(&params.end_state_hash);
        // §3.3 (normative): END carries rflags.AUX = 1 and boundary_rip = 0;
        // a minimal replayer that skips AUX records still terminates via the
        // header's end_icount/end_state_hash. END does NOT count toward
        // flags.HAS_AUX — snapshot has_aux before appending it.
        let has_aux = self.has_aux;
        self.record(KIND_END, RFLAG_AUX, params.end_icount, 0, &payload)?;
        self.has_aux = has_aux;

        let h = &self.header;
        let buf = &mut self.buf;
        buf[0..6].copy_from_slice(b"DHILOG");
        buf[6..8].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
        buf[8..12].copy_from_slice(&(HEADER_LEN as u32).to_le_bytes());
        let flags = FLAG_SEALED | if self.has_aux { FLAG_HAS_AUX } else { 0 };
        buf[12..16].copy_from_slice(&flags.to_le_bytes());
        buf[16..48].copy_from_slice(&h.base_snapshot_id);
        buf[48..80].copy_from_slice(&params.end_snapshot_id);
        buf[80..112].copy_from_slice(&h.entropy_seed);
        buf[112..144].copy_from_slice(&h.machine_config_hash);
        buf[144..148].copy_from_slice(&h.clock_num.to_le_bytes());
        buf[148..152].copy_from_slice(&h.clock_den.to_le_bytes());
        buf[152..160].copy_from_slice(&self.record_count.to_le_bytes());
        buf[160..168].copy_from_slice(&params.end_icount.to_le_bytes());
        buf[168..176].copy_from_slice(&params.end_vns.to_le_bytes());
        buf[176..208].copy_from_slice(&params.end_state_hash);
        buf[240..248].copy_from_slice(&h.encoder_fingerprint.to_le_bytes());
        let body_hash = blake3::hash(&buf[HEADER_LEN..]);
        buf[208..240].copy_from_slice(body_hash.as_bytes());
        // [248..256) reserved, already zeros.
        Ok(self.buf)
    }

    // ---- framing ------------------------------------------------------------

    fn record(
        &mut self,
        kind: u8,
        rflags: u8,
        icount: u64,
        boundary_rip: u64,
        payload: &[u8],
    ) -> Result<(), WriteError> {
        if payload.len() > MAX_PAYLOAD {
            return Err(WriteError::PayloadTooLong);
        }
        if self.record_count > 0 && icount < self.last_icount {
            return Err(WriteError::IcountRegressed);
        }
        // seq == record index; both checked before any bytes are written so
        // a failed append leaves the buffer untouched.
        let seq: u32 = self
            .record_count
            .try_into()
            .map_err(|_| WriteError::SeqOverflow)?;

        self.buf.push(kind);
        self.buf.push(rflags);
        self.buf
            .extend_from_slice(&(payload.len() as u16).to_le_bytes());
        self.buf.extend_from_slice(&seq.to_le_bytes());
        self.buf.extend_from_slice(&icount.to_le_bytes());
        self.buf.extend_from_slice(&boundary_rip.to_le_bytes());
        self.buf.extend_from_slice(payload);
        // Zero-pad to the next 8-byte multiple (§3.2).
        let pad = (8 - payload.len() % 8) % 8;
        self.buf.extend_from_slice(&[0u8; 8][..pad]);

        self.record_count += 1;
        self.last_icount = icount;
        if rflags & RFLAG_AUX != 0 {
            self.has_aux = true;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> SegmentHeader {
        SegmentHeader {
            base_snapshot_id: [0xAA; 32],
            entropy_seed: [0xBB; 32],
            machine_config_hash: [0xCC; 32],
            clock_num: 1,
            clock_den: 1,
            encoder_fingerprint: 0,
        }
    }

    fn seal_params() -> SealParams {
        SealParams {
            end_snapshot_id: [0xDD; 32],
            end_icount: 5000,
            end_vns: 5000,
            end_state_hash: [0xEE; 32],
            stop_reason: 2,
        }
    }

    #[test]
    fn header_layout_is_spec_exact() {
        let log = LogWriter::new(header()).seal(seal_params()).unwrap();
        assert_eq!(&log[0..6], b"DHILOG");
        assert_eq!(&log[6..8], &[0x00, 0x01]); // 0x0100 LE: minor, major
        assert_eq!(u32::from_le_bytes(log[8..12].try_into().unwrap()), 256);
        let flags = u32::from_le_bytes(log[12..16].try_into().unwrap());
        // END alone does not set HAS_AUX (§3.1 normative ruling).
        assert_eq!(flags, FLAG_SEALED);
        assert_eq!(&log[16..48], &[0xAA; 32]);
        assert_eq!(&log[48..80], &[0xDD; 32]);
        assert_eq!(&log[80..112], &[0xBB; 32]);
        assert_eq!(&log[112..144], &[0xCC; 32]);
        assert_eq!(u32::from_le_bytes(log[144..148].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(log[148..152].try_into().unwrap()), 1);
        assert_eq!(u64::from_le_bytes(log[152..160].try_into().unwrap()), 1); // END only
        assert_eq!(u64::from_le_bytes(log[160..168].try_into().unwrap()), 5000);
        assert_eq!(u64::from_le_bytes(log[168..176].try_into().unwrap()), 5000);
        assert_eq!(&log[176..208], &[0xEE; 32]);
        let body_hash = blake3::hash(&log[256..]);
        assert_eq!(&log[208..240], body_hash.as_bytes());
        assert_eq!(&log[240..256], &[0u8; 16]); // reserved-means-zero
    }

    #[test]
    fn record_framing_and_padding() {
        let mut w = LogWriter::new(header());
        w.pad_set(100, 0x1000, 0, 0xDEAD_BEEF, FRAME_HINT_NONE)
            .unwrap();
        let log = w.seal(seal_params()).unwrap();

        let r = &log[256..];
        assert_eq!(r[0], KIND_PAD_SET);
        assert_eq!(r[1], 0); // canonical
        assert_eq!(u16::from_le_bytes(r[2..4].try_into().unwrap()), 12);
        assert_eq!(u32::from_le_bytes(r[4..8].try_into().unwrap()), 0); // seq
        assert_eq!(u64::from_le_bytes(r[8..16].try_into().unwrap()), 100);
        assert_eq!(u64::from_le_bytes(r[16..24].try_into().unwrap()), 0x1000);
        // payload: port, pad3, buttons, frame_hint
        assert_eq!(r[24], 0);
        assert_eq!(&r[25..28], &[0, 0, 0]);
        assert_eq!(
            u32::from_le_bytes(r[28..32].try_into().unwrap()),
            0xDEAD_BEEF
        );
        assert_eq!(
            u32::from_le_bytes(r[32..36].try_into().unwrap()),
            FRAME_HINT_NONE
        );
        // 12-byte payload pads to 16; next record (END) starts 24+16 in.
        assert_eq!(&r[36..40], &[0u8; 4]);
        assert_eq!(r[40], KIND_END);
        assert_eq!(u32::from_le_bytes(r[44..48].try_into().unwrap()), 1); // END seq
    }

    #[test]
    fn pio_answer_dev_event_encoding() {
        let mut w = LogWriter::new(header());
        w.pio_answer(7, 0x2000, 0xD370, 0x1234_5678).unwrap();
        let log = w.seal(seal_params()).unwrap();
        let r = &log[256..];
        assert_eq!(r[0], KIND_DEV_EVENT);
        let p = &r[24..];
        assert_eq!(
            u16::from_le_bytes(p[0..2].try_into().unwrap()),
            DEVICE_ID_DETCHANNEL
        );
        assert_eq!(
            u16::from_le_bytes(p[2..4].try_into().unwrap()),
            EVENT_PIO_ANSWER
        );
        assert_eq!(u32::from_le_bytes(p[4..8].try_into().unwrap()), 8); // data_len
        assert_eq!(u16::from_le_bytes(p[8..10].try_into().unwrap()), 0xD370);
        assert_eq!(
            u32::from_le_bytes(p[12..16].try_into().unwrap()),
            0x1234_5678
        );
    }

    #[test]
    fn aux_record_layouts() {
        let mut w = LogWriter::new(header());
        w.entropy(10, 0, 64, 0x0102_0304_0506_0708).unwrap();
        w.timer_fire(20, 0, 0x30, 19_000, 20).unwrap();
        w.sdk_event(30, 0, 5, 100, 0x1111_2222_3333_4444).unwrap();
        w.frame_mark(40, 0, 7).unwrap();
        let log = w.seal(seal_params()).unwrap();

        let mut off = 256;
        let rec = |off: usize, log: &[u8]| -> (u8, u8, usize) {
            let kind = log[off];
            let rflags = log[off + 1];
            let plen = u16::from_le_bytes(log[off + 2..off + 4].try_into().unwrap()) as usize;
            (kind, rflags, 24 + plen + (8 - plen % 8) % 8)
        };
        for expected in [
            KIND_ENTROPY,
            KIND_TIMER_FIRE,
            KIND_SDK_EVENT,
            KIND_FRAME_MARK,
            KIND_END,
        ] {
            let (kind, rflags, sz) = rec(off, &log);
            assert_eq!(kind, expected);
            assert_eq!(rflags, RFLAG_AUX);
            off += sz;
        }
        assert_eq!(off, log.len());
    }

    #[test]
    fn ordering_enforced_and_equal_icount_allowed() {
        let mut w = LogWriter::new(header());
        w.pad_set(100, 0, 0, 1, FRAME_HINT_NONE).unwrap();
        w.pad_set(100, 0, 0, 2, FRAME_HINT_NONE).unwrap(); // same icount: ok, seq orders
        assert_eq!(
            w.pad_set(99, 0, 0, 3, FRAME_HINT_NONE),
            Err(WriteError::IcountRegressed)
        );
    }

    #[test]
    fn payload_limit_enforced() {
        let mut w = LogWriter::new(header());
        // dev_event data is capped at 4088 (8-byte preamble inside the
        // 4096 payload limit): the boundary fits, one more byte does not.
        let max = vec![0u8; MAX_DEV_EVENT_DATA];
        w.dev_event(1, 0, 1, 1, &max).unwrap();
        let over = vec![0u8; MAX_DEV_EVENT_DATA + 1];
        assert_eq!(
            w.dev_event(2, 0, 1, 1, &over),
            Err(WriteError::PayloadTooLong)
        );
    }

    #[test]
    fn aux_flag_set_only_by_real_aux_records() {
        let mut w = LogWriter::new(header());
        w.frame_mark(10, 0, 1).unwrap();
        let log = w.seal(seal_params()).unwrap();
        let flags = u32::from_le_bytes(log[12..16].try_into().unwrap());
        assert_eq!(flags, FLAG_SEALED | FLAG_HAS_AUX);
    }

    #[test]
    fn empty_log_is_just_header_plus_end() {
        let log = LogWriter::new(header()).seal(seal_params()).unwrap();
        assert_eq!(log.len(), 256 + 24 + 40); // END: 40-byte payload, already 8-aligned
        let body_hash = blake3::hash(&log[256..]);
        assert_eq!(&log[208..240], body_hash.as_bytes());
    }

    #[test]
    fn digest8_is_blake3_prefix_le() {
        let d = LogWriter::digest8(b"hello");
        let h = blake3::hash(b"hello");
        assert_eq!(d, u64::from_le_bytes(h.as_bytes()[..8].try_into().unwrap()));
    }

    #[test]
    fn deterministic_byte_identical_rebuild() {
        let build = || {
            let mut w = LogWriter::new(header());
            w.pio_answer(7, 0x2000, 0xD370, 42).unwrap();
            w.frame_mark(50, 0x3000, 1).unwrap();
            w.pad_set(60, 0x4000, 0, 3, 1).unwrap();
            w.seal(seal_params()).unwrap()
        };
        assert_eq!(build(), build());
    }
}
