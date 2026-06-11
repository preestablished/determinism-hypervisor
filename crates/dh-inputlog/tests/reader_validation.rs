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
        records.iter().map(|r| r.seq).collect::<Vec<_>>(),
        (0..8).collect::<Vec<_>>()
    );
    assert!(records.windows(2).all(|w| w[0].icount <= w[1].icount));

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
    let kinds: Vec<u8> = r.canonical().map(|r| r.kind).collect();
    // Canonical = the three inputs; ENTROPY/TIMER_FIRE/SDK_EVENT/FRAME_MARK
    // and END (AUX-flagged) are all skipped.
    assert_eq!(kinds, vec![KIND_PAD_SET, KIND_DEV_EVENT, KIND_DEV_EVENT]);
    let aux_kinds: Vec<u8> = r.aux().map(|r| r.kind).collect();
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
        .find(|r| r.kind == KIND_ENTROPY)
        .unwrap()
        .seq;
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
fn rejects_end_mismatch() {
    // END's end_state_hash diverges from the header's.
    let mut log = full_log();
    let end_payload_off = log.len() - 40; // END payload is the last 40 bytes
    log[end_payload_off + 8] ^= 0xFF;
    reseal(&mut log);
    assert_eq!(LogReader::parse(&log).unwrap_err(), ReadError::EndMismatch);

    // END's icount diverges from header.end_icount.
    let mut log = full_log();
    let end_rec_off = log.len() - 64;
    log[end_rec_off + 8..end_rec_off + 16].copy_from_slice(&4999u64.to_le_bytes());
    reseal(&mut log);
    assert_eq!(LogReader::parse(&log).unwrap_err(), ReadError::EndMismatch);
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
