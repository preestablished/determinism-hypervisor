//! DHILOG v1 reader — the validating read side of API.md §3 (normative).
//!
//! `LogReader::parse` is a TOTAL decoder over untrusted bytes: every input
//! yields `Ok` or a `ReadError`, never a panic. It runs the full validation
//! battery up front — header shape (incl. the reserved-means-zero rule),
//! `body_hash`, per-record framing, the (`icount`, `seq`) watermark, known
//! payload layouts, flag/record consistency, and END semantics — so the
//! iterators it hands out are infallible views over already-validated bytes.
//!
//! AUX SKIPPING CONTRACT (§3.3/§3.4): records with `rflags.AUX = 1` are
//! derived data; a minimal replayer iterates [`LogReader::canonical`] and may
//! ignore AUX entirely. Unknown AUX kinds are therefore accepted (that is how
//! later v1.x minors extend the format) and surface as [`RecordBody::Unknown`];
//! unknown CANONICAL kinds are inputs a replayer cannot apply, so parsing
//! rejects them (`UnknownCanonicalKind`).
//!
//! Like the writer, the sealed replay path is no_std-compatible by
//! construction (core idioms only; iteration is allocation-free over
//! borrowed payloads). The inspection-only crash-artifact path allocates a
//! bounded prefix of validated records so it can stop cleanly at the first
//! corruption without exposing a replay-capable `LogReader`.

use crate::dhilog::{
    FLAG_EPOCH_HASHES, FLAG_HAS_AUX, FLAG_SEALED, HEADER_LEN, KIND_DEV_EVENT, KIND_END,
    KIND_ENTROPY, KIND_EPOCH_HASH, KIND_FRAME_MARK, KIND_NET_RX, KIND_NET_TX, KIND_PAD_SET,
    KIND_SDK_EVENT, KIND_TIMER_FIRE, MAX_NET_RX_FRAME, MAX_PAYLOAD, RFLAG_AUX,
};

/// Validation failure. Variants carry the record `seq` where applicable —
/// enough to locate the fault without re-walking the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadError {
    /// Shorter than the fixed 256-byte header.
    TooShort,
    /// `magic` is not ASCII `DHILOG`.
    BadMagic,
    /// `version` major byte is not 1 (minors are additive and accepted).
    UnsupportedVersion { found: u16 },
    /// `header_len` is not 256 (fixed for v1).
    BadHeaderLen { found: u32 },
    /// Header flag bits beyond SEALED/HAS_AUX/EPOCH_HASHES are set
    /// ("others 0", §3.1).
    UnknownHeaderFlags { flags: u32 },
    /// `flags.SEALED == 0`: a crash artifact, MUST NOT be replayed (§3.4.4).
    NotSealed,
    /// Reserved bytes [248..256) are nonzero (readers MUST reject, §3.1).
    ReservedNonzero,
    /// BLAKE3 of `[256, EOF)` does not match `header.body_hash`.
    BodyHashMismatch,
    /// A record header or payload runs past EOF.
    Truncated { seq: u32 },
    /// `payload_len` exceeds 4096 (§3.2).
    PayloadTooLong { seq: u32 },
    /// `seq` is not the record's index (starts 0, +1 per record, §3.2).
    SeqMismatch { expected: u32, found: u32 },
    /// `icount` regressed (records MUST be ordered by icount, §3.2).
    IcountRegressed { seq: u32 },
    /// Record flag bits beyond AUX are set ("others 0", §3.2).
    UnknownRecordFlags { rflags: u8, seq: u32 },
    /// A canonical (non-AUX) record of a kind this reader cannot apply.
    UnknownCanonicalKind { kind: u8, seq: u32 },
    /// A known kind whose `rflags.AUX` contradicts its §3.3 class.
    KindAuxMismatch { kind: u8, seq: u32 },
    /// A known kind with the wrong payload size/shape for its §3.3 layout.
    BadPayloadLayout { kind: u8, seq: u32 },
    /// Inter-record zero-padding is nonzero (writer zero-pads, §3.2).
    NonzeroPadding { seq: u32 },
    /// `header.record_count` does not match the records actually present.
    RecordCountMismatch { header: u64, actual: u64 },
    /// Sealed log without a final END record (§3.3: always last, always
    /// present), or with records after it.
    EndNotLast,
    /// END payload disagrees with the header (`end_icount`/`end_state_hash`)
    /// or violates the END ruling (`boundary_rip = 0`, zero pad bytes).
    /// `what` names the failed check for forensic/divergence tooling.
    EndMismatch { what: &'static str },
    /// `clock_den == 0`: not a §3.1 rule, but an untrusted log must not be
    /// able to smuggle a divide-by-zero into a downstream replayer.
    BadClock,
    /// `flags.HAS_AUX` disagrees with the records (END does not count).
    HasAuxFlagMismatch,
    /// `flags.EPOCH_HASHES` disagrees with the presence of EPOCH_HASH records.
    EpochHashesFlagMismatch,
}

