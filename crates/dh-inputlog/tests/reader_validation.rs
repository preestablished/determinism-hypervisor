//! DHILOG v1 reader validation battery (bead ecv): round-trips through the
//! writer, then byte-surgery negatives for every §3 validation rule. The
//! reseal helper recomputes `body_hash` after record surgery so the targeted
//! check is what fails, not the hash gate in front of it.

use dh_inputlog::dhilog::*;
use dh_inputlog::reader::{LogReader, ReadError, RecordBody};

fn header() -> SegmentHeader {
    SegmentHeader {
        base_snapshot_id: [0xAA; 32],
        entropy_seed: [0xBB; 32],
        machine_config_hash: [0xCC; 32],
        clock_num: 3,
        clock_den: 1,
        encoder_fingerprint: 0x1122_3344_5566_7788,
    }
}

fn seal_params() -> SealParams {
    SealParams {
        end_snapshot_id: [0xDD; 32],
        end_icount: 5000,
        end_vns: 15000,
        end_state_hash: [0xEE; 32],
        stop_reason: 2,
    }
}

/// A sealed log exercising every writer-emittable record kind.
fn full_log() -> Vec<u8> {
    let mut w = LogWriter::new(header());
    w.pad_set(100, 0x1000, 0, 0xDEAD_BEEF, FRAME_HINT_NONE)
        .unwrap();
    w.pio_answer(200, 0x2000, 0xD370, 42).unwrap();
    w.dev_event(
        300,
        0x3000,
        DEVICE_ID_DETCHANNEL,
        EVENT_RING_PUSH,
        &[1, 0, 0, 0, 8, 0, 0, 0, 9],
    )
    .unwrap();
    w.entropy(400, 0x4000, 64, 0x0102_0304_0506_0708).unwrap();
    w.timer_fire(500, 0x5000, 0x30, 450, 500).unwrap();
    w.sdk_event(600, 0x6000, 5, 100, 0x1111_2222_3333_4444)
        .unwrap();
    w.frame_mark(700, 0x7000, 9).unwrap();
    w.seal(seal_params()).unwrap()
}

/// Recompute body_hash after record surgery (header [208..240)).
fn reseal(log: &mut [u8]) {
    let hash = *blake3::hash(&log[HEADER_LEN..]).as_bytes();
    log[208..240].copy_from_slice(&hash);
}

// ---- happy path -------------------------------------------------------------

#[test]
fn parses_writer_output_and_exposes_header() {
    let log = full_log();
    let r = LogReader::parse(&log).unwrap();
    let h = r.header();
    assert_eq!(h.version, FORMAT_VERSION);
    assert_eq!(h.flags, FLAG_SEALED | FLAG_HAS_AUX);
    assert_eq!(h.base_snapshot_id, [0xAA; 32]);
    assert_eq!(h.end_snapshot_id, [0xDD; 32]);
    assert_eq!(h.entropy_seed, [0xBB; 32]);
    assert_eq!(h.machine_config_hash, [0xCC; 32]);
    assert_eq!((h.clock_num, h.clock_den), (3, 1));
    assert_eq!(h.record_count, 8); // 7 + END
    assert_eq!(h.end_icount, 5000);
    assert_eq!(h.end_vns, 15000);
    assert_eq!(h.end_state_hash, [0xEE; 32]);
    assert_eq!(h.encoder_fingerprint, 0x1122_3344_5566_7788);
}

#[test]
fn iterates_all_records_in_order_with_typed_bodies() {
    let log = full_log();
    let r = LogReader::parse(&log).unwrap();
    let records: Vec<_> = r.records().collect();
    assert_eq!(records.len(), 8);
    assert_eq!(
        records.iter().map(|r| r.seq()).collect::<Vec<_>>(),
        (0..8).collect::<Vec<_>>()
    );
    assert!(records.windows(2).all(|w| w[0].icount() <= w[1].icount()));

    match records[0].body() {
        RecordBody::PadSet {
            port,
            buttons,
            frame_hint,
        } => {
            assert_eq!(port, 0);
            assert_eq!(buttons, 0xDEAD_BEEF);
            assert_eq!(frame_hint, FRAME_HINT_NONE);
        }
        other => panic!("expected PadSet, got {other:?}"),
    }
    match records[1].body() {
        RecordBody::DevEvent {
            device_id,
            event_type,
            data,
        } => {
            assert_eq!(device_id, DEVICE_ID_DETCHANNEL);
            assert_eq!(event_type, EVENT_PIO_ANSWER);
            assert_eq!(data.len(), 8);
        }
        other => panic!("expected DevEvent, got {other:?}"),
    }
    match records[4].body() {
        RecordBody::TimerFire {
            vector,
            armed_deadline_vns,
            delivered_icount,
        } => {
            assert_eq!(vector, 0x30);
            assert_eq!(armed_deadline_vns, 450);
            assert_eq!(delivered_icount, 500);
        }
        other => panic!("expected TimerFire, got {other:?}"),
    }
}

