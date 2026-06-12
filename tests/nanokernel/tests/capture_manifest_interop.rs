//! HOST-RUNNABLE interop proof for the capture fixture's manifest (bead
//! 4ws, same idea as channel_interop.rs): build the channel page exactly
//! as asm/capture_fixture.asm does — header bytes, manifest header, one
//! FRAMEBUFFER entry, one extent, the known framebuffer pattern — then
//! run the REAL host code over it. Channel::attach must accept,
//! read_manifest must resolve the region with the published
//! layout_version and flags, and read_region must walk the extent into
//! the known content. Executing the guest is HARDWARE-GATED (the M6 C5/C2
//! acceptance, bead pee); this is the byte-level contract those tests
//! stand on.

use detguest_host::{Channel, GuestMem, MockGuestMem};
use detguest_wire::header::OFF_MANIFEST;
use detguest_wire::manifest::{
    Extent, RegionEntry, MANIFEST_MAGIC, MANIFEST_VERSION, REGION_CAPACITY,
    REGION_FLAG_FRAMEBUFFER,
};
use nanokernel::{
    CAPTURE_FIXTURE_CHANNEL_GPA, CAPTURE_FIXTURE_DEFAULT_LAYOUT_VERSION, CAPTURE_FIXTURE_FB_BYTES,
    CAPTURE_FIXTURE_FB_GPA, CAPTURE_FIXTURE_FB_QWORD_BASE, CAPTURE_FIXTURE_REGION_NAME,
    DEVICE_EXERCISE_RING_DESCS,
};

/// Write channel page + framebuffer the way the guest does. The single
/// mock spans channel page, gap, and framebuffer (the fixture requires
/// mem_size >= FB_GPA + FB_BYTES; the const assert in elf_shape.rs pins
/// the framebuffer clear of the channel page, so the span is coherent).
/// `layout_version` models the cmdline knob the guest parses.
fn guest_built_memory(layout_version: u32) -> MockGuestMem {
    let base = CAPTURE_FIXTURE_CHANNEL_GPA;
    let span = (CAPTURE_FIXTURE_FB_GPA + CAPTURE_FIXTURE_FB_BYTES) - base;
    let mut gm = MockGuestMem::with_zeroed(base, span as usize);

    // Channel header: magic "DETGUEST" LE, proto 1, flags 0, ring_desc[4]
    // (the same canonical values as device_exercise — shared %defines).
    // Built by hand ON PURPOSE: the point is that the asm's literal bytes
    // pass the real attach — do not collapse this into a header-building
    // helper, or the byte-level guarantee is lost.
    gm.write(base, &0x5453_4555_4754_4544u64.to_le_bytes())
        .unwrap();
    gm.write(base + 0x08, &1u32.to_le_bytes()).unwrap();
    gm.write(base + 0x0C, &0u32.to_le_bytes()).unwrap();
    for (i, (off, size)) in DEVICE_EXERCISE_RING_DESCS.iter().enumerate() {
        let at = base + 0x10 + (i as u64) * 8;
        gm.write(at, &off.to_le_bytes()).unwrap();
        gm.write(at + 4, &size.to_le_bytes()).unwrap();
    }

    // Manifest area, byte-for-byte as the asm stores it (everything the
    // asm leaves to zeroed guest RAM stays zero here too: generation,
    // region_id, gva, extent_off, the other 63 slots, name padding).
    let m = base + OFF_MANIFEST as u64;
    gm.write(m, &MANIFEST_MAGIC.to_le_bytes()).unwrap();
    gm.write(m + 4, &MANIFEST_VERSION.to_le_bytes()).unwrap();
    gm.write(m + 6, &(REGION_CAPACITY as u16).to_le_bytes())
        .unwrap();
    gm.write(m + 16, &1u32.to_le_bytes()).unwrap(); // region_count
    gm.write(m + 20, &1u32.to_le_bytes()).unwrap(); // extent_count

    let e0 = m + RegionEntry::offset(0) as u64;
    gm.write(e0 + 4, &1u32.to_le_bytes()).unwrap(); // name_id
    gm.write(e0 + 8, &layout_version.to_le_bytes()).unwrap();
    gm.write(e0 + 12, &REGION_FLAG_FRAMEBUFFER.to_le_bytes())
        .unwrap();
    gm.write(e0 + 24, &CAPTURE_FIXTURE_FB_BYTES.to_le_bytes())
        .unwrap();
    gm.write(e0 + 36, &1u32.to_le_bytes()).unwrap(); // extent_n
    gm.write(e0 + 40, CAPTURE_FIXTURE_REGION_NAME).unwrap();

    let x0 = m + Extent::offset(0) as u64;
    gm.write(x0, &CAPTURE_FIXTURE_FB_GPA.to_le_bytes()).unwrap();
    gm.write(x0 + 8, &CAPTURE_FIXTURE_FB_BYTES.to_le_bytes())
        .unwrap();

    // The known framebuffer content: qword j = FB_QWORD_BASE + j.
    let mut fb = Vec::with_capacity(CAPTURE_FIXTURE_FB_BYTES as usize);
    for j in 0..CAPTURE_FIXTURE_FB_BYTES / 8 {
        fb.extend_from_slice(&(CAPTURE_FIXTURE_FB_QWORD_BASE + j).to_le_bytes());
    }
    gm.write(CAPTURE_FIXTURE_FB_GPA, &fb).unwrap();
    gm
}