/// The fixed 256-byte header, parsed (§3.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub version: u16,
    pub flags: u32,
    pub base_snapshot_id: [u8; 32],
    /// Zeros if no end snapshot was taken.
    pub end_snapshot_id: [u8; 32],
    pub entropy_seed: [u8; 32],
    pub machine_config_hash: [u8; 32],
    pub clock_num: u32,
    pub clock_den: u32,
    pub record_count: u64,
    pub end_icount: u64,
    pub end_vns: u64,
    pub end_state_hash: [u8; 32],
    pub body_hash: [u8; 32],
    /// detguest-wire encoder fingerprint at [240..248) (bead 4ld); zero ⇒ no
    /// SDK digests in this segment. Verifiers compare fingerprints before
    /// digests to detect encoder skew.
    pub encoder_fingerprint: u64,
}

impl Header {
    pub fn has_aux(&self) -> bool {
        self.flags & FLAG_HAS_AUX != 0
    }
    pub fn has_epoch_hashes(&self) -> bool {
        self.flags & FLAG_EPOCH_HASHES != 0
    }
}

/// One record, borrowed from the validated byte image (§3.2).
///
/// Fields are private so the ONLY construction path is the validated
/// iterator — [`Record::body`]'s infallibility is true by construction, not
/// by caller discipline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record<'a> {
    kind: u8,
    rflags: u8,
    seq: u32,
    icount: u64,
    boundary_rip: u64,
    payload: &'a [u8],
}

impl<'a> Record<'a> {
    pub fn kind(&self) -> u8 {
        self.kind
    }
    pub fn rflags(&self) -> u8 {
        self.rflags
    }
    pub fn seq(&self) -> u32 {
        self.seq
    }
    pub fn icount(&self) -> u64 {
        self.icount
    }
    pub fn boundary_rip(&self) -> u64 {
        self.boundary_rip
    }
    pub fn payload(&self) -> &'a [u8] {
        self.payload
    }

    pub fn is_aux(&self) -> bool {
        self.rflags & RFLAG_AUX != 0
    }

    /// Typed view of the payload. Infallible: records only come out of
    /// `LogReader::parse` or `LogInspection::parse_unsealed`, both of
    /// which validate every exposed known-kind layout; unknown AUX kinds
    /// surface as [`RecordBody::Unknown`].
    pub fn body(&self) -> RecordBody<'a> {
        let p = self.payload;
        let u16at = |o: usize| u16::from_le_bytes(p[o..o + 2].try_into().unwrap());
        let u32at = |o: usize| u32::from_le_bytes(p[o..o + 4].try_into().unwrap());
        let u64at = |o: usize| u64::from_le_bytes(p[o..o + 8].try_into().unwrap());
        match self.kind {
            KIND_PAD_SET => RecordBody::PadSet {
                port: p[0],
                buttons: u32at(4),
                frame_hint: u32at(8),
            },
            KIND_DEV_EVENT => RecordBody::DevEvent {
                device_id: u16at(0),
                event_type: u16at(2),
                data: &p[8..],
            },
            KIND_NET_RX => RecordBody::NetRx { frame: p },
            KIND_ENTROPY => RecordBody::Entropy {
                len: u32at(0),
                digest8: u64at(8),
            },
            KIND_TIMER_FIRE => RecordBody::TimerFire {
                vector: p[0],
                armed_deadline_vns: u64at(4),
                delivered_icount: u64at(12),
            },
            KIND_EPOCH_HASH => RecordBody::EpochHash {
                epoch_index: u64at(0),
                chain_value: p[8..40].try_into().unwrap(),
            },
            KIND_SDK_EVENT => RecordBody::SdkEvent {
                stream: u16at(0),
                len: u32at(4),
                digest8: u64at(8),
            },
            KIND_NET_TX => RecordBody::NetTx {
                len: u32at(0),
                digest8: u64at(8),
            },
            KIND_FRAME_MARK => RecordBody::FrameMark {
                frame_index: u32at(0),
            },
            KIND_END => RecordBody::End {
                stop_reason: p[0],
                end_state_hash: p[8..40].try_into().unwrap(),
            },
            kind => RecordBody::Unknown { kind, payload: p },
        }
    }
}

