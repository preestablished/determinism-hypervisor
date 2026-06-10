//! Smoke test for the guest-sdk Milestone-1 host API surface this repo
//! consumes (bead determinism-hypervisor-2w8): `Channel::attach`,
//! `drain_events`, `push_command`, `read_manifest` (seqlock retry),
//! `read_region`, `ChannelWriteSink`, `InjectResponder`.
//!
//! This is a linkage + contract check over `MockGuestMem`, not a detchannel
//! implementation — that lands in `crates/dh-devices/src/detchannel*`
//! (bead determinism-hypervisor-nln).

use std::cell::Cell;

// `GuestMem` is load-bearing: it brings the `read()`/`write()` trait methods
// on `MockGuestMem` into scope.
use detguest_host::{
    AttachError, Channel, GuestMem, InjectResponder, LogFaultPlan, MemError, MockGuestMem,
    OwnedPayload, RecordingSink, RegionReadError, SinkOp, WireError,
};
use detguest_wire::events::{encode_event, EventPayload, QuiesceMode};
use detguest_wire::header::{OFF_MANIFEST, OFF_RESERVED};
use detguest_wire::manifest::{
    init_manifest, writer_begin, Extent, ManifestHeader, RegionEntry, MANIFEST_TOTAL_SIZE,
    REGION_CAPACITY,
};
use detguest_wire::ports::PORT_INJECT;
use detguest_wire::{ChannelHeader, Command, RingId, CHANNEL_SIZE};

const BASE: u64 = 0x1000_0000;

/// A 2 MiB channel page with the canonical header and an empty manifest
/// (generation 0 — even, readable).
fn fresh_channel_mem() -> MockGuestMem {
    let mut gm = MockGuestMem::with_zeroed(BASE, CHANNEL_SIZE);

    let mut hdr = [0u8; OFF_RESERVED];
    ChannelHeader::canonical().write_to(&mut hdr).unwrap();
    gm.write(BASE, &hdr).unwrap();

    let mut area = vec![0u8; MANIFEST_TOTAL_SIZE];
    init_manifest(&mut area).unwrap();
    gm.write(BASE + OFF_MANIFEST as u64, &area).unwrap();

    gm
}

/// Overwrite the manifest area wholesale (the test is the "agent" here;
/// seqlock discipline is exercised separately).
fn install_manifest(gm: &mut MockGuestMem, area: &[u8]) {
    gm.write(BASE + OFF_MANIFEST as u64, area).unwrap();
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
fn drain_events_returns_guest_records_and_bumps_consumer() {
    // Lay one NameIntern record at ring W offset 0 and publish the producer
    // index, the way the in-guest SDK would.
    let mut gm = fresh_channel_mem();
    let desc = RingId::W.canonical_desc();
    let mut buf = [0u8; 4096];
    let n = encode_event(
        &mut buf,
        0,
        1,
        0,
        &EventPayload::NameIntern {
            name_id: 1,
            name: b"io_read",
        },
    )
    .unwrap();
    gm.write(BASE + desc.offset as u64, &buf[..n]).unwrap();
    gm.write(
        BASE + RingId::W.prod_offset() as u64,
        &(n as u32).to_le_bytes(),
    )
    .unwrap();

    let mut ch = Channel::attach(gm, BASE).unwrap();
    let mut sink = RecordingSink::default();
    let events = ch.drain_events(&mut sink).unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].ring, RingId::W);
    assert!(matches!(
        &events[0].payload,
        OwnedPayload::NameIntern { name_id: 1, name, .. } if name == b"io_read"
    ));
    // The intern folded into the channel's name table, and the consumer bump
    // surfaced through the sink (it mutates channel memory → input log).
    assert_eq!(ch.intern_name(1), Some("io_read"));
    assert!(sink
        .ops
        .iter()
        .any(|op| matches!(op, SinkOp::ConsBump { ring: RingId::W, new_cons } if *new_cons == n as u32)));
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
fn read_manifest_snapshots_empty_manifest() {
    let ch = Channel::attach(fresh_channel_mem(), BASE).unwrap();
    let m = ch.read_manifest().unwrap();
    assert_eq!(m.header.region_count, 0);
    // All 64 slots are present; zeroed entries are live-but-nameless
    // (flags == 0; DEAD is bit 31), so nothing resolves by name.
    assert_eq!(m.entries.len(), REGION_CAPACITY);
    assert!(m.resolve("telemetry").is_none());
}

