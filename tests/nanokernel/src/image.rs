//! pv-blk base image + CoW overlay fixtures (bead ws4; ARCH §6.5).
//!
//! The base image is generated DETERMINISTICALLY (no host state, no
//! randomness): its content hash is a workspace constant
//! ([`BASE_IMAGE_BLAKE3`], drift-gated by a unit test) suitable for
//! recording in MachineConfig. Sector content is asm-verifiable by a
//! guest: the first 8 bytes are the LE sector index, the rest follow a
//! two-multiply byte formula (see [`base_sector`]).
//!
//! The overlay fixture ([`OVERLAY_WRITES`]) is the M1-acceptance write
//! set: writes that must land ONLY in the CoW overlay (the base file's
//! bytes and mtime must not change — consumption tests assert both).
//! The set deliberately exercises: a single sector in cluster 0, a
//! write crossing the cluster 0→1 boundary, a second write into an
//! already-dirtied cluster (RMW reuse), and the exact image tail.

/// Mirrors dh-devices::blk geometry (drift-asserted by the consumption
/// test in dh-vmm — nanokernel stays dependency-light by design).
pub const IMAGE_SECTOR_SIZE: usize = 512;
pub const IMAGE_SECTORS_PER_CLUSTER: u64 = 128;

/// 1 MiB base image: 2,048 sectors = 16 CoW clusters.
pub const BASE_IMAGE_SECTORS: u64 = 2048;

/// blake3 of the full generated base image — the MachineConfig
/// content-hash fixture. A unit test regenerates and compares, so any
/// generator drift fails the build's test gate.
pub const BASE_IMAGE_BLAKE3: [u8; 32] = [
    0x5d, 0x22, 0x16, 0x07, 0x97, 0x75, 0x3e, 0x1e, 0xf9, 0x84, 0x4e, 0xae, 0x15, 0xf8, 0xd4, 0x90,
    0xc7, 0x02, 0x54, 0x7f, 0x34, 0xdd, 0x6a, 0x4f, 0xb0, 0x83, 0x59, 0x8c, 0x6c, 0x20, 0xe8, 0x5f,
];

/// One base sector: bytes 0..8 = LE sector index (a guest checks its
/// read landed with one qword compare); bytes 8.. follow
/// `(sector*167 + i*13 + 5) & 0xFF` — two multiplies and an add, cheap
/// to spot-check from asm.
pub fn base_sector(sector: u64) -> [u8; IMAGE_SECTOR_SIZE] {
    let mut s = [0u8; IMAGE_SECTOR_SIZE];
    s[..8].copy_from_slice(&sector.to_le_bytes());
    for (i, b) in s.iter_mut().enumerate().skip(8) {
        *b = (sector
            .wrapping_mul(167)
            .wrapping_add(i as u64 * 13)
            .wrapping_add(5)) as u8;
    }
    s
}

/// The full base image, in memory.
pub fn base_image() -> Vec<u8> {
    let mut v = Vec::with_capacity(BASE_IMAGE_SECTORS as usize * IMAGE_SECTOR_SIZE);
    for sector in 0..BASE_IMAGE_SECTORS {
        v.extend_from_slice(&base_sector(sector));
    }
    v
}

/// Produce the base image file (the build/fixture-production step).
pub fn write_base_image(path: &std::path::Path) -> std::io::Result<()> {
    std::fs::write(path, base_image())
}

/// The M1 overlay write set as (first_sector, sector_count), strictly
/// non-overlapping, in capacity:
///   (3, 1)    — lone sector, cluster 0
///   (127, 2)  — crosses the cluster 0→1 boundary
///   (130, 1)  — cluster 1 again (already dirtied: RMW-reuse case)
///   (2040, 8) — the exact image tail (ends at capacity)
pub const OVERLAY_WRITES: &[(u64, u32)] = &[(3, 1), (127, 2), (130, 1), (2040, 8)];

/// Clusters the write set dirties: {0, 1, 15} (derived in a unit test
/// from OVERLAY_WRITES; hand value here for consumers).
pub const OVERLAY_EXPECTED_DIRTY_CLUSTERS: usize = 3;

