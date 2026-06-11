//! DHSNAP v1 container codec — the device blob of API.md §4 (byte-level,
//! normative). The blob is embedded byte-identically in every snapshot
//! manifest, so its bytes are part of the snapshot ref: encoding MUST be a
//! pure function of the inputs (section order is caller-fixed, padding is
//! zeroed, no ambient state).
//!
//! Layout: 16-byte header (`magic "DHSNAP"`, `version 0x0100`,
//! `section_count u32`, `_pad u32`) then sections back to back. Section:
//! `tag [u8;4]`, `sec_version u16`, `_pad u16`, `len u32`, `len` contents
//! bytes, zero-padded to 8-byte alignment.
//!
//! Ownership split (the in-tree convention this codec completes):
//! - Section CONTENTS are produced/consumed by their owners —
//!   `DetDevice::snapshot`/`restore` for device tags, detchannel's
//!   `snapshot`/`restore` for `EVTC`, dh-vmm's
//!   `vcpu_state::encode_section` for `VCPU` (the API.md §4 raw-struct
//!   layout; never re-serialized here; no KVM types in this crate,
//!   bead-v5w discipline — NOTE: the state-HASH preimage uses hash.rs's
//!   field-selective `canonical_vcpu_blob`, a different artifact; the
//!   snapshot engine bead reconciles which feeds the snapshot ref),
//!   dh-vmm's canonical MachineConfig encoding for `MCFG`.
//! - This module owns the FRAMING, the tag table, the device-id↔tag map
//!   (one place, per the `DetDevice` doc), and the typed structs for the
//!   engine-owned fixed sections `TIME` and `ENTR`.
//!
//! `Container::parse` is a TOTAL decoder over untrusted bytes: every input
//! yields Ok or Err, never a panic. Unknown tags are REJECTED (§4: device
//! state is never optional — a reader that cannot interpret a section must
//! not restore the snapshot). Duplicate tags are rejected (each device
//! appears once in v1).

/// ASCII `DHSNAP`.
pub const MAGIC: [u8; 6] = *b"DHSNAP";
/// major.minor in the high/low byte: 1.0.
pub const FORMAT_VERSION: u16 = 0x0100;
pub const HEADER_LEN: usize = 16;
/// Section header: tag(4) + sec_version(2) + _pad(2) + len(4).
pub const SECTION_HEADER_LEN: usize = 12;

/// The §4 tag table. Contents owners are noted in the module doc.
pub mod tag {
    pub const MCFG: [u8; 4] = *b"MCFG";
    pub const VCPU: [u8; 4] = *b"VCPU";
    pub const LAPC: [u8; 4] = *b"LAPC";
    pub const TIME: [u8; 4] = *b"TIME";
    pub const ENTR: [u8; 4] = *b"ENTR";
    pub const CLKD: [u8; 4] = *b"CLKD";
    pub const PADD: [u8; 4] = *b"PADD";
    pub const EVTC: [u8; 4] = *b"EVTC";
    pub const BLKO: [u8; 4] = *b"BLKO";
    pub const NETL: [u8; 4] = *b"NETL";
    pub const SERL: [u8; 4] = *b"SERL";
}

/// Every tag a v1 reader accepts, in the canonical §4 table order (also the
/// recommended write order; the codec does not enforce order, the snapshot
/// engine fixes it for byte determinism).
pub const KNOWN_TAGS: [[u8; 4]; 11] = [
    tag::MCFG,
    tag::VCPU,
    tag::LAPC,
    tag::TIME,
    tag::ENTR,
    tag::CLKD,
    tag::PADD,
    tag::EVTC,
    tag::BLKO,
    tag::NETL,
    tag::SERL,
];

