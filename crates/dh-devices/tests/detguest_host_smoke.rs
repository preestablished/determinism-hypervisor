//! Smoke test for the guest-sdk Milestone-1 host API surface this repo
//! consumes (bead determinism-hypervisor-2w8): `Channel::attach`,
//! `drain_events`, `push_command`, `read_manifest` (seqlock retry),
//! `read_region`, `ChannelWriteSink`, `InjectResponder`.
//!
//! This is a linkage + contract check over `MockGuestMem`, not a detchannel
//! implementation — that lands in `crates/dh-devices/src/detchannel*`
//! (bead determinism-hypervisor-nln).

use detguest_host::{
    AttachError, Channel, GuestMem, InjectResponder, LogFaultPlan, MockGuestMem, RecordingSink,
    RegionReadError, SinkOp, WireError,
};
use detguest_wire::events::QuiesceMode;
use detguest_wire::header::{OFF_MANIFEST, OFF_RESERVED};
use detguest_wire::manifest::{ManifestHeader, MANIFEST_MAGIC, MANIFEST_VERSION, REGION_CAPACITY};
use detguest_wire::ports::PORT_INJECT;
use detguest_wire::{ChannelHeader, Command, RingId, CHANNEL_SIZE};

const BASE: u64 = 0x1000_0000;

/// A 2 MiB channel page with the canonical header and a valid empty manifest.
fn fresh_channel_mem() -> MockGuestMem {
    let mut gm = MockGuestMem::with_zeroed(BASE, CHANNEL_SIZE);

    let mut hdr = [0u8; OFF_RESERVED];
    ChannelHeader::canonical().write_to(&mut hdr).unwrap();
    gm.write(BASE, &hdr).unwrap();

    let manifest = ManifestHeader {
        magic: MANIFEST_MAGIC,
        manifest_version: MANIFEST_VERSION,
        region_capacity: REGION_CAPACITY as u16,
        generation: 0,
        region_count: 0,
        extent_count: 0,
    };
    let mut m = [0u8; 32];
    manifest.write_to(&mut m).unwrap();
    gm.write(BASE + OFF_MANIFEST as u64, &m).unwrap();

    gm
}

#[test]
fn attach_validates_canonical_header() {
    let ch = Channel::attach(fresh_channel_mem(), BASE).unwrap();
    assert_eq!(ch.base_gpa(), BASE);
    assert_eq!(ch.header().proto_version, detguest_wire::PROTO_VERSION);

    // Unmapped GPA refuses with the status code CHANNEL_INIT reports.
    let err = Channel::attach(MockGuestMem::new(), BASE).unwrap_err();
    assert!(matches!(err, AttachError::Mem(_)));
}

#[test]
fn drain_events_on_empty_rings_yields_nothing() {
    let mut ch = Channel::attach(fresh_channel_mem(), BASE).unwrap();
    let mut sink = RecordingSink::default();
    let events = ch.drain_events(&mut sink).unwrap();
    assert!(events.is_empty());
    // Nothing drained → no consumer bumps either.
    assert!(sink.ops.is_empty());
}

#[test]
fn push_command_reports_ring_write_through_sink() {
    let mut ch = Channel::attach(fresh_channel_mem(), BASE).unwrap();
    let mut sink = RecordingSink::default();
    ch.push_command(
        &Command::Quiesce {
            token: 7,
            mode: QuiesceMode::Coop,
        },
        &mut sink,
    )
    .unwrap();

    // The host mutation surfaced as exactly one ring C push with a published
    // producer index — the invariant the input log depends on.
    assert_eq!(sink.ops.len(), 1);
    assert!(matches!(
        &sink.ops[0],
        SinkOp::RingPush { ring: RingId::C, new_prod, bytes }
            if *new_prod > 0 && !bytes.is_empty()
    ));
    assert_eq!(ch.producer_seqs().ring_c, 1);
}

#[test]
fn read_manifest_snapshots_and_retries_seqlock() {
    let ch = Channel::attach(fresh_channel_mem(), BASE).unwrap();
    let m = ch.read_manifest().unwrap();
    assert_eq!(m.header.region_count, 0);
    // All 64 slots are present (dead entries keep their slots); nothing
    // resolves by name in an empty manifest.
    assert_eq!(m.entries.len(), REGION_CAPACITY);
    assert!(m.resolve("telemetry").is_none());

    // Generation stuck odd (writer mid-update forever) → bounded retry, then
    // SeqlockLivelock instead of a hang.
    let mut gm = fresh_channel_mem();
    let manifest = ManifestHeader {
        magic: MANIFEST_MAGIC,
        manifest_version: MANIFEST_VERSION,
        region_capacity: REGION_CAPACITY as u16,
        generation: 1,
        region_count: 0,
        extent_count: 0,
    };
    let mut m = [0u8; 32];
    manifest.write_to(&mut m).unwrap();
    gm.write(BASE + OFF_MANIFEST as u64, &m).unwrap();
    let ch = Channel::attach(gm, BASE).unwrap();
    assert!(matches!(
        ch.read_manifest(),
        Err(WireError::SeqlockLivelock)
    ));
}

#[test]
fn read_region_resolves_by_name() {
    let ch = Channel::attach(fresh_channel_mem(), BASE).unwrap();
    let mut buf = [0u8; 8];
    assert!(matches!(
        ch.read_region("telemetry", 0, &mut buf),
        Err(RegionReadError::NameNotFound)
    ));
}

#[test]
fn inject_responder_answers_via_pio_sink() {
    let mut ch = Channel::attach(fresh_channel_mem(), BASE).unwrap();
    let mut responder = InjectResponder::new(LogFaultPlan::default());
    let mut sink = RecordingSink::default();

    // No drained InjectQuery for iseq 0 → Proceed (0) + warning metric.
    let value = responder.answer(&mut ch, 0, &mut sink);
    assert_eq!(value, 0);
    assert_eq!(ch.unmatched_injects, 1);
    assert_eq!(
        sink.ops.last(),
        Some(&SinkOp::PioAnswer {
            port: PORT_INJECT,
            value: 0
        })
    );
}
