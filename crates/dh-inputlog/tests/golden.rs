//! DHILOG v1.0 golden-bytes freeze (bead bp9).
//!
//! `tests/fixtures/v1_kitchen_sink.dhilog` covers the header (incl. the
//! encoder fingerprint) and every canonical + writer-emittable AUX record
//! kind: PAD_SET, DEV_EVENT (RING_PUSH, CONS_BUMP and the PIO_ANSWER
//! detchannel encodings), NET_RX, ENTROPY, TIMER_FIRE, SDK_EVENT,
//! FRAME_MARK, END. `tests/fixtures/v1_minimal.dhilog` is the header+END
//! degenerate case.
//!
//! THE V1.0 FORMAT FREEZES HERE: these tests assert (1) the checked-in
//! fixture bytes are BLAKE3-pinned, (2) today's writer re-serializes the
//! same inputs to the identical bytes, and (3) the reader parses the
//! fixtures to the expected structure. Any layout change breaks all three
//! and requires a format-version bump plus NEW fixtures — never edit the
//! checked-in v1.0 files.
//!
//! Regenerate (only for a NEW format version, into new file names):
//! `DHILOG_REGEN_GOLDEN=1 cargo test -p dh-inputlog --test golden`

use dh_inputlog::dhilog::*;
use dh_inputlog::reader::{LogReader, RecordBody};
use std::path::PathBuf;

/// BLAKE3 of the checked-in fixtures — the freeze anchors. If a test fails
/// here, the WRITER drifted; fix the writer, do not regenerate.
const KITCHEN_SINK_BLAKE3: &str =
    "03c70c30528127f38fde719fd8b1d9ac82d4f18aa8695fab527cae19b4989491";
const MINIMAL_BLAKE3: &str = "babf7d47b461cf5dc9cbe82cdffd84e9d133fb3256c9103e386cbaa1c2bfc648";

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// The canonical kitchen-sink build: every writer-emittable kind, fixed
/// inputs. This function's output is what v1.0 freezes.
fn build_kitchen_sink() -> Vec<u8> {
    let mut w = LogWriter::new(SegmentHeader {
        base_snapshot_id: [0x11; 32],
        entropy_seed: [0x22; 32],
        machine_config_hash: [0x33; 32],
        clock_num: 3,
        clock_den: 2,
        encoder_fingerprint: 0xFEED_FACE_CAFE_BEEF,
    });
    // Canonical records.
    w.pad_set(100, 0xFFFF_8000_0000_1000, 1, 0x0000_00F0, FRAME_HINT_NONE)
        .unwrap();
    w.pad_set(150, 0xFFFF_8000_0000_1008, 0, 0x0000_000F, 7)
        .unwrap();
    // RING_PUSH: ring 1 (I), new_prod 2, eight record bytes.
    w.dev_event(
        200,
        0xFFFF_8000_0000_1010,
        DEVICE_ID_DETCHANNEL,
        EVENT_RING_PUSH,
        &[
            1, 0, 0, 0, 2, 0, 0, 0, 0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03, 0x04,
        ],
    )
    .unwrap();
    // CONS_BUMP: ring 2 (A), new_cons 5.
    w.dev_event(
        300,
        0xFFFF_8000_0000_1018,
        DEVICE_ID_DETCHANNEL,
        EVENT_CONS_BUMP,
        &[2, 0, 0, 0, 5, 0, 0, 0],
    )
    .unwrap();
    w.pio_answer(400, 0xFFFF_8000_0000_1020, 0xD370, 0x1234_5678)
        .unwrap();
    w.net_rx(500, 0xFFFF_8000_0000_1028, &[0xAA, 0xBB, 0xCC, 0xDD, 0xEE])
        .unwrap();
    // AUX records.
    w.entropy(600, 0xFFFF_8000_0000_1030, 64, 0x0102_0304_0506_0708)
        .unwrap();
    w.timer_fire(700, 0xFFFF_8000_0000_1038, 0x30, 690, 700)
        .unwrap();
    w.sdk_event(800, 0xFFFF_8000_0000_1040, 5, 256, 0x1111_2222_3333_4444)
        .unwrap();
    w.frame_mark(900, 0xFFFF_8000_0000_1048, 42).unwrap();
    w.seal(SealParams {
        end_snapshot_id: [0x44; 32],
        end_icount: 1000,
        end_vns: 1500,
        end_state_hash: [0x55; 32],
        stop_reason: 2,
    })
    .unwrap()
}