#[test]
fn read_manifest_bounds_a_stuck_seqlock() {
    // Writer enters the seqlock (odd generation) and never finishes →
    // bounded retry, then SeqlockLivelock instead of a hang.
    let mut gm = fresh_channel_mem();
    let mut area = vec![0u8; MANIFEST_TOTAL_SIZE];
    init_manifest(&mut area).unwrap();
    writer_begin(&mut area).unwrap();
    install_manifest(&mut gm, &area);
    let ch = Channel::attach(gm, BASE).unwrap();
    assert!(matches!(
        ch.read_manifest(),
        Err(WireError::SeqlockLivelock)
    ));
}

/// `GuestMem` whose first read of the generation word reports the writer
/// mid-update (odd), after which the underlying (even) value shows through —
/// the transient `read_manifest`'s retry loop exists for.
struct WriterFinishes {
    inner: MockGuestMem,
    gen_gpa: u64,
    served_odd: Cell<bool>,
}

impl GuestMem for WriterFinishes {
    fn read(&self, gpa: u64, buf: &mut [u8]) -> Result<(), MemError> {
        self.inner.read(gpa, buf)?;
        if gpa == self.gen_gpa && buf.len() == 8 && !self.served_odd.replace(true) {
            let odd = u64::from_le_bytes(buf[..8].try_into().unwrap()) + 1;
            buf.copy_from_slice(&odd.to_le_bytes());
        }
        Ok(())
    }
    fn write(&mut self, gpa: u64, buf: &[u8]) -> Result<(), MemError> {
        self.inner.write(gpa, buf)
    }
}

#[test]
fn read_manifest_retries_past_a_transient_writer() {
    use detguest_wire::manifest::OFF_GENERATION;
    let gm = WriterFinishes {
        inner: fresh_channel_mem(),
        gen_gpa: BASE + (OFF_MANIFEST + OFF_GENERATION) as u64,
        served_odd: Cell::new(false),
    };
    let ch = Channel::attach(gm, BASE).unwrap();
    let m = ch.read_manifest().unwrap();
    assert_eq!(m.header.region_count, 0);
    assert!(ch.guest_mem().served_odd.get(), "retry path never exercised");
}

#[test]
fn read_region_rejects_unknown_name() {
    let ch = Channel::attach(fresh_channel_mem(), BASE).unwrap();
    let mut buf = [0u8; 8];
    assert!(matches!(
        ch.read_region("telemetry", 0, &mut buf),
        Err(RegionReadError::NameNotFound)
    ));
}

#[test]
fn read_region_returns_bytes_for_a_live_region() {
    const SEG: u64 = 0x4000_0000;
    let mut gm = fresh_channel_mem();

    // Publish one 8-byte region named "telemetry" backed by a single extent
    // in a separate guest-RAM segment.
    let mut area = vec![0u8; MANIFEST_TOTAL_SIZE];
    init_manifest(&mut area).unwrap();
    RegionEntry {
        region_id: 0,
        name_id: 1,
        layout_version: 1,
        flags: 0,
        gva: 0,
        len: 8,
        extent_off: 0,
        extent_n: 1,
        name: RegionEntry::pack_name(b"telemetry").unwrap(),
    }
    .write_to(&mut area, 0)
    .unwrap();
    Extent { gpa: SEG, len: 8 }.write_to(&mut area, 0).unwrap();
    let mut hdr = ManifestHeader::read_from(&area).unwrap();
    hdr.region_count = 1;
    hdr.extent_count = 1;
    hdr.write_to(&mut area).unwrap();
    install_manifest(&mut gm, &area);
    gm.add_segment(SEG, (0u8..8).collect());

    let ch = Channel::attach(gm, BASE).unwrap();
    let mut buf = [0u8; 8];
    ch.read_region("telemetry", 0, &mut buf).unwrap();
    assert_eq!(buf, [0, 1, 2, 3, 4, 5, 6, 7]);

    // Past-the-end reads are rejected, not truncated.
    let mut big = [0u8; 9];
    assert!(matches!(
        ch.read_region("telemetry", 1, &mut big[..8]),
        Err(RegionReadError::OutOfBounds)
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
