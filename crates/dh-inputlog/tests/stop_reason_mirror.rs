//! The DHILOG END `stop_reason` u8 ↔ proto `StopReason` mirror pin
//! (bead sr5; API.md §3.3 "mirrors proto StopReason"). Both sides are
//! individually frozen — the proto numbers in dh-proto's pin test, the
//! byte values in this crate's golden fixtures — but nothing asserted
//! the COUPLING until now. If either side renumbers, this test names
//! the divergence; the codec itself stays a transparent u8 carrier
//! (reader.rs deliberately does not range-check the byte).

use dh_inputlog::reader::LogReader;
use dh_proto::v1::StopReason;

/// Every proto StopReason value must fit the END record's u8 slot —
/// the mirror claim is meaningless otherwise.
#[test]
fn every_proto_stop_reason_fits_the_u8_slot() {
    let mut seen = 0;
    for raw in 0..=255i32 {
        if let Ok(v) = StopReason::try_from(raw) {
            assert!(
                u8::try_from(v as i32).is_ok(),
                "{v:?} = {raw} does not fit u8"
            );
            seen += 1;
        }
    }
    // 0..=7 today (UNSPECIFIED..FAULTED); a new proto value extends this
    // count and must keep fitting the byte.
    assert_eq!(seen, 8, "proto StopReason variant count moved");
}

/// The golden fixtures' frozen END bytes decode to the INTENDED proto
/// variants: kitchen sink sealed with GOAL_SATISFIED, minimal with
/// STOP_UNSPECIFIED. This is the cross-crate coupling assertion.
#[test]
fn golden_fixture_stop_reasons_decode_to_the_intended_proto_variants() {
    for (fixture, want) in [
        (
            &include_bytes!("fixtures/v1_kitchen_sink.dhilog")[..],
            StopReason::GoalSatisfied,
        ),
        (
            &include_bytes!("fixtures/v1_minimal.dhilog")[..],
            StopReason::StopUnspecified,
        ),
    ] {
        let log = LogReader::parse(fixture).expect("golden fixture parses");
        let (stop_reason, _end_hash) = log.end();
        let decoded = StopReason::try_from(i32::from(stop_reason))
            .expect("fixture stop_reason byte is a known proto value");
        assert_eq!(decoded, want);
    }
}