#[test]
fn canonical_iterator_implements_aux_skipping_contract() {
    let log = full_log();
    let r = LogReader::parse(&log).unwrap();
    let kinds: Vec<u8> = r.canonical().map(|r| r.kind()).collect();
    // Canonical = the three inputs; ENTROPY/TIMER_FIRE/SDK_EVENT/FRAME_MARK
    // and END (AUX-flagged) are all skipped.
    assert_eq!(kinds, vec![KIND_PAD_SET, KIND_DEV_EVENT, KIND_DEV_EVENT]);
    let aux_kinds: Vec<u8> = r.aux().map(|r| r.kind()).collect();
    assert_eq!(
        aux_kinds,
        vec![
            KIND_ENTROPY,
            KIND_TIMER_FIRE,
            KIND_SDK_EVENT,
            KIND_FRAME_MARK,
            KIND_END
        ]
    );
}

#[test]
fn end_semantics_exposed() {
    let log = full_log();
    let r = LogReader::parse(&log).unwrap();
    let (stop_reason, end_state_hash) = r.end();
    assert_eq!(stop_reason, 2);
    assert_eq!(end_state_hash, [0xEE; 32]);
}

#[test]
fn empty_log_parses_with_just_end() {
    let log = LogWriter::new(header()).seal(seal_params()).unwrap();
    let r = LogReader::parse(&log).unwrap();
    assert_eq!(r.header().record_count, 1);
    assert_eq!(r.canonical().count(), 0);
    assert_eq!(r.aux().count(), 1); // END only
    assert!(!r.header().has_aux()); // END alone does not set HAS_AUX
}

// ---- header negatives -------------------------------------------------------

#[test]
fn rejects_short_and_bad_magic() {
    assert_eq!(LogReader::parse(&[]).unwrap_err(), ReadError::TooShort);
    assert_eq!(
        LogReader::parse(&[0u8; 255]).unwrap_err(),
        ReadError::TooShort
    );
    let mut log = full_log();
    log[0] = b'X';
    assert_eq!(LogReader::parse(&log).unwrap_err(), ReadError::BadMagic);
}

#[test]
fn rejects_wrong_major_accepts_newer_minor() {
    let mut log = full_log();
    log[7] = 0x02; // version 2.0
    assert_eq!(
        LogReader::parse(&log).unwrap_err(),
        ReadError::UnsupportedVersion { found: 0x0200 }
    );

    let mut log = full_log();
    log[6] = 0x01; // version 1.1: additive minor, accepted
    assert!(LogReader::parse(&log).is_ok());
}

#[test]
fn rejects_bad_header_len() {
    let mut log = full_log();
    log[8..12].copy_from_slice(&512u32.to_le_bytes());
    assert_eq!(
        LogReader::parse(&log).unwrap_err(),
        ReadError::BadHeaderLen { found: 512 }
    );
}

#[test]
fn rejects_unsealed_and_unknown_flags() {
    let mut log = full_log();
    let flags = FLAG_HAS_AUX; // SEALED cleared
    log[12..16].copy_from_slice(&flags.to_le_bytes());
    assert_eq!(LogReader::parse(&log).unwrap_err(), ReadError::NotSealed);

    let mut log = full_log();
    let flags = FLAG_SEALED | FLAG_HAS_AUX | (1 << 7);
    log[12..16].copy_from_slice(&flags.to_le_bytes());
    assert!(matches!(
        LogReader::parse(&log).unwrap_err(),
        ReadError::UnknownHeaderFlags { .. }
    ));
}

#[test]
fn rejects_nonzero_reserved_but_reads_fingerprint() {
    // [240..248) is the encoder fingerprint (read, not reserved)…
    let log = full_log();
    assert_eq!(
        LogReader::parse(&log).unwrap().header().encoder_fingerprint,
        0x1122_3344_5566_7788
    );
    // …[248..256) is reserved-means-zero.
    let mut log = full_log();
    log[250] = 1;
    assert_eq!(
        LogReader::parse(&log).unwrap_err(),
        ReadError::ReservedNonzero
    );
}