#[test]
fn fixture_manifest_attaches_and_resolves_framebuffer() {
    let ch = Channel::attach(
        guest_built_memory(CAPTURE_FIXTURE_DEFAULT_LAYOUT_VERSION),
        CAPTURE_FIXTURE_CHANNEL_GPA,
    )
    .expect("the capture-fixture header must pass the real attach");

    let manifest = ch
        .read_manifest()
        .expect("the fixture's manifest must pass the real validated read");
    assert_eq!(manifest.header.region_count, 1);
    assert_eq!(manifest.header.extent_count, 1);
    assert_eq!(manifest.header.generation, 0, "even = quiescent");

    let region = manifest
        .resolve(core::str::from_utf8(CAPTURE_FIXTURE_REGION_NAME).unwrap())
        .expect("resolve(\"framebuffer\") must find the published region");
    assert_eq!(
        region.layout_version,
        CAPTURE_FIXTURE_DEFAULT_LAYOUT_VERSION
    );
    assert_eq!(region.flags, REGION_FLAG_FRAMEBUFFER);
    assert_eq!(region.len, CAPTURE_FIXTURE_FB_BYTES);
    assert_eq!(
        region.extents,
        vec![Extent {
            gpa: CAPTURE_FIXTURE_FB_GPA,
            len: CAPTURE_FIXTURE_FB_BYTES,
        }]
    );

    // Name resolution is an exact NUL-terminated compare, not a prefix
    // or extension match.
    assert!(manifest.resolve("framebuffe").is_none());
    assert!(manifest.resolve("framebufferX").is_none());
}

#[test]
fn read_region_walks_the_extent_into_the_known_content() {
    let ch = Channel::attach(
        guest_built_memory(CAPTURE_FIXTURE_DEFAULT_LAYOUT_VERSION),
        CAPTURE_FIXTURE_CHANNEL_GPA,
    )
    .unwrap();

    // Full-region read reproduces the pattern...
    let mut buf = vec![0u8; CAPTURE_FIXTURE_FB_BYTES as usize];
    ch.read_region("framebuffer", 0, &mut buf).unwrap();
    for (j, chunk) in buf.chunks_exact(8).enumerate() {
        assert_eq!(
            u64::from_le_bytes(chunk.try_into().unwrap()),
            CAPTURE_FIXTURE_FB_QWORD_BASE + j as u64,
            "framebuffer qword {j}"
        );
    }

    // ...an interior unaligned slice agrees with it...
    let mut slice = vec![0u8; 100];
    ch.read_region("framebuffer", 1234, &mut slice).unwrap();
    assert_eq!(&slice[..], &buf[1234..1334]);

    // ...the exact-fit final qword succeeds (no off-by-one at the end)...
    let mut last = [0u8; 8];
    ch.read_region("framebuffer", CAPTURE_FIXTURE_FB_BYTES - 8, &mut last)
        .unwrap();
    assert_eq!(
        u64::from_le_bytes(last),
        CAPTURE_FIXTURE_FB_QWORD_BASE + CAPTURE_FIXTURE_FB_BYTES / 8 - 1
    );

    // ...and a read past the region end is refused, not truncated.
    let mut over = vec![0u8; 16];
    assert!(ch
        .read_region("framebuffer", CAPTURE_FIXTURE_FB_BYTES - 8, &mut over)
        .is_err());
}

/// The cmdline knob, as the host sees it: a bumped layout_version lands
/// verbatim in the resolved region — exactly what the M6 C2 test compares
/// against an ExtractRange to produce FAILED_PRECONDITION on mismatch.
#[test]
fn bumped_layout_version_is_visible_to_resolve() {
    let ch = Channel::attach(guest_built_memory(2), CAPTURE_FIXTURE_CHANNEL_GPA).unwrap();
    let region = ch.read_manifest().unwrap().resolve("framebuffer").unwrap();
    assert_eq!(region.layout_version, 2);
    assert_ne!(
        region.layout_version,
        CAPTURE_FIXTURE_DEFAULT_LAYOUT_VERSION,
        "the bump must be distinguishable from the default"
    );
}
