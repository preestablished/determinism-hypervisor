//! DHSNAP v1.0 golden-bytes freeze (bead 9tl) — same triple-freeze
//! discipline as the DHILOG fixtures (dh-inputlog/tests/golden.rs, bp9):
//!
//! `tests/fixtures/v1_kitchen_sink.dhsnap` carries one section per §4 tag
//! (all 11), with the engine-owned TIME/ENTR typed layouts and
//! representative owner-shaped payloads for the rest (EVTC at its v1
//! 39-byte length, PADD at 21, empty NETL/SERL per their rules).
//! `tests/fixtures/v1_minimal.dhsnap` is the empty container (header only).
//!
//! THE V1.0 CONTAINER LAYOUT FREEZES HERE: (1) checked-in fixture bytes are
//! BLAKE3-pinned, (2) today's writer re-serializes the same inputs to the
//! identical bytes, (3) the reader decodes every section to pinned values.
//! A layout change requires a format-version bump plus NEW fixture files —
//! never edit the checked-in v1.0 files, and never regenerate + re-pin the
//! hashes in the same change (that is exactly the laundering the pins
//! exist to catch).
//!
//! Section CONTENTS here are framing-representative, not authoritative
//! device states: each device freezes its own contents via its
//! snapshot/restore round-trip tests; what this file freezes is the
//! CONTAINER (header, section headers, ordering, padding) plus the
//! engine-owned TIME/ENTR layouts.
//!
//! Regenerate (only for a NEW format version, into new file names):
//! `DHSNAP_REGEN_GOLDEN=1 cargo test -p dh-snapshot --test golden`

use dh_snapshot::dhsnap::*;
use std::path::PathBuf;

/// BLAKE3 of the checked-in fixtures — the freeze anchors. If a test fails
/// here, the WRITER drifted; fix the writer, do not regenerate. Reviewers:
/// any change touching BOTH these constants and the fixture files in one
/// PR is laundering a format break — reject unless it is an explicit
/// version bump landing NEW fixture file names.
const KITCHEN_SINK_BLAKE3: &str =
    "9014b09685b48490c4e93a708ad3a56d074554174d8e008d41168571b4853a91";
const ENTR_V2_BLAKE3: &str = "17c06b954816baebff872761013d067bb43872761ac25bf86035b972d00e79cf";
const MINIMAL_BLAKE3: &str = "2e9df50e686e7d1c167d61beb669c6fadb67636d34c06807bfeb30fe50e084aa";

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// The canonical kitchen-sink build: one section per §4 tag in table
/// order, deliberately byte-order-sensitive values. This function's output
/// is what v1.0 freezes.
fn build_kitchen_sink() -> Vec<u8> {
    let mut w = ContainerWriter::new();
    // Owner-shaped representative payloads (sizes match the real owners).
    w.push_section(tag::MCFG, 1, &(1u8..=64).collect::<Vec<u8>>())
        .unwrap();
    w.push_section(
        tag::VCPU,
        1,
        &(0u8..200).map(|i| i ^ 0xA5).collect::<Vec<u8>>(),
    )
    .unwrap();
    w.push_section(tag::LAPC, 1, &[0x4C; 40]).unwrap();
    w.push_section(
        tag::TIME,
        TimeSection::VERSION,
        &TimeSection {
            cumulative_icount: 0x0102_0304_0506_0708,
            vns: 0x1112_1314_1516_1718,
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
            stream: 0x2122_2324_2526_2728,
            word_pos: 0x3132_3334_3536_3738_4142_4344_4546_4748,
        }
        .encode(),
    )
    .unwrap();
    w.push_section(tag::CLKD, 1, &[0xC1; 12]).unwrap();
    w.push_section(tag::PADD, 1, &[0xDA; 21]).unwrap();
    w.push_section(tag::EVTC, 1, &[0xEC; 39]).unwrap();
    w.push_section(tag::BLKO, 1, &[0xB0; 36]).unwrap();
    w.push_section(tag::NETL, 1, &[]).unwrap();
    w.push_section(tag::SERL, 1, &[]).unwrap();
    w.finish()
}

fn build_minimal() -> Vec<u8> {
    ContainerWriter::new().finish()
}

/// ENTR sec_version 2 (bead 6yl): a NEW fixture file — the v1 fixtures
/// above are frozen and never grow sections.
fn build_entr_v2() -> Vec<u8> {
    let v2 = EntrSectionV2 {
        seed: [0x6E; 32],
        stream: 0x0102_0304_0506_0708,
        word_pos: 0x1112_1314_1516_1718_2122_2324_2526_2728,
        buf_gpa: 0xD000_3000,
        len: 64,
        status: 1,
    };
    let mut w = ContainerWriter::new();
    w.push_section(tag::ENTR, EntrSectionV2::VERSION, &v2.encode())
        .unwrap();
    w.finish()
}

fn load_or_regen(name: &str, built: &[u8]) -> Vec<u8> {
    let path = fixture_path(name);
    if std::env::var_os("DHSNAP_REGEN_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, built).unwrap();
    }
    std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden fixture {} ({e}); see module doc",
            path.display()
        )
    })
}