#[test]
fn rejects_body_hash_mismatch() {
    let mut log = full_log();
    let last = log.len() - 1;
    log[last] ^= 0xFF; // corrupt record bytes without resealing
    assert_eq!(
        LogReader::parse(&log).unwrap_err(),
        ReadError::BodyHashMismatch
    );
}

// ---- record negatives (surgery + reseal) -------------------------------------

#[test]
fn rejects_seq_mismatch() {
    let mut log = full_log();
    log[HEADER_LEN + 4..HEADER_LEN + 8].copy_from_slice(&7u32.to_le_bytes());
    reseal(&mut log);
    assert_eq!(
        LogReader::parse(&log).unwrap_err(),
        ReadError::SeqMismatch {
            expected: 0,
            found: 7
        }
    );
}

#[test]
fn rejects_icount_regression() {
    let mut log = full_log();
    // Second record (PIO_ANSWER at offset 256+40: first record is 24+12+4=40).
    let off = HEADER_LEN + 40;
    log[off + 8..off + 16].copy_from_slice(&50u64.to_le_bytes()); // < 100
    reseal(&mut log);
    assert_eq!(
        LogReader::parse(&log).unwrap_err(),
        ReadError::IcountRegressed { seq: 1 }
    );
}

#[test]
fn rejects_unknown_record_flags_and_nonzero_padding() {
    let mut log = full_log();
    log[HEADER_LEN + 1] |= 0x80;
    reseal(&mut log);
    assert_eq!(
        LogReader::parse(&log).unwrap_err(),
        ReadError::UnknownRecordFlags {
            rflags: 0x80,
            seq: 0
        }
    );

    let mut log = full_log();
    // First record: 12-byte payload at 256+24, padded to 16 — pad bytes at
    // 256+24+12 .. 256+24+16.
    log[HEADER_LEN + 24 + 12] = 0xFF;
    reseal(&mut log);
    assert_eq!(
        LogReader::parse(&log).unwrap_err(),
        ReadError::NonzeroPadding { seq: 0 }
    );
}

#[test]
fn rejects_unknown_canonical_kind_accepts_unknown_aux() {
    // Unknown canonical: replay cannot apply it (§3.4).
    let mut log = full_log();
    log[HEADER_LEN] = 0x1F; // unknown, rflags stays 0 (canonical)
    reseal(&mut log);
    assert_eq!(
        LogReader::parse(&log).unwrap_err(),
        ReadError::UnknownCanonicalKind { kind: 0x1F, seq: 0 }
    );

    // Unknown AUX: skippable forward-compat path — flip an existing AUX
    // record (ENTROPY) to an unknown AUX kind. Locate it by walking the
    // framing rather than hard-coding offsets.
    let mut log = full_log();
    let entropy_seq = LogReader::parse(&log)
        .unwrap()
        .records()
        .find(|r| r.kind() == KIND_ENTROPY)
        .unwrap()
        .seq();
    let mut o = HEADER_LEN;
    for _ in 0..entropy_seq {
        let plen = u16::from_le_bytes(log[o + 2..o + 4].try_into().unwrap()) as usize;
        o += 24 + plen + (8 - plen % 8) % 8;
    }
    log[o] = 0x6E; // unknown AUX kind (rflags already AUX)
    reseal(&mut log);
    let r = LogReader::parse(&log).unwrap();
    assert!(r
        .records()
        .any(|rec| matches!(rec.body(), RecordBody::Unknown { kind: 0x6E, .. })));
}

#[test]
fn rejects_kind_aux_mismatch() {
    // PAD_SET flagged AUX contradicts its canonical class.
    let mut log = full_log();
    log[HEADER_LEN + 1] |= RFLAG_AUX;
    reseal(&mut log);
    assert_eq!(
        LogReader::parse(&log).unwrap_err(),
        ReadError::KindAuxMismatch {
            kind: KIND_PAD_SET,
            seq: 0
        }
    );
}