/// One overlay-written sector: first 8 bytes = LE bitwise-NOT of the
/// sector index (cannot collide with any base sector header), the rest
/// `(sector*89 + i*31 + 11) & 0xFF`.
pub fn overlay_sector(sector: u64) -> [u8; IMAGE_SECTOR_SIZE] {
    let mut s = [0u8; IMAGE_SECTOR_SIZE];
    s[..8].copy_from_slice(&(!sector).to_le_bytes());
    for (i, b) in s.iter_mut().enumerate().skip(8) {
        *b = (sector
            .wrapping_mul(89)
            .wrapping_add(i as u64 * 31)
            .wrapping_add(11)) as u8;
    }
    s
}

/// Whether `sector` is covered by [`OVERLAY_WRITES`].
pub fn sector_is_overlay_written(sector: u64) -> bool {
    OVERLAY_WRITES
        .iter()
        .any(|&(s, n)| sector >= s && sector < s + u64::from(n))
}

/// Expected read-back of `sector` AFTER the write set is applied:
/// overlay content where written, base content everywhere else —
/// including unwritten sectors of dirtied clusters (the RMW fill must
/// preserve base bytes exactly).
pub fn expected_sector_after_writes(sector: u64) -> [u8; IMAGE_SECTOR_SIZE] {
    if sector_is_overlay_written(sector) {
        overlay_sector(sector)
    } else {
        base_sector(sector)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generator_is_deterministic_and_hash_gated() {
        let a = base_image();
        let b = base_image();
        assert_eq!(a, b, "generation must be bit-identical");
        assert_eq!(
            a.len(),
            BASE_IMAGE_SECTORS as usize * IMAGE_SECTOR_SIZE,
            "1 MiB"
        );
        assert_eq!(
            *blake3::hash(&a).as_bytes(),
            BASE_IMAGE_BLAKE3,
            "generator drifted from the recorded MachineConfig hash"
        );
    }

    #[test]
    fn sector_headers_are_unique_and_patterns_differ() {
        // The LE-index header makes every sector distinct even where the
        // byte formula aliases (period 256 in `sector`).
        assert_ne!(base_sector(0), base_sector(256));
        assert_ne!(base_sector(3)[..8], overlay_sector(3)[..8]);
        // Overlay headers can't collide with ANY base header: base
        // headers are < BASE_IMAGE_SECTORS, overlay headers are !sector
        // (top bits set).
        for &(s, n) in OVERLAY_WRITES {
            for sec in s..s + u64::from(n) {
                let hdr = u64::from_le_bytes(overlay_sector(sec)[..8].try_into().unwrap());
                assert!(hdr >= BASE_IMAGE_SECTORS, "header {hdr:#x} aliases base");
            }
        }
    }

    #[test]
    fn write_set_is_in_capacity_nonoverlapping_and_dirty_count_matches() {
        let mut covered = std::collections::BTreeSet::new();
        let mut clusters = std::collections::BTreeSet::new();
        for &(s, n) in OVERLAY_WRITES {
            assert!(n > 0);
            let end = s + u64::from(n);
            assert!(end <= BASE_IMAGE_SECTORS, "write past capacity");
            for sec in s..end {
                assert!(covered.insert(sec), "write set overlaps at sector {sec}");
                clusters.insert(sec / IMAGE_SECTORS_PER_CLUSTER);
            }
        }
        assert_eq!(clusters.len(), OVERLAY_EXPECTED_DIRTY_CLUSTERS);
        assert_eq!(
            clusters.into_iter().collect::<Vec<_>>(),
            vec![0, 1, 15],
            "the documented cluster set"
        );
    }

    #[test]
    fn file_production_round_trips() {
        let p = std::env::temp_dir().join(format!("dh-ws4-base-{}.img", std::process::id()));
        write_base_image(&p).unwrap();
        let on_disk = std::fs::read(&p).unwrap();
        std::fs::remove_file(&p).ok();
        assert_eq!(on_disk, base_image());
    }
}