#[test]
fn kitchen_sink_fixture_is_frozen() {
    let built = build_kitchen_sink();
    let fixture = load_or_regen("v1_kitchen_sink.dhsnap", &built);
    assert_eq!(
        blake3::hash(&fixture).to_hex().as_str(),
        KITCHEN_SINK_BLAKE3,
        "checked-in fixture changed — the v1.0 freeze is violated"
    );
    assert_eq!(
        built, fixture,
        "writer output drifted from the frozen v1.0 fixture"
    );
}

#[test]
fn minimal_fixture_is_frozen() {
    let built = build_minimal();
    let fixture = load_or_regen("v1_minimal.dhsnap", &built);
    assert_eq!(
        blake3::hash(&fixture).to_hex().as_str(),
        MINIMAL_BLAKE3,
        "checked-in fixture changed — the v1.0 freeze is violated"
    );
    assert_eq!(
        built, fixture,
        "writer output drifted from the frozen v1.0 fixture"
    );
}

#[test]
fn kitchen_sink_fixture_parses_to_pinned_sections() {
    let fixture = std::fs::read(fixture_path("v1_kitchen_sink.dhsnap")).unwrap();
    let c = Container::parse(&fixture).expect("frozen fixture must parse");

    let tags: Vec<[u8; 4]> = c.sections().map(|s| s.tag).collect();
    assert_eq!(tags, KNOWN_TAGS.to_vec());
    // Header version + every section's declared layout version.
    assert_eq!(&fixture[6..8], &FORMAT_VERSION.to_le_bytes());
    assert!(c.sections().all(|s| s.sec_version == 1));

    // Reader half of the freeze: every section's contents pinned. (The
    // expected-value expressions are shared with build_kitchen_sink, so
    // these asserts are decode checks; the independent anchor for the
    // BYTES is the BLAKE3 constant above.)
    assert_eq!(
        c.get(tag::MCFG).unwrap().contents,
        (1u8..=64).collect::<Vec<u8>>().as_slice()
    );
    assert_eq!(
        c.get(tag::VCPU).unwrap().contents,
        (0u8..200).map(|i| i ^ 0xA5).collect::<Vec<u8>>().as_slice()
    );
    assert_eq!(c.get(tag::LAPC).unwrap().contents, &[0x4C; 40][..]);

    let t = c.get(tag::TIME).unwrap();
    assert_eq!(
        TimeSection::decode(t.contents, t.sec_version).unwrap(),
        TimeSection {
            cumulative_icount: 0x0102_0304_0506_0708,
            vns: 0x1112_1314_1516_1718,
            epoch_index: 20,
            hash_chain: [0x44; 32],
        }
    );
    let e = c.get(tag::ENTR).unwrap();
    assert_eq!(
        EntrSection::decode(e.contents, e.sec_version).unwrap(),
        EntrSection {
            seed: [0x55; 32],
            stream: 0x2122_2324_2526_2728,
            word_pos: 0x3132_3334_3536_3738_4142_4344_4546_4748,
        }
    );

    assert_eq!(c.get(tag::CLKD).unwrap().contents, &[0xC1; 12][..]);
    assert_eq!(c.get(tag::PADD).unwrap().contents, &[0xDA; 21][..]);
    assert_eq!(c.get(tag::EVTC).unwrap().contents, &[0xEC; 39][..]);
    assert_eq!(c.get(tag::BLKO).unwrap().contents, &[0xB0; 36][..]);
    assert_eq!(c.get(tag::NETL).unwrap().contents, &[] as &[u8]);
    assert_eq!(c.get(tag::SERL).unwrap().contents, &[] as &[u8]);
}

#[test]
fn entr_v2_fixture_is_frozen_and_decodes() {
    let built = build_entr_v2();
    let fixture = load_or_regen("v1_entr_v2.dhsnap", &built);
    assert_eq!(
        blake3::hash(&fixture).to_hex().as_str(),
        ENTR_V2_BLAKE3,
        "checked-in fixture changed — the ENTR v2 freeze is violated"
    );
    assert_eq!(built, fixture, "writer output drifted from the fixture");

    let c = Container::parse(&fixture).unwrap();
    let sec = c.get(tag::ENTR).unwrap();
    assert_eq!(sec.sec_version, EntrSectionV2::VERSION);
    let v2 = EntrSectionV2::decode(sec.contents, sec.sec_version).unwrap();
    assert_eq!(v2.seed, [0x6E; 32]);
    assert_eq!(v2.stream, 0x0102_0304_0506_0708);
    assert_eq!(v2.word_pos, 0x1112_1314_1516_1718_2122_2324_2526_2728);
    assert_eq!((v2.buf_gpa, v2.len, v2.status), (0xD000_3000, 64, 1));
}

#[test]
fn minimal_fixture_parses_empty() {
    let fixture = std::fs::read(fixture_path("v1_minimal.dhsnap")).unwrap();
    let c = Container::parse(&fixture).expect("frozen fixture must parse");
    assert_eq!(c.sections().count(), 0);
    assert_eq!(fixture.len(), HEADER_LEN);
}