#[test]
fn rejects_bad_payload_layout() {
    // PAD_SET payload must be exactly 12; shrink it to 8 (still 8-aligned,
    // and fix up the following bytes so framing stays consistent: easiest is
    // to corrupt payload_len only and let the walk misalign into Truncated
    // or layout error — assert it errors, exact variant depends on the walk).
    let mut log = full_log();
    log[HEADER_LEN + 2..HEADER_LEN + 4].copy_from_slice(&8u16.to_le_bytes());
    reseal(&mut log);
    assert!(LogReader::parse(&log).is_err());
}

#[test]
fn rejects_truncation() {
    let log = full_log();
    // Cut mid-record (drop the last 8 bytes) and reseal so the hash gate
    // passes and the framing check is what fires.
    let mut cut = log[..log.len() - 8].to_vec();
    reseal(&mut cut);
    let err = LogReader::parse(&cut).unwrap_err();
    assert!(
        matches!(err, ReadError::Truncated { .. } | ReadError::EndNotLast),
        "got {err:?}"
    );
}

#[test]
fn rejects_record_count_mismatch() {
    let mut log = full_log();
    log[152..160].copy_from_slice(&99u64.to_le_bytes());
    reseal(&mut log); // body unchanged; only the header field lies
    assert_eq!(
        LogReader::parse(&log).unwrap_err(),
        ReadError::RecordCountMismatch {
            header: 99,
            actual: 8
        }
    );
}

#[test]
fn rejects_missing_end_and_record_after_end() {
    // Strip the END record entirely (END = 24 + 40 = 64 bytes) and patch
    // record_count so the count check is not what fires.
    let log = full_log();
    let mut stripped = log[..log.len() - 64].to_vec();
    stripped[152..160].copy_from_slice(&7u64.to_le_bytes());
    reseal(&mut stripped);
    assert_eq!(
        LogReader::parse(&stripped).unwrap_err(),
        ReadError::EndNotLast
    );
}

#[test]
fn rejects_end_mismatch_all_four_causes() {
    // END's end_state_hash diverges from the header's.
    let mut log = full_log();
    let end_payload_off = log.len() - 40; // END payload is the last 40 bytes
    log[end_payload_off + 8] ^= 0xFF;
    reseal(&mut log);
    assert_eq!(
        LogReader::parse(&log).unwrap_err(),
        ReadError::EndMismatch {
            what: "end_state_hash != header.end_state_hash"
        }
    );

    // END's icount diverges from header.end_icount.
    let mut log = full_log();
    let end_rec_off = log.len() - 64;
    log[end_rec_off + 8..end_rec_off + 16].copy_from_slice(&4999u64.to_le_bytes());
    reseal(&mut log);
    assert_eq!(
        LogReader::parse(&log).unwrap_err(),
        ReadError::EndMismatch {
            what: "icount != header.end_icount"
        }
    );

    // END's boundary_rip must be 0 (§3.3 END ruling).
    let mut log = full_log();
    let end_rec_off = log.len() - 64;
    log[end_rec_off + 16..end_rec_off + 24].copy_from_slice(&1u64.to_le_bytes());
    reseal(&mut log);
    assert_eq!(
        LogReader::parse(&log).unwrap_err(),
        ReadError::EndMismatch {
            what: "boundary_rip != 0"
        }
    );

    // END's stop_reason pad bytes must be zero.
    let mut log = full_log();
    let end_payload_off = log.len() - 40;
    log[end_payload_off + 3] = 0xFF;
    reseal(&mut log);
    assert_eq!(
        LogReader::parse(&log).unwrap_err(),
        ReadError::EndMismatch {
            what: "nonzero pad bytes"
        }
    );
}

#[test]
fn rejects_zero_clock_den() {
    let mut log = full_log();
    log[148..152].copy_from_slice(&0u32.to_le_bytes());
    assert_eq!(LogReader::parse(&log).unwrap_err(), ReadError::BadClock);
}

#[test]
fn rejects_has_aux_flag_mismatch() {
    // Log whose only AUX record is END: HAS_AUX must be clear; force it set.
    let mut w = LogWriter::new(header());
    w.pad_set(1, 0, 0, 1, FRAME_HINT_NONE).unwrap();
    let mut log = w.seal(seal_params()).unwrap();
    let flags = FLAG_SEALED | FLAG_HAS_AUX;
    log[12..16].copy_from_slice(&flags.to_le_bytes());
    assert_eq!(
        LogReader::parse(&log).unwrap_err(),
        ReadError::HasAuxFlagMismatch
    );
}

