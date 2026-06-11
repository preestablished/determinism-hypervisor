//! DHSNAP v1 container codec battery (bead 68l): writer round-trips,
//! byte-surgery negatives for every §4 validation rule, golden pinned
//! bytes (the container layout freezes with the same discipline as the
//! DHILOG fixtures), and totality smokes.

use dh_snapshot::dhsnap::*;

/// A representative container: one section per §4 tag, realistic shapes.
fn full_container() -> Vec<u8> {
    let mut w = ContainerWriter::new();
    w.push_section(tag::MCFG, 1, &[0x11; 64]).unwrap(); // canonical MachineConfig encoding (dh-vmm owns)
    w.push_section(tag::VCPU, 1, &[0x22; 200]).unwrap(); // canonical_vcpu_blob (dh-vmm owns)
    w.push_section(tag::LAPC, 1, &[0x33; 40]).unwrap();
    w.push_section(
        tag::TIME,
        TimeSection::VERSION,
        &TimeSection {
            cumulative_icount: 1_000_000_000,
            vns: 3_000_000_000,
            epoch_index: 20,
            hash_chain: [0x44; 32],
        }
        .encode(),
    )
    .unwrap();
    w.push_section(
        tag::ENTR,
        EntrSection::VERSION,
        &EntrSection {
            seed: [0x55; 32],
            stream: 7,
            word_pos: 123_456,
        }
        .encode(),
    )
    .unwrap();
    w.push_section(tag::CLKD, 1, &[0x66; 12]).unwrap(); // pv-clock regs (device owns)
    w.push_section(tag::PADD, 1, &[0x77; 21]).unwrap(); // pv-pad latches+vector+frame (device owns)
    w.push_section(tag::EVTC, 1, &[0x88; 39]).unwrap(); // detchannel EVTC_LEN (device owns)
    w.push_section(tag::BLKO, 1, &[0x99; 36]).unwrap(); // pv-blk overlay (device owns)
    w.push_section(tag::NETL, 1, &[]).unwrap(); // pv-net: empty pending-RX rule
    w.push_section(tag::SERL, 1, &[]).unwrap(); // debug-serial: empty section rule
    w.finish()
}

// ---- happy path -------------------------------------------------------------

#[test]
fn roundtrips_all_eleven_tags_in_order() {
    let bytes = full_container();
    let c = Container::parse(&bytes).unwrap();
    let tags: Vec<[u8; 4]> = c.sections().map(|s| s.tag).collect();
    assert_eq!(tags, KNOWN_TAGS.to_vec());

    let vcpu = c.get(tag::VCPU).unwrap();
    assert_eq!(vcpu.sec_version, 1);
    assert_eq!(vcpu.contents, &[0x22; 200][..]);

    // Empty sections are real sections.
    assert_eq!(c.get(tag::SERL).unwrap().contents, &[] as &[u8]);
    assert!(c.has(tag::NETL));
}

#[test]
fn typed_sections_roundtrip_and_pin_their_layout() {
    let bytes = full_container();
    let c = Container::parse(&bytes).unwrap();

    let t = c.get(tag::TIME).unwrap();
    let time = TimeSection::decode(t.contents, t.sec_version).unwrap();
    assert_eq!(time.cumulative_icount, 1_000_000_000);
    assert_eq!(time.vns, 3_000_000_000);
    assert_eq!(time.epoch_index, 20);
    assert_eq!(time.hash_chain, [0x44; 32]);

    let e = c.get(tag::ENTR).unwrap();
    let entr = EntrSection::decode(e.contents, e.sec_version).unwrap();
    assert_eq!(entr.seed, [0x55; 32]);
    assert_eq!(entr.stream, 7);
    assert_eq!(entr.word_pos, 123_456);

    // Field-offset pins (LE): a swapped field must fail here.
    let te = time.encode();
    assert_eq!(&te[0..8], &1_000_000_000u64.to_le_bytes());
    assert_eq!(&te[8..16], &3_000_000_000u64.to_le_bytes());
    assert_eq!(&te[16..24], &20u64.to_le_bytes());
    assert_eq!(&te[24..56], &[0x44; 32]);
    let ee = entr.encode();
    assert_eq!(&ee[0..32], &[0x55; 32]);
    assert_eq!(&ee[32..40], &7u64.to_le_bytes());
    assert_eq!(&ee[40..56], &123_456u128.to_le_bytes());
}