fn build_minimal() -> Vec<u8> {
    LogWriter::new(SegmentHeader {
        base_snapshot_id: [0x66; 32],
        entropy_seed: [0u8; 32], // zeros ⇒ continue base PRNG stream
        machine_config_hash: [0x77; 32],
        clock_num: 1,
        clock_den: 1,
        encoder_fingerprint: 0, // zero ⇒ no SDK digests
    })
    .seal(SealParams {
        end_snapshot_id: [0u8; 32], // zeros ⇒ no end snapshot
        end_icount: 1,
        end_vns: 1,
        end_state_hash: [0x88; 32],
        stop_reason: 0,
    })
    .unwrap()
}

fn load_or_regen(name: &str, built: &[u8]) -> Vec<u8> {
    let path = fixture_path(name);
    if std::env::var_os("DHILOG_REGEN_GOLDEN").is_some() {
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
    let fixture = load_or_regen("v1_kitchen_sink.dhilog", &built);

    // (1) The checked-in bytes are pinned.
    assert_eq!(
        blake3::hash(&fixture).to_hex().as_str(),
        KITCHEN_SINK_BLAKE3,
        "checked-in fixture changed — the v1.0 freeze is violated"
    );
    // (2) Byte-identical re-serialization: today's writer reproduces the
    // frozen bytes exactly.
    assert_eq!(
        built, fixture,
        "writer output drifted from the frozen v1.0 fixture"
    );
}

#[test]
fn minimal_fixture_is_frozen() {
    let built = build_minimal();
    let fixture = load_or_regen("v1_minimal.dhilog", &built);
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
fn kitchen_sink_fixture_parses_to_expected_structure() {
    let fixture = std::fs::read(fixture_path("v1_kitchen_sink.dhilog")).unwrap();
    let r = LogReader::parse(&fixture).expect("frozen fixture must parse");

    let h = r.header();
    assert_eq!(h.version, FORMAT_VERSION);
    assert_eq!(h.flags, FLAG_SEALED | FLAG_HAS_AUX);
    assert_eq!(h.base_snapshot_id, [0x11; 32]);
    assert_eq!(h.end_snapshot_id, [0x44; 32]);
    assert_eq!(h.entropy_seed, [0x22; 32]);
    assert_eq!(h.machine_config_hash, [0x33; 32]);
    assert_eq!((h.clock_num, h.clock_den), (3, 2));
    assert_eq!(h.record_count, 11);
    assert_eq!(h.end_icount, 1000);
    assert_eq!(h.end_vns, 1500);
    assert_eq!(h.end_state_hash, [0x55; 32]);
    assert_eq!(h.encoder_fingerprint, 0xFEED_FACE_CAFE_BEEF);

    let kinds: Vec<u8> = r.records().map(|rec| rec.kind()).collect();
    assert_eq!(
        kinds,
        vec![
            KIND_PAD_SET,
            KIND_PAD_SET,
            KIND_DEV_EVENT,
            KIND_DEV_EVENT,
            KIND_DEV_EVENT,
            KIND_NET_RX,
            KIND_ENTROPY,
            KIND_TIMER_FIRE,
            KIND_SDK_EVENT,
            KIND_FRAME_MARK,
            KIND_END,
        ]
    );

    // Spot-pin one record of each class through the typed view.
    let records: Vec<_> = r.records().collect();
    match records[1].body() {
        RecordBody::PadSet {
            port,
            buttons,
            frame_hint,
        } => assert_eq!((port, buttons, frame_hint), (0, 0x0F, 7)),
        other => panic!("expected PadSet, got {other:?}"),
    }
    match records[5].body() {
        RecordBody::NetRx { frame } => assert_eq!(frame, &[0xAA, 0xBB, 0xCC, 0xDD, 0xEE]),
        other => panic!("expected NetRx, got {other:?}"),
    }
    match records[7].body() {
        RecordBody::TimerFire {
            vector,
            armed_deadline_vns,
            delivered_icount,
        } => assert_eq!(
            (vector, armed_deadline_vns, delivered_icount),
            (0x30, 690, 700)
        ),
        other => panic!("expected TimerFire, got {other:?}"),
    }
    let (stop_reason, end_state_hash) = r.end();
    assert_eq!((stop_reason, end_state_hash), (2, [0x55; 32]));
}

#[test]
fn minimal_fixture_parses() {
    let fixture = std::fs::read(fixture_path("v1_minimal.dhilog")).unwrap();
    let r = LogReader::parse(&fixture).expect("frozen fixture must parse");
    assert_eq!(r.header().record_count, 1);
    assert_eq!(r.header().encoder_fingerprint, 0);
    assert!(!r.header().has_aux());
    assert_eq!(r.canonical().count(), 0);
}