/// The device-id↔tag mapping (ONE place, per the `DetDevice` trait doc).
/// Ids are the §6.1 MAGIC register values served by the bus.
///
/// RESOLVED (bead 6yl): §4's `ENTR` v1 contents are the 56-byte VMM-owned
/// PRNG state ([`EntrSection`]) — but the pv-entropy DEVICE also carries
/// 16 bytes of guest-visible MMIO regs `{buf_gpa, len, status}` with no §4
/// tag of their own. The engine therefore writes `ENTR` at sec_version 2
/// ([`EntrSectionV2`], 72 bytes = PRNG state ‖ regs) by combining
/// `DetEntropy::state()` with the device's `DetDevice::snapshot` — NEVER
/// by framing the device blob alone (that is the original landmine:
/// `EntrSection::decode` ⇒ `BadLength{16}`).
pub fn tag_for_device_id(device_id: u16) -> Option<[u8; 4]> {
    match device_id {
        0x0001 => Some(tag::EVTC), // detchannel (dh-inputlog DEVICE_ID_DETCHANNEL)
        0x0002 => Some(tag::CLKD), // pv-clock
        0x0003 => Some(tag::PADD), // pv-pad
        0x0004 => Some(tag::ENTR), // pv-entropy — sec_version 2, see RESOLVED above
        0x0005 => Some(tag::BLKO), // pv-blk overlay
        0x0006 => Some(tag::SERL), // debug-serial (empty section rule)
        // ANTICIPATED: pv-net lands with bead mmv, which must define
        // DEVICE_ID_PV_NET = 0x0007 to match (the bead is annotated).
        0x0007 => Some(tag::NETL),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteError {
    /// Section contents exceed u32::MAX (len is a u32 on the wire).
    SectionTooLong,
    /// Not one of the 11 §4 tags — writers must not invent tags either.
    UnknownTag { tag: [u8; 4] },
    /// Each tag appears at most once in v1.
    DuplicateTag { tag: [u8; 4] },
}

/// Append-only container writer; `finish` back-fills the header.
#[derive(Debug, Default)]
pub struct ContainerWriter {
    buf: Vec<u8>,
    count: u32,
    seen: Vec<[u8; 4]>,
}

impl ContainerWriter {
    pub fn new() -> Self {
        Self {
            buf: vec![0u8; HEADER_LEN],
            count: 0,
            seen: Vec::new(),
        }
    }

    pub fn push_section(
        &mut self,
        tag: [u8; 4],
        sec_version: u16,
        contents: &[u8],
    ) -> Result<(), WriteError> {
        if !KNOWN_TAGS.contains(&tag) {
            return Err(WriteError::UnknownTag { tag });
        }
        if self.seen.contains(&tag) {
            return Err(WriteError::DuplicateTag { tag });
        }
        let len: u32 = contents
            .len()
            .try_into()
            .map_err(|_| WriteError::SectionTooLong)?;
        self.buf.extend_from_slice(&tag);
        self.buf.extend_from_slice(&sec_version.to_le_bytes());
        self.buf.extend_from_slice(&[0u8; 2]);
        self.buf.extend_from_slice(&len.to_le_bytes());
        self.buf.extend_from_slice(contents);
        let pad = (8 - contents.len() % 8) % 8;
        self.buf.extend_from_slice(&[0u8; 8][..pad]);
        self.seen.push(tag);
        self.count += 1;
        Ok(())
    }

    pub fn finish(mut self) -> Vec<u8> {
        self.buf[0..6].copy_from_slice(&MAGIC);
        self.buf[6..8].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
        self.buf[8..12].copy_from_slice(&self.count.to_le_bytes());
        // [12..16) _pad, already zeros.
        self.buf
    }
}

/// Validation failure. `index` is the section's ordinal where applicable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadError {
    /// Shorter than the fixed 16-byte header.
    TooShort,
    /// `magic` is not ASCII `DHSNAP`.
    BadMagic,
    /// `version` major byte is not 1 (minors are additive and accepted).
    UnsupportedVersion { found: u16 },
    /// Header `_pad` [12..16) is nonzero (reserved-means-zero rule).
    NonzeroHeaderPad,
    /// A section header or contents runs past EOF.
    Truncated { index: u32 },
    /// §4: readers MUST reject unknown tags — device state is never optional.
    UnknownTag { tag: [u8; 4], index: u32 },
    /// Each tag appears at most once in v1.
    DuplicateTag { tag: [u8; 4] },
    /// Section header `_pad` or inter-section alignment padding is nonzero.
    NonzeroPadding { index: u32 },
    /// `section_count` disagrees with the sections actually present.
    SectionCountMismatch { header: u32, actual: u32 },
}

/// One section, borrowed from the validated byte image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Section<'a> {
    pub tag: [u8; 4],
    pub sec_version: u16,
    pub contents: &'a [u8],
}

/// Validated DHSNAP container. `parse` performs the full §4 validation;
/// accessors are infallible afterwards.
#[derive(Clone, Debug)]
pub struct Container<'a> {
    sections: Vec<Section<'a>>,
}