#[test]
fn deterministic_byte_identical_rebuild() {
    assert_eq!(full_container(), full_container());
}

#[test]
fn empty_container_is_just_header() {
    let bytes = ContainerWriter::new().finish();
    assert_eq!(bytes.len(), HEADER_LEN);
    let c = Container::parse(&bytes).unwrap();
    assert_eq!(c.sections().count(), 0);
}

#[test]
fn device_id_tag_map_is_total_over_known_devices() {
    assert_eq!(tag_for_device_id(0x0001), Some(tag::EVTC));
    assert_eq!(tag_for_device_id(0x0002), Some(tag::CLKD));
    assert_eq!(tag_for_device_id(0x0003), Some(tag::PADD));
    assert_eq!(tag_for_device_id(0x0004), Some(tag::ENTR));
    assert_eq!(tag_for_device_id(0x0005), Some(tag::BLKO));
    assert_eq!(tag_for_device_id(0x0006), Some(tag::SERL));
    assert_eq!(tag_for_device_id(0x0007), Some(tag::NETL));
    assert_eq!(tag_for_device_id(0x0008), None);
}

// ---- golden bytes (layout freeze, same discipline as the DHILOG fixtures) ----

#[test]
fn golden_bytes_minimal_container() {
    // Hand-assembled: header + one TIME section. Round-trips alone cannot
    // catch wrong-but-symmetric layouts; these bytes can.
    let mut expect = Vec::new();
    expect.extend_from_slice(b"DHSNAP"); // magic
    expect.extend_from_slice(&[0x00, 0x01]); // version 0x0100 LE
    expect.extend_from_slice(&1u32.to_le_bytes()); // section_count
    expect.extend_from_slice(&[0u8; 4]); // _pad
    expect.extend_from_slice(b"TIME"); // tag
    expect.extend_from_slice(&1u16.to_le_bytes()); // sec_version
    expect.extend_from_slice(&[0u8; 2]); // _pad
    expect.extend_from_slice(&56u32.to_le_bytes()); // len
    expect.extend_from_slice(&42u64.to_le_bytes()); // cumulative_icount
    expect.extend_from_slice(&84u64.to_le_bytes()); // vns
    expect.extend_from_slice(&1u64.to_le_bytes()); // epoch_index
    expect.extend_from_slice(&[0xAB; 32]); // hash_chain
                                           // 56 % 8 == 0: no alignment padding.

    let mut w = ContainerWriter::new();
    w.push_section(
        tag::TIME,
        TimeSection::VERSION,
        &TimeSection {
            cumulative_icount: 42,
            vns: 84,
            epoch_index: 1,
            hash_chain: [0xAB; 32],
        }
        .encode(),
    )
    .unwrap();
    assert_eq!(w.finish(), expect);
}

#[test]
fn alignment_padding_is_emitted_and_zeroed() {
    let mut w = ContainerWriter::new();
    w.push_section(tag::PADD, 1, &[0xFF; 21]).unwrap(); // 21 → pad 3
    let bytes = w.finish();
    assert_eq!(bytes.len(), HEADER_LEN + SECTION_HEADER_LEN + 21 + 3);
    assert_eq!(&bytes[bytes.len() - 3..], &[0u8; 3]);
    assert!(Container::parse(&bytes).is_ok());
}

// ---- writer negatives ---------------------------------------------------------

#[test]
fn writer_rejects_unknown_and_duplicate_tags() {
    let mut w = ContainerWriter::new();
    assert_eq!(
        w.push_section(*b"XXXX", 1, &[]),
        Err(WriteError::UnknownTag { tag: *b"XXXX" })
    );
    w.push_section(tag::TIME, 1, &[0; 56]).unwrap();
    assert_eq!(
        w.push_section(tag::TIME, 1, &[0; 56]),
        Err(WriteError::DuplicateTag { tag: tag::TIME })
    );
}

// ---- reader negatives -----------------------------------------------------------

