//! HOST-RUNNABLE interop proof for the device-exercise guest's detchannel
//! stage: build the channel page exactly as asm/device_exercise.asm does
//! (same header values via DEVICE_EXERCISE_RING_DESCS, same Beacon record
//! bytes), then run the REAL host code over it — Channel::attach must
//! accept and drain_events must yield exactly the guest's Beacon. This is
//! the test that catches a header value the host would reject (it found
//! the ring-W 0x1E0000-vs-power-of-two doc contradiction in review).

use detguest_host::{Channel, ChannelWriteSink, GuestMem, MockGuestMem, OwnedPayload};
use detguest_wire::RingId;
use nanokernel::{
    DEVICE_EXERCISE_BEACON_ID, DEVICE_EXERCISE_CHANNEL_GPA, DEVICE_EXERCISE_RING_DESCS,
};

struct NullSink;
impl ChannelWriteSink for NullSink {
    fn ring_push(&mut self, _: RingId, _: &[u8], _: u32) {}
    fn cons_bump(&mut self, _: RingId, _: u32) {}
    fn pio_answer(&mut self, _: u16, _: u32) {}
}

/// Write the page the way the guest does (offsets per the guest-sdk docs).
fn guest_built_page() -> MockGuestMem {
    let base = DEVICE_EXERCISE_CHANNEL_GPA;
    let mut gm = MockGuestMem::with_zeroed(base, 0x20_0000);

    // Header: magic "DETGUEST" LE, proto 1, flags 0, ring_desc[4].
    gm.write(base, &0x5453_4555_4754_4544u64.to_le_bytes())
        .unwrap();
    gm.write(base + 0x08, &1u32.to_le_bytes()).unwrap();
    gm.write(base + 0x0C, &0u32.to_le_bytes()).unwrap();
    for (i, (off, size)) in DEVICE_EXERCISE_RING_DESCS.iter().enumerate() {
        let at = base + 0x10 + (i as u64) * 8;
        gm.write(at, &off.to_le_bytes()).unwrap();
        gm.write(at + 4, &size.to_le_bytes()).unwrap();
    }

    // The Beacon record at ring W offset 0, exactly as the asm stores it:
    // len 24 | kind 5 | flags 0 | seq 0 | vnanos (arbitrary) | id | pad.
    let w_off = u64::from(DEVICE_EXERCISE_RING_DESCS[3].0);
    let rec = base + w_off;
    gm.write(rec, &24u16.to_le_bytes()).unwrap();
    gm.write(rec + 2, &[5u8, 0u8]).unwrap();
    gm.write(rec + 4, &0u32.to_le_bytes()).unwrap();
    gm.write(rec + 8, &0xABCDu64.to_le_bytes()).unwrap();
    gm.write(rec + 16, &DEVICE_EXERCISE_BEACON_ID.to_le_bytes())
        .unwrap();
    gm.write(rec + 20, &0u32.to_le_bytes()).unwrap();
    // Publish ringW_prod = 24 (header offset 0x280).
    gm.write(base + 0x280, &24u32.to_le_bytes()).unwrap();
    gm
}

#[test]
fn guest_header_attaches_and_beacon_drains() {
    let mut ch = Channel::attach(guest_built_page(), DEVICE_EXERCISE_CHANNEL_GPA)
        .expect("the device-exercise header must pass the real attach");
    let events = ch.drain_events(&mut NullSink).expect("drain");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].ring, RingId::W);
    assert_eq!(events[0].seq, 0);
    assert!(matches!(
        events[0].payload,
        OwnedPayload::Beacon { beacon_id } if beacon_id == DEVICE_EXERCISE_BEACON_ID
    ));
}