impl<'a> Container<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ReadError> {
        if bytes.len() < HEADER_LEN {
            return Err(ReadError::TooShort);
        }
        if bytes[0..6] != MAGIC {
            return Err(ReadError::BadMagic);
        }
        let version = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
        if version >> 8 != 0x01 {
            return Err(ReadError::UnsupportedVersion { found: version });
        }
        let section_count = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        if bytes[12..16] != [0u8; 4] {
            return Err(ReadError::NonzeroHeaderPad);
        }

        let mut sections = Vec::new();
        let mut offset = HEADER_LEN;
        let mut index: u32 = 0;
        while offset < bytes.len() {
            if bytes.len() - offset < SECTION_HEADER_LEN {
                return Err(ReadError::Truncated { index });
            }
            let b = &bytes[offset..];
            let tag: [u8; 4] = b[0..4].try_into().unwrap();
            let sec_version = u16::from_le_bytes(b[4..6].try_into().unwrap());
            if b[6..8] != [0u8; 2] {
                return Err(ReadError::NonzeroPadding { index });
            }
            let len = u32::from_le_bytes(b[8..12].try_into().unwrap()) as usize;
            // len is u32 and offset/SECTION_HEADER_LEN are small: the sum
            // cannot overflow u64-sized usize; on 32-bit hosts checked_add
            // keeps the decoder total anyway.
            let contents_end = SECTION_HEADER_LEN
                .checked_add(len)
                .ok_or(ReadError::Truncated { index })?;
            let padded_end = contents_end
                .checked_add((8 - len % 8) % 8)
                .ok_or(ReadError::Truncated { index })?;
            if bytes.len() - offset < padded_end {
                return Err(ReadError::Truncated { index });
            }
            if !KNOWN_TAGS.contains(&tag) {
                return Err(ReadError::UnknownTag { tag, index });
            }
            if sections.iter().any(|s: &Section| s.tag == tag) {
                return Err(ReadError::DuplicateTag { tag });
            }
            if b[contents_end..padded_end].iter().any(|&x| x != 0) {
                return Err(ReadError::NonzeroPadding { index });
            }
            sections.push(Section {
                tag,
                sec_version,
                contents: &b[SECTION_HEADER_LEN..contents_end],
            });
            offset += padded_end;
            index += 1;
        }

        if index != section_count {
            return Err(ReadError::SectionCountMismatch {
                header: section_count,
                actual: index,
            });
        }
        Ok(Self { sections })
    }

    /// Sections in file order.
    pub fn sections(&self) -> impl Iterator<Item = &Section<'a>> {
        self.sections.iter()
    }

    pub fn get(&self, tag: [u8; 4]) -> Option<&Section<'a>> {
        self.sections.iter().find(|s| s.tag == tag)
    }

    pub fn has(&self, tag: [u8; 4]) -> bool {
        self.get(tag).is_some()
    }
}

// ---- engine-owned fixed sections ---------------------------------------------

/// `TIME` section (§4): exactly 56 bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimeSection {
    pub cumulative_icount: u64,
    pub vns: u64,
    pub epoch_index: u64,
    pub hash_chain: [u8; 32],
}

impl TimeSection {
    pub const LEN: usize = 56;
    pub const VERSION: u16 = 1;

    pub fn encode(&self) -> [u8; Self::LEN] {
        let mut out = [0u8; Self::LEN];
        out[0..8].copy_from_slice(&self.cumulative_icount.to_le_bytes());
        out[8..16].copy_from_slice(&self.vns.to_le_bytes());
        out[16..24].copy_from_slice(&self.epoch_index.to_le_bytes());
        out[24..56].copy_from_slice(&self.hash_chain);
        out
    }

    pub fn decode(bytes: &[u8], sec_version: u16) -> Result<Self, SectionError> {
        if sec_version != Self::VERSION {
            return Err(SectionError::BadVersion { found: sec_version });
        }
        if bytes.len() != Self::LEN {
            return Err(SectionError::BadLength { found: bytes.len() });
        }
        Ok(Self {
            cumulative_icount: u64::from_le_bytes(bytes[0..8].try_into().unwrap()),
            vns: u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
            epoch_index: u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
            hash_chain: bytes[24..56].try_into().unwrap(),
        })
    }
}

/// `ENTR` section (§4): ChaCha20 PRNG state, exactly 56 bytes —
/// `seed [u8;32], stream u64, word_pos u128`. Mirrors dh-devices'
/// `EntropyState` (rand_chacha's exportable state); restore re-seeds via
/// `set_stream`/`set_word_pos` and MUST reproduce the next draws
/// bit-identically (IMPLEMENTATION-PLAN M4; integration is bead 6yl).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EntrSection {
    pub seed: [u8; 32],
    pub stream: u64,
    pub word_pos: u128,
}

impl EntrSection {
    pub const LEN: usize = 56;
    pub const VERSION: u16 = 1;