#[test]
fn rejects_short_bad_magic_and_version() {
    assert_eq!(Container::parse(&[]).unwrap_err(), ReadError::TooShort);
    assert_eq!(
        Container::parse(&[0u8; 15]).unwrap_err(),
        ReadError::TooShort
    );
    let mut b = full_container();
    b[0] = b'X';
    assert_eq!(Container::parse(&b).unwrap_err(), ReadError::BadMagic);

    let mut b = full_container();
    b[7] = 0x02; // major 2
    assert_eq!(
        Container::parse(&b).unwrap_err(),
        ReadError::UnsupportedVersion { found: 0x0200 }
    );
    let mut b = full_container();
    b[6] = 0x01; // 1.1: additive minor accepted
    assert!(Container::parse(&b).is_ok());
}

#[test]
fn rejects_nonzero_header_pad() {
    let mut b = full_container();
    b[13] = 1;
    assert_eq!(
        Container::parse(&b).unwrap_err(),
        ReadError::NonzeroHeaderPad
    );
}

#[test]
fn rejects_unknown_tag_and_duplicate_tag() {
    let mut b = full_container();
    b[HEADER_LEN..HEADER_LEN + 4].copy_from_slice(b"ZZZZ");
    assert_eq!(
        Container::parse(&b).unwrap_err(),
        ReadError::UnknownTag {
            tag: *b"ZZZZ",
            index: 0
        }
    );

    // Rewrite the second section's tag (VCPU) to MCFG → duplicate.
    let mut b = full_container();
    let second = HEADER_LEN + SECTION_HEADER_LEN + 64; // MCFG is 64 bytes, 8-aligned
    b[second..second + 4].copy_from_slice(&tag::MCFG);
    assert_eq!(
        Container::parse(&b).unwrap_err(),
        ReadError::DuplicateTag { tag: tag::MCFG }
    );
}

#[test]
fn rejects_nonzero_section_pads() {
    // Section-header _pad bytes.
    let mut b = full_container();
    b[HEADER_LEN + 6] = 0xFF;
    assert_eq!(
        Container::parse(&b).unwrap_err(),
        ReadError::NonzeroPadding { index: 0 }
    );

    // Inter-section alignment padding: PADD's 21-byte contents pad by 3.
    let mut w = ContainerWriter::new();
    w.push_section(tag::PADD, 1, &[0xFF; 21]).unwrap();
    let mut b = w.finish();
    let last = b.len() - 1;
    b[last] = 0xFF;
    assert_eq!(
        Container::parse(&b).unwrap_err(),
        ReadError::NonzeroPadding { index: 0 }
    );
}

#[test]
fn rejects_truncation_and_forged_len() {
    let full = full_container();
    let cut = &full[..full.len() - 4];
    assert!(matches!(
        Container::parse(cut).unwrap_err(),
        ReadError::Truncated { .. } | ReadError::SectionCountMismatch { .. }
    ));

    // Forge a section len that runs past EOF.
    let mut b = full_container();
    b[HEADER_LEN + 8..HEADER_LEN + 12].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    assert_eq!(
        Container::parse(&b).unwrap_err(),
        ReadError::Truncated { index: 0 }
    );
}

#[test]
fn rejects_section_count_mismatch() {
    let mut b = full_container();
    b[8..12].copy_from_slice(&99u32.to_le_bytes());
    assert_eq!(
        Container::parse(&b).unwrap_err(),
        ReadError::SectionCountMismatch {
            header: 99,
            actual: 11
        }
    );
}

#[test]
fn typed_section_decode_negatives() {
    assert_eq!(
        TimeSection::decode(&[0; 55], TimeSection::VERSION),
        Err(SectionError::BadLength { found: 55 })
    );
    assert_eq!(
        TimeSection::decode(&[0; 56], 2),
        Err(SectionError::BadVersion { found: 2 })
    );
    assert_eq!(
        EntrSection::decode(&[0; 57], EntrSection::VERSION),
        Err(SectionError::BadLength { found: 57 })
    );
    assert_eq!(
        EntrSection::decode(&[0; 56], 0),
        Err(SectionError::BadVersion { found: 0 })
    );
}

// ---- totality smoke -------------------------------------------------------------

#[test]
fn arbitrary_truncations_never_panic() {
    let b = full_container();
    for len in 0..b.len() {
        let _ = Container::parse(&b[..len]);
    }
}

#[test]
fn single_byte_corruptions_never_panic() {
    let b = full_container();
    for i in 0..b.len() {
        let mut m = b.clone();
        m[i] ^= 0xFF;
        let _ = Container::parse(&m); // Ok or Err both fine; no panic
    }
}