/// Typed §3.3 payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordBody<'a> {
    // Canonical — replay applies these.
    PadSet {
        port: u8,
        buttons: u32,
        /// `FRAME_HINT_NONE` (0xFFFF_FFFF) when not frame-scheduled.
        frame_hint: u32,
    },
    DevEvent {
        device_id: u16,
        event_type: u16,
        data: &'a [u8],
    },
    NetRx {
        frame: &'a [u8],
    },
    // AUX — recomputed and compared during verification, skippable.
    Entropy {
        len: u32,
        digest8: u64,
    },
    TimerFire {
        vector: u8,
        armed_deadline_vns: u64,
        delivered_icount: u64,
    },
    EpochHash {
        epoch_index: u64,
        chain_value: [u8; 32],
    },
    SdkEvent {
        stream: u16,
        len: u32,
        digest8: u64,
    },
    NetTx {
        len: u32,
        digest8: u64,
    },
    FrameMark {
        frame_index: u32,
    },
    End {
        stop_reason: u8,
        end_state_hash: [u8; 32],
    },
    /// An AUX kind this reader does not know (later v1.x minor). Canonical
    /// unknowns never get here — parse rejects them.
    Unknown {
        kind: u8,
        payload: &'a [u8],
    },
}

/// Validated DHILOG image. Construction (`parse`) performs the full §3
/// validation battery; the accessors are infallible afterwards.
#[derive(Clone, Debug)]
pub struct LogReader<'a> {
    header: Header,
    /// The record region `[256, EOF)`, fully framing-validated.
    body: &'a [u8],
}

/// Inspection-only view of a DHILOG crash artifact. This is deliberately
/// NOT a `LogReader`: replay and verification only accept sealed logs
/// through [`LogReader::parse`].
#[derive(Clone, Debug)]
pub struct LogInspection<'a> {
    header: Header,
    records: Vec<Record<'a>>,
    stop: InspectionStop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InspectionStop {
    /// Reached EOF after a valid record prefix. For unsealed artifacts this
    /// does not imply replayability: body_hash, record_count, HAS_AUX,
    /// EPOCH_HASHES, and END presence/cross-checks were intentionally not
    /// enforced.
    Eof,
    /// The first record-level corruption after the returned prefix.
    Corrupt(ReadError),
}

impl<'a> LogReader<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ReadError> {
        let header = parse_header(bytes)?;

        let body = &bytes[HEADER_LEN..];
        if *blake3::hash(body).as_bytes() != header.body_hash {
            return Err(ReadError::BodyHashMismatch);
        }

        validate_records(&header, body)?;
        Ok(Self { header, body })
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    /// All records, file order (= normative (`icount`, `seq`) order).
    pub fn records(&self) -> Records<'a> {
        Records {
            body: self.body,
            offset: 0,
        }
    }

    /// Canonical records only — the minimal-replay view (AUX skipped,
    /// END excluded since it is AUX-flagged).
    pub fn canonical(&self) -> impl Iterator<Item = Record<'a>> {
        self.records().filter(|r| !r.is_aux())
    }

    /// AUX records only (END included) — the verification view.
    pub fn aux(&self) -> impl Iterator<Item = Record<'a>> {
        self.records().filter(|r| r.is_aux())
    }

    /// The END record's payload (always present in a sealed log).
    pub fn end(&self) -> (u8, [u8; 32]) {
        // parse() guaranteed the last record is END with a valid layout.
        let last = self.records().last().unwrap();
        match last.body() {
            RecordBody::End {
                stop_reason,
                end_state_hash,
            } => (stop_reason, end_state_hash),
            _ => unreachable!("parse() guarantees END is last"),
        }
    }
}

