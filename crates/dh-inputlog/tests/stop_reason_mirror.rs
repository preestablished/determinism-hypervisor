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
/// the mirror claim is meaningless otherwise. prost's `TryFrom<i32>` is
/// CLOSED over the generated set, so a count pin scanned over 0..=255
/// would never even SEE a future variant numbered ≥256 (the exact kind
/// that breaks the mirror) — pin the maximum wire number over a wide
/// scan instead. Residual blind spot: a value above u16::MAX; proto
/// enum numbers that large would also have to dodge dh-proto's
/// per-variant pin test, where every new value is added by convention.
#[test]
fn every_proto_stop_reason_fits_the_u8_slot() {
    let known: Vec<i32> = (0..=i32::from(u16::MAX))
        .filter(|raw| StopReason::try_from(*raw).is_ok())
        .collect();
    assert_eq!(
        known.last().copied(),
        Some(7),
        "max StopReason wire number moved — re-prove it fits the END u8"
    );
    assert_eq!(known, (0..=7).collect::<Vec<_>>(), "set is 0..=7, gapless");
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
