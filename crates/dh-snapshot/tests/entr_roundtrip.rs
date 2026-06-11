//! ENTR round trip (bead 6yl, IMPLEMENTATION-PLAN M4 golden requirement):
//! a restored PRNG MUST reproduce the next draws bit-identically. The
//! whole chain is exercised — live `DetEntropy` → `EntropyState` →
//! `EntrSectionV2` (with the device's reg blob) → DHSNAP container bytes →
//! parse → decode → `DetEntropy::restore` → identical continuation.

use dh_devices::entropy::{DetEntropy, EntropyState};
use dh_devices::EntropySource;
use dh_snapshot::dhsnap::*;

#[test]
fn restored_prng_reproduces_the_next_draws_bit_identically() {
    let mut original = DetEntropy::from_seed([0x42; 32]);

    // Consume an odd amount of stream first, including a sub-word fill
    // (the word-granularity invariant: a 37-byte fill discards the
    // remainder; state() is always word-aligned).
    let mut burn = vec![0u8; 1024];
    original.fill(&mut burn);
    let mut odd = vec![0u8; 37];
    original.fill(&mut odd);

    // Snapshot: PRNG state + a synthetic device-reg blob, through the
    // FULL container codec.
    let state = original.state();
    let device_regs = {
        let mut r = [0u8; 16];
        r[0..8].copy_from_slice(&0xD000_3000u64.to_le_bytes()); // buf_gpa
        r[8..12].copy_from_slice(&64u32.to_le_bytes()); // len
        r[12..16].copy_from_slice(&1u32.to_le_bytes()); // status
        r
    };
    let v2 = EntrSectionV2::from_parts(
        EntrSection {
            seed: state.seed,
            stream: state.stream,
            word_pos: state.word_pos,
        },
        &device_regs,
    )
    .unwrap();

    let mut w = ContainerWriter::new();
    w.push_section(tag::ENTR, EntrSectionV2::VERSION, &v2.encode())
        .unwrap();
    let container = w.finish();

    // Restore from the parsed container.
    let c = Container::parse(&container).unwrap();
    let sec = c.get(tag::ENTR).unwrap();
    assert_eq!(sec.sec_version, EntrSectionV2::VERSION);
    let back = EntrSectionV2::decode(sec.contents, sec.sec_version).unwrap();
    assert_eq!(back, v2);
    assert_eq!(back.device_regs(), device_regs);

    let mut restored = DetEntropy::restore(EntropyState {
        seed: back.seed,
        stream: back.stream,
        word_pos: back.word_pos,
    });

    // THE golden requirement: the next draws are bit-identical, across
    // several fills of varying (including sub-word) sizes.
    for size in [1usize, 4, 7, 64, 1000, 37] {
        let mut a = vec![0u8; size];
        let mut b = vec![0u8; size];
        original.fill(&mut a);
        restored.fill(&mut b);
        assert_eq!(a, b, "draw of {size} bytes diverged after restore");
    }
}

#[test]
fn v1_and_v2_sections_coexist_and_misuse_is_loud() {
    let prng = EntrSection {
        seed: [0x11; 32],
        stream: 5,
        word_pos: 99,
    };

    // v1 (spec-exact 56 bytes) still decodes as v1.
    let mut w = ContainerWriter::new();
    w.push_section(tag::ENTR, EntrSection::VERSION, &prng.encode())
        .unwrap();
    let bytes = w.finish();
    let c = Container::parse(&bytes).unwrap();
    let sec = c.get(tag::ENTR).unwrap();
    assert_eq!(
        EntrSection::decode(sec.contents, sec.sec_version).unwrap(),
        prng
    );
    // …and refuses to decode as v2 (wrong version AND wrong length).
    assert!(EntrSectionV2::decode(sec.contents, sec.sec_version).is_err());

    // The original landmine stays loud: a bare 16-byte device blob is not
    // an ENTR section under either version.
    assert_eq!(
        EntrSection::decode(&[0u8; 16], 1),
        Err(SectionError::BadLength { found: 16 })
    );
    assert_eq!(
        EntrSectionV2::decode(&[0u8; 16], 2),
        Err(SectionError::BadLength { found: 16 })
    );
    assert_eq!(
        EntrSectionV2::from_parts(prng, &[0u8; 15]),
        Err(SectionError::BadLength { found: 15 })
    );

    // v2 round-trips through prng()/device_regs() losslessly.
    let v2 = EntrSectionV2::from_parts(prng, &[0xAB; 16]).unwrap();
    assert_eq!(v2.prng(), prng);
    assert_eq!(v2.device_regs(), [0xAB; 16]);
    assert_eq!(
        EntrSectionV2::decode(&v2.encode(), EntrSectionV2::VERSION).unwrap(),
        v2
    );
}