impl<'a> LogInspection<'a> {
    /// Parse an unsealed or partially-corrupt artifact for diagnostics.
    ///
    /// Header shape is still validated, but replay-only gates are skipped:
    /// SEALED, body_hash, record_count/HAS_AUX/EPOCH_HASHES consistency,
    /// END presence, END-last, and END/header cross-checks. Records are
    /// returned only while their framing, watermark, flags, and known-kind
    /// layouts are safe to expose through [`Record::body`].
    pub fn parse_unsealed(bytes: &'a [u8]) -> Result<Self, ReadError> {
        let header = parse_header_for_inspection(bytes)?;
        let body = &bytes[HEADER_LEN..];
        let mut state = RecordScan::default();
        let mut records = Vec::new();
        let stop = loop {
            match scan_next_record(body, &mut state) {
                Ok(Some(record)) => records.push(record),
                Ok(None) => break InspectionStop::Eof,
                Err(err) => break InspectionStop::Corrupt(err),
            }
        };
        Ok(Self {
            header,
            records,
            stop,
        })
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn stop(&self) -> InspectionStop {
        self.stop
    }

    pub fn records(&self) -> impl Iterator<Item = Record<'a>> + '_ {
        self.records.iter().copied()
    }
}

/// Infallible record iterator over the validated body.
#[derive(Clone, Debug)]
pub struct Records<'a> {
    body: &'a [u8],
    offset: usize,
}

impl<'a> Iterator for Records<'a> {
    type Item = Record<'a>;

    fn next(&mut self) -> Option<Record<'a>> {
        if self.offset >= self.body.len() {
            return None;
        }
        let b = &self.body[self.offset..];
        let payload_len = u16::from_le_bytes(b[2..4].try_into().unwrap()) as usize;
        let record = Record {
            kind: b[0],
            rflags: b[1],
            seq: u32::from_le_bytes(b[4..8].try_into().unwrap()),
            icount: u64::from_le_bytes(b[8..16].try_into().unwrap()),
            boundary_rip: u64::from_le_bytes(b[16..24].try_into().unwrap()),
            payload: &b[24..24 + payload_len],
        };
        self.offset += 24 + payload_len + pad_len(payload_len);
        Some(record)
    }
}

fn pad_len(payload_len: usize) -> usize {
    (8 - payload_len % 8) % 8
}

fn parse_header(bytes: &[u8]) -> Result<Header, ReadError> {
    parse_header_with_seal(bytes, true)
}

fn parse_header_for_inspection(bytes: &[u8]) -> Result<Header, ReadError> {
    parse_header_with_seal(bytes, false)
}