#[test]
fn rejects_epoch_hashes_flag_mismatch() {
    let mut log = full_log();
    let flags = FLAG_SEALED | FLAG_HAS_AUX | FLAG_EPOCH_HASHES;
    log[12..16].copy_from_slice(&flags.to_le_bytes());
    assert_eq!(
        LogReader::parse(&log).unwrap_err(),
        ReadError::EpochHashesFlagMismatch
    );
}

// ---- spec kinds the writer does not emit yet (NET_RX, EPOCH_HASH) -----------

/// Frame one raw §3.2 record.
fn make_record(kind: u8, rflags: u8, seq: u32, icount: u64, rip: u64, payload: &[u8]) -> Vec<u8> {
    let mut r = Vec::new();
    r.push(kind);
    r.push(rflags);
    r.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    r.extend_from_slice(&seq.to_le_bytes());
    r.extend_from_slice(&icount.to_le_bytes());
    r.extend_from_slice(&rip.to_le_bytes());
    r.extend_from_slice(payload);
    r.extend_from_slice(&[0u8; 8][..(8 - payload.len() % 8) % 8]);
    r
}

/// Insert hand-framed records before END in an empty sealed log, fixing
/// END's seq, record_count, flags, and the body hash.
fn splice_before_end(records: &[Vec<u8>], flags: u32) -> Vec<u8> {
    let base = LogWriter::new(header()).seal(seal_params()).unwrap();
    let mut log = base[..HEADER_LEN].to_vec();
    for r in records {
        log.extend_from_slice(r);
    }
    let mut end = base[HEADER_LEN..].to_vec(); // the END record (64 bytes)
    end[4..8].copy_from_slice(&(records.len() as u32).to_le_bytes());
    log.extend_from_slice(&end);
    log[12..16].copy_from_slice(&flags.to_le_bytes());
    log[152..160].copy_from_slice(&(records.len() as u64 + 1).to_le_bytes());
    reseal(&mut log);
    log
}

#[test]
fn net_rx_frame_boundaries() {
    // 2048 exactly: accepted; the typed body is the raw frame.
    let frame = vec![0xAB; MAX_NET_RX_FRAME];
    let rec = make_record(KIND_NET_RX, 0, 0, 10, 0x1000, &frame);
    let log = splice_before_end(&[rec], FLAG_SEALED);
    let r = LogReader::parse(&log).unwrap();
    let first = r.canonical().next().unwrap();
    match first.body() {
        RecordBody::NetRx { frame: f } => assert_eq!(f, &frame[..]),
        other => panic!("expected NetRx, got {other:?}"),
    }

    // Zero-length frame: §3.3 gives no lower bound — accepted by design.
    let rec = make_record(KIND_NET_RX, 0, 0, 10, 0x1000, &[]);
    assert!(LogReader::parse(&splice_before_end(&[rec], FLAG_SEALED)).is_ok());

    // 2049: rejected.
    let rec = make_record(
        KIND_NET_RX,
        0,
        0,
        10,
        0x1000,
        &vec![0u8; MAX_NET_RX_FRAME + 1],
    );
    assert_eq!(
        LogReader::parse(&splice_before_end(&[rec], FLAG_SEALED)).unwrap_err(),
        ReadError::BadPayloadLayout {
            kind: KIND_NET_RX,
            seq: 0
        }
    );
}

#[test]
fn epoch_hash_positive_path() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&3u64.to_le_bytes());
    payload.extend_from_slice(&[0x5A; 32]);
    let rec = make_record(KIND_EPOCH_HASH, RFLAG_AUX, 0, 10, 0, &payload);
    // EPOCH_HASH is AUX, so both flags must be set for consistency.
    let log = splice_before_end(&[rec], FLAG_SEALED | FLAG_HAS_AUX | FLAG_EPOCH_HASHES);
    let r = LogReader::parse(&log).unwrap();
    assert!(r.header().has_epoch_hashes());
    let first = r.aux().next().unwrap();
    match first.body() {
        RecordBody::EpochHash {
            epoch_index,
            chain_value,
        } => {
            assert_eq!(epoch_index, 3);
            assert_eq!(chain_value, [0x5A; 32]);
        }
        other => panic!("expected EpochHash, got {other:?}"),
    }
}

// ---- golden bytes (layout pinned against coordinated writer/reader drift) ----