    pub fn encode(&self) -> [u8; Self::LEN] {
        let mut out = [0u8; Self::LEN];
        out[0..32].copy_from_slice(&self.seed);
        out[32..40].copy_from_slice(&self.stream.to_le_bytes());
        out[40..56].copy_from_slice(&self.word_pos.to_le_bytes());
        out
    }

    pub fn decode(bytes: &[u8], sec_version: u16) -> Result<Self, SectionError> {
        if sec_version != Self::VERSION {
            return Err(SectionError::BadVersion { found: sec_version });
        }
        if bytes.len() != Self::LEN {
            return Err(SectionError::BadLength { found: bytes.len() });
        }
        Ok(Self {
            seed: bytes[0..32].try_into().unwrap(),
            stream: u64::from_le_bytes(bytes[32..40].try_into().unwrap()),
            word_pos: u128::from_le_bytes(bytes[40..56].try_into().unwrap()),
        })
    }
}

/// `ENTR` section, sec_version 2 (bead 6yl): the v1 PRNG state PLUS the
/// pv-entropy device's guest-visible MMIO regs — 72 bytes total. v1 (56
/// bytes, PRNG only) remains decodable for spec-exact producers; the
/// snapshot engine WRITES v2 so the device regs travel with the snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EntrSectionV2 {
    pub seed: [u8; 32],
    pub stream: u64,
    pub word_pos: u128,
    pub buf_gpa: u64,
    pub len: u32,
    pub status: u32,
}

impl EntrSectionV2 {
    pub const LEN: usize = 72;
    pub const VERSION: u16 = 2;

    pub fn prng(&self) -> EntrSection {
        EntrSection {
            seed: self.seed,
            stream: self.stream,
            word_pos: self.word_pos,
        }
    }

    /// Combine the VMM-owned PRNG state with the device's 16-byte
    /// `DetDevice::snapshot` reg blob (buf_gpa u64 ‖ len u32 ‖ status u32).
    pub fn from_parts(prng: EntrSection, device_regs: &[u8]) -> Result<Self, SectionError> {
        if device_regs.len() != 16 {
            return Err(SectionError::BadLength {
                found: device_regs.len(),
            });
        }
        Ok(Self {
            seed: prng.seed,
            stream: prng.stream,
            word_pos: prng.word_pos,
            buf_gpa: u64::from_le_bytes(device_regs[0..8].try_into().expect("8")),
            len: u32::from_le_bytes(device_regs[8..12].try_into().expect("4")),
            status: u32::from_le_bytes(device_regs[12..16].try_into().expect("4")),
        })
    }

    /// The device-reg half, in the exact `DetDevice::restore` layout.
    ///
    /// VERSION-DOMAIN SPLIT (engine trap, verified): feed this to
    /// `device.restore(&regs, 1)` — the DEVICE's own section version,
    /// NOT the ENTR section's `2`. The device rejects version 2.
    pub fn device_regs(&self) -> [u8; 16] {
        let mut out = [0u8; 16];
        out[0..8].copy_from_slice(&self.buf_gpa.to_le_bytes());
        out[8..12].copy_from_slice(&self.len.to_le_bytes());
        out[12..16].copy_from_slice(&self.status.to_le_bytes());
        out
    }

    pub fn encode(&self) -> [u8; Self::LEN] {
        let mut out = [0u8; Self::LEN];
        out[0..32].copy_from_slice(&self.seed);
        out[32..40].copy_from_slice(&self.stream.to_le_bytes());
        out[40..56].copy_from_slice(&self.word_pos.to_le_bytes());
        out[56..72].copy_from_slice(&self.device_regs());
        out
    }

    pub fn decode(bytes: &[u8], sec_version: u16) -> Result<Self, SectionError> {
        if sec_version != Self::VERSION {
            return Err(SectionError::BadVersion { found: sec_version });
        }
        if bytes.len() != Self::LEN {
            return Err(SectionError::BadLength { found: bytes.len() });
        }
        Ok(Self {
            seed: bytes[0..32].try_into().expect("32"),
            stream: u64::from_le_bytes(bytes[32..40].try_into().expect("8")),
            word_pos: u128::from_le_bytes(bytes[40..56].try_into().expect("16")),
            buf_gpa: u64::from_le_bytes(bytes[56..64].try_into().expect("8")),
            len: u32::from_le_bytes(bytes[64..68].try_into().expect("4")),
            status: u32::from_le_bytes(bytes[68..72].try_into().expect("4")),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionError {
    BadVersion { found: u16 },
    BadLength { found: usize },
}