fn parse_header_with_seal(bytes: &[u8], require_sealed: bool) -> Result<Header, ReadError> {
    if bytes.len() < HEADER_LEN {
        return Err(ReadError::TooShort);
    }
    if &bytes[0..6] != b"DHILOG" {
        return Err(ReadError::BadMagic);
    }
    let version = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
    if version >> 8 != 0x01 {
        return Err(ReadError::UnsupportedVersion { found: version });
    }
    let header_len = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    if header_len as usize != HEADER_LEN {
        return Err(ReadError::BadHeaderLen { found: header_len });
    }
    let flags = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    if flags & !(FLAG_SEALED | FLAG_HAS_AUX | FLAG_EPOCH_HASHES) != 0 {
        return Err(ReadError::UnknownHeaderFlags { flags });
    }
    if require_sealed && flags & FLAG_SEALED == 0 {
        return Err(ReadError::NotSealed);
    }
    if bytes[248..256] != [0u8; 8] {
        return Err(ReadError::ReservedNonzero);
    }
    if u32::from_le_bytes(bytes[148..152].try_into().unwrap()) == 0 {
        return Err(ReadError::BadClock);
    }
    Ok(Header {
        version,
        flags,
        base_snapshot_id: bytes[16..48].try_into().unwrap(),
        end_snapshot_id: bytes[48..80].try_into().unwrap(),
        entropy_seed: bytes[80..112].try_into().unwrap(),
        machine_config_hash: bytes[112..144].try_into().unwrap(),
        clock_num: u32::from_le_bytes(bytes[144..148].try_into().unwrap()),
        clock_den: u32::from_le_bytes(bytes[148..152].try_into().unwrap()),
        record_count: u64::from_le_bytes(bytes[152..160].try_into().unwrap()),
        end_icount: u64::from_le_bytes(bytes[160..168].try_into().unwrap()),
        end_vns: u64::from_le_bytes(bytes[168..176].try_into().unwrap()),
        end_state_hash: bytes[176..208].try_into().unwrap(),
        body_hash: bytes[208..240].try_into().unwrap(),
        encoder_fingerprint: u64::from_le_bytes(bytes[240..248].try_into().unwrap()),
    })
}

/// The §3.2/§3.3 record walk: framing, watermark, seq, layouts, flag
/// consistency, END semantics. Every slice index below is dominated by the
/// explicit length checks at the top of the loop body.
fn validate_records(header: &Header, body: &[u8]) -> Result<(), ReadError> {
    let mut scan = RecordScan::default();
    let mut saw_end = false;
    let mut saw_aux_non_end = false;
    let mut saw_epoch_hash = false;

    while scan.offset < body.len() {
        if saw_end {
            return Err(ReadError::EndNotLast);
        }
        let rec = scan_next_record(body, &mut scan)?.expect("offset < len");

        match rec.kind {
            KIND_END => {
                // §3.3 END ruling: AUX-flagged, boundary_rip = 0, zero pad
                // bytes, payload cross-checked against the header. The
                // stop_reason byte (payload[0]) is intentionally NOT range-
                // checked: it mirrors proto StopReason, which grows without a
                // format bump (forward-compatible by design).
                if rec.boundary_rip != 0 {
                    return Err(ReadError::EndMismatch {
                        what: "boundary_rip != 0",
                    });
                }
                if rec.payload[1..8] != [0u8; 7] {
                    return Err(ReadError::EndMismatch {
                        what: "nonzero pad bytes",
                    });
                }
                if rec.icount != header.end_icount {
                    return Err(ReadError::EndMismatch {
                        what: "icount != header.end_icount",
                    });
                }
                if rec.payload[8..40] != header.end_state_hash {
                    return Err(ReadError::EndMismatch {
                        what: "end_state_hash != header.end_state_hash",
                    });
                }
                saw_end = true;
            }
            KIND_EPOCH_HASH => saw_epoch_hash = true,
            _ if rec.is_aux() => saw_aux_non_end = true,
            _ => {}
        }
    }

    if !saw_end {
        return Err(ReadError::EndNotLast);
    }
    if scan.count != header.record_count {
        return Err(ReadError::RecordCountMismatch {
            header: header.record_count,
            actual: scan.count,
        });
    }
    // EPOCH_HASH records are AUX too, so fold them into the HAS_AUX check.
    if header.has_aux() != (saw_aux_non_end || saw_epoch_hash) {
        return Err(ReadError::HasAuxFlagMismatch);
    }
    if header.has_epoch_hashes() != saw_epoch_hash {
        return Err(ReadError::EpochHashesFlagMismatch);
    }
    Ok(())
}

#[derive(Default)]
struct RecordScan {
    offset: usize,
    count: u64,
    last_icount: u64,
}