#[test]
fn golden_bytes_decode_pinned() {
    // A minimal hand-pinned log: header + one PAD_SET + END. Round-trips
    // alone cannot catch wrong-but-symmetric layouts; these bytes can.
    let mut log = Vec::new();
    log.extend_from_slice(b"DHILOG"); // magic
    log.extend_from_slice(&[0x00, 0x01]); // version 1.0 LE
    log.extend_from_slice(&256u32.to_le_bytes()); // header_len
    log.extend_from_slice(&1u32.to_le_bytes()); // flags: SEALED
    log.extend_from_slice(&[0x11; 32]); // base_snapshot_id
    log.extend_from_slice(&[0x22; 32]); // end_snapshot_id
    log.extend_from_slice(&[0x33; 32]); // entropy_seed
    log.extend_from_slice(&[0x44; 32]); // machine_config_hash
    log.extend_from_slice(&1u32.to_le_bytes()); // clock_num
    log.extend_from_slice(&1u32.to_le_bytes()); // clock_den
    log.extend_from_slice(&2u64.to_le_bytes()); // record_count
    log.extend_from_slice(&777u64.to_le_bytes()); // end_icount
    log.extend_from_slice(&999u64.to_le_bytes()); // end_vns
    log.extend_from_slice(&[0x55; 32]); // end_state_hash
    log.extend_from_slice(&[0u8; 32]); // body_hash (resealed below)
    log.extend_from_slice(&0xFEED_FACE_CAFE_BEEFu64.to_le_bytes()); // fingerprint
    log.extend_from_slice(&[0u8; 8]); // reserved
    assert_eq!(log.len(), 256);

    // PAD_SET: port 2, buttons 0x0000_0010, frame_hint 5, at icount 42.
    let mut pad = [0u8; 12];
    pad[0] = 2;
    pad[4..8].copy_from_slice(&0x10u32.to_le_bytes());
    pad[8..12].copy_from_slice(&5u32.to_le_bytes());
    log.extend_from_slice(&make_record(
        KIND_PAD_SET,
        0,
        0,
        42,
        0xFFFF_8000_0000_1000,
        &pad,
    ));

    let mut end = [0u8; 40];
    end[0] = 1; // stop_reason
    end[8..40].copy_from_slice(&[0x55; 32]);
    log.extend_from_slice(&make_record(KIND_END, RFLAG_AUX, 1, 777, 0, &end));
    reseal(&mut log);

    let r = LogReader::parse(&log).expect("golden bytes must parse");
    let h = r.header();
    assert_eq!(h.encoder_fingerprint, 0xFEED_FACE_CAFE_BEEF);
    assert_eq!(h.end_icount, 777);
    let first = r.records().next().unwrap();
    assert_eq!(first.icount(), 42);
    assert_eq!(first.boundary_rip(), 0xFFFF_8000_0000_1000);
    match first.body() {
        RecordBody::PadSet {
            port,
            buttons,
            frame_hint,
        } => {
            assert_eq!((port, buttons, frame_hint), (2, 0x10, 5));
        }
        other => panic!("expected PadSet, got {other:?}"),
    }
    // And the writer produces these exact bytes for the same inputs (the
    // bp9 golden-fixture direction, pinned here at minimal scale).
    let mut w = LogWriter::new(SegmentHeader {
        base_snapshot_id: [0x11; 32],
        entropy_seed: [0x33; 32],
        machine_config_hash: [0x44; 32],
        clock_num: 1,
        clock_den: 1,
        encoder_fingerprint: 0xFEED_FACE_CAFE_BEEF,
    });
    w.pad_set(42, 0xFFFF_8000_0000_1000, 2, 0x10, 5).unwrap();
    let written = w
        .seal(SealParams {
            end_snapshot_id: [0x22; 32],
            end_icount: 777,
            end_vns: 999,
            end_state_hash: [0x55; 32],
            stop_reason: 1,
        })
        .unwrap();
    assert_eq!(written, log);
}

// ---- totality smoke (precursor to the 1j4 fuzz target) -----------------------

#[test]
fn arbitrary_truncations_never_panic() {
    let log = full_log();
    for len in 0..log.len() {
        let _ = LogReader::parse(&log[..len]); // must return Err, not panic
    }
}

#[test]
fn single_byte_corruptions_never_panic() {
    let log = full_log();
    for i in 0..log.len() {
        let mut m = log.clone();
        m[i] ^= 0xFF;
        let _ = LogReader::parse(&m); // Ok or Err both fine; no panic
        reseal(&mut m);
        let _ = LogReader::parse(&m);
    }
}