fn scan_next_record<'a>(
    body: &'a [u8],
    scan: &mut RecordScan,
) -> Result<Option<Record<'a>>, ReadError> {
    if scan.offset >= body.len() {
        return Ok(None);
    }
    let seq_for_err = u32::try_from(scan.count).unwrap_or(u32::MAX);
    if body.len() - scan.offset < 24 {
        return Err(ReadError::Truncated { seq: seq_for_err });
    }
    let b = &body[scan.offset..];
    let kind = b[0];
    let rflags = b[1];
    let payload_len = u16::from_le_bytes(b[2..4].try_into().unwrap()) as usize;
    let seq = u32::from_le_bytes(b[4..8].try_into().unwrap());
    let icount = u64::from_le_bytes(b[8..16].try_into().unwrap());
    let boundary_rip = u64::from_le_bytes(b[16..24].try_into().unwrap());

    if payload_len > MAX_PAYLOAD {
        return Err(ReadError::PayloadTooLong { seq: seq_for_err });
    }
    let padded = 24 + payload_len + pad_len(payload_len);
    if body.len() - scan.offset < padded {
        return Err(ReadError::Truncated { seq: seq_for_err });
    }
    if u64::from(seq) != scan.count {
        return Err(ReadError::SeqMismatch {
            expected: seq_for_err,
            found: seq,
        });
    }
    if scan.count > 0 && icount < scan.last_icount {
        return Err(ReadError::IcountRegressed { seq });
    }
    if rflags & !RFLAG_AUX != 0 {
        return Err(ReadError::UnknownRecordFlags { rflags, seq });
    }
    if b[24 + payload_len..padded].iter().any(|&x| x != 0) {
        return Err(ReadError::NonzeroPadding { seq });
    }

    let aux = rflags & RFLAG_AUX != 0;
    let payload = &b[24..24 + payload_len];
    validate_kind(kind, aux, payload, seq)?;

    let record = Record {
        kind,
        rflags,
        seq,
        icount,
        boundary_rip,
        payload,
    };
    scan.offset += padded;
    scan.last_icount = icount;
    scan.count += 1;
    Ok(Some(record))
}

/// Per-kind §3.3 layout validation. `payload` length is already ≤ 4096 and
/// in-bounds.
fn validate_kind(kind: u8, aux: bool, payload: &[u8], seq: u32) -> Result<(), ReadError> {
    let class_aux = match kind {
        KIND_PAD_SET | KIND_DEV_EVENT | KIND_NET_RX => false,
        KIND_ENTROPY | KIND_TIMER_FIRE | KIND_EPOCH_HASH | KIND_SDK_EVENT | KIND_NET_TX
        | KIND_FRAME_MARK | KIND_END => true,
        _ if aux => return Ok(()), // unknown AUX: skippable, accepted (§3.4)
        _ => return Err(ReadError::UnknownCanonicalKind { kind, seq }),
    };
    if class_aux != aux {
        return Err(ReadError::KindAuxMismatch { kind, seq });
    }
    let layout_ok = match kind {
        KIND_PAD_SET => payload.len() == 12,
        KIND_DEV_EVENT => {
            payload.len() >= 8
                && u32::from_le_bytes(payload[4..8].try_into().unwrap()) as usize
                    == payload.len() - 8
        }
        // NET_RX is 1..=2048 (bead 206; API.md §3.3 amended, ledger #19):
        // PvNet rejects empty delivery, so an accepted empty record would
        // be unreplayable — the codec forbids what the device refuses.
        KIND_NET_RX => (1..=MAX_NET_RX_FRAME).contains(&payload.len()),
        KIND_ENTROPY | KIND_SDK_EVENT | KIND_NET_TX => payload.len() == 16,
        KIND_TIMER_FIRE => payload.len() == 20,
        KIND_EPOCH_HASH | KIND_END => payload.len() == 40,
        KIND_FRAME_MARK => payload.len() == 8,
        // Unreachable: unknown kinds returned above (Ok for AUX, Err for
        // canonical), so `kind` is a known kind here. Kept total anyway.
        _ => true,
    };
    if !layout_ok {
        return Err(ReadError::BadPayloadLayout { kind, seq });
    }
    Ok(())
}
