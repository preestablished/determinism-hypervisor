//! The CoW contract over the ws4 fixtures, host-only (no KVM — this
//! runs on EVERY lane, including aarch64): produce the base image,
//! drive the real PvBlk + FileBase through the MMIO register protocol,
//! and assert ARCH §6.5 end to end —
//!
//!   reads return the known base patterns;
//!   the M1 write set lands ONLY in the overlay (dirty-cluster count
//!   exact); re-reads see overlay-where-written, base-everywhere-else
//!   (RMW fill preserves base bytes in dirtied clusters);
//!   the base FILE is byte-unchanged and its mtime untouched.

use std::fs::File;
use std::io::Write;

use dh_devices::blk::{
    PvBlk, CMD_READ, CMD_WRITE, REG_BUF_GPA, REG_CMD, REG_COUNT, REG_SECTOR, REG_STATUS,
    SECTORS_PER_CLUSTER, SECTOR_SIZE, STATUS_OK,
};
use dh_devices::ctx::{DevCtx, VecGuestMem};
use dh_devices::{DetDevice, EntropySource};
use dh_inputlog::dhilog::{LogWriter, SegmentHeader};
use dh_vmm::blkfile::FileBase;
use nanokernel::image;

struct NoEntropy;
impl EntropySource for NoEntropy {
    fn fill(&mut self, _buf: &mut [u8]) {}
}

/// Collision-free temp path without host randomness on the test path.
fn temp_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("dh-ws4-{}-{}.img", std::process::id(), tag))
}

/// Issue one request through the MMIO register protocol and return the
/// status read back through REG_STATUS.
fn request(dev: &mut PvBlk, mem: &mut VecGuestMem, cmd: u32, sector: u64, gpa: u64, n: u32) -> u32 {
    let mut log = LogWriter::new(SegmentHeader {
        base_snapshot_id: [0; 32],
        entropy_seed: [0; 32],
        machine_config_hash: [0; 32],
        clock_num: 1,
        clock_den: 1,
    });
    let mut ent = NoEntropy;
    let mut irqs = Vec::new();
    let mut ctx = DevCtx::new(1, 0, &mut log, mem, &mut ent, &mut irqs);
    dev.mmio_write(REG_SECTOR, &sector.to_le_bytes(), &mut ctx);
    dev.mmio_write(REG_BUF_GPA, &gpa.to_le_bytes(), &mut ctx);
    dev.mmio_write(REG_COUNT, &n.to_le_bytes(), &mut ctx);
    dev.mmio_write(REG_CMD, &cmd.to_le_bytes(), &mut ctx);
    let mut st = [0u8; 4];
    dev.mmio_read(REG_STATUS, &mut st, &mut ctx);
    u32::from_le_bytes(st)
}

#[test]
fn geometry_mirrors_dh_devices() {
    // nanokernel stays dependency-light and mirrors the blk geometry;
    // this is the drift gate (bootinfo.inc discipline).
    assert_eq!(image::IMAGE_SECTOR_SIZE, SECTOR_SIZE);
    assert_eq!(image::IMAGE_SECTORS_PER_CLUSTER, SECTORS_PER_CLUSTER);
}

#[test]
fn cow_contract_end_to_end() {
    // ---- fixture production ----
    let path = temp_path("cow");
    image::write_base_image(&path).unwrap();
    let mtime_before = std::fs::metadata(&path).unwrap().modified().unwrap();

    let base = FileBase::open(&path).unwrap();
    assert_eq!(
        base.len(),
        image::BASE_IMAGE_SECTORS * SECTOR_SIZE as u64,
        "fixture length"
    );
    let mut dev = PvBlk::new(Box::new(base));
    assert_eq!(dev.capacity_sectors(), image::BASE_IMAGE_SECTORS);
    let mut mem = VecGuestMem(vec![0u8; 64 * 1024]);

    // ---- phase 1: every sector reads back the known base pattern ----
    // 32 KiB per read, fits the guest buffer; the sweep covers every
    // sector exactly once IFF BATCH divides the sector count.
    const BATCH: u64 = 64;
    assert_eq!(image::BASE_IMAGE_SECTORS % BATCH, 0);
    for first in (0..image::BASE_IMAGE_SECTORS).step_by(BATCH as usize) {
        let st = request(&mut dev, &mut mem, CMD_READ, first, 0, BATCH as u32);
        assert_eq!(st, STATUS_OK, "base read at sector {first}");
        for sec in first..first + BATCH {
            let off = ((sec - first) as usize) * SECTOR_SIZE;
            assert_eq!(
                mem.0[off..off + SECTOR_SIZE],
                image::base_sector(sec),
                "base pattern at sector {sec}"
            );
        }
    }
    assert_eq!(dev.dirty_clusters(), 0, "reads must not populate overlay");

    // ---- phase 2: apply the M1 write set ----
    for &(first, n) in image::OVERLAY_WRITES {
        for sec in first..first + u64::from(n) {
            let off = ((sec - first) as usize) * SECTOR_SIZE;
            mem.0[off..off + SECTOR_SIZE].copy_from_slice(&image::overlay_sector(sec));
        }
        let st = request(&mut dev, &mut mem, CMD_WRITE, first, 0, n);
        assert_eq!(st, STATUS_OK, "overlay write at sector {first}");
    }
    assert_eq!(
        dev.dirty_clusters(),
        image::OVERLAY_EXPECTED_DIRTY_CLUSTERS,
        "exact dirty-cluster population"
    );

    // ---- phase 3: re-read everything; overlay where written, base
    // everywhere else (incl. unwritten sectors of dirtied clusters) ----
    for first in (0..image::BASE_IMAGE_SECTORS).step_by(BATCH as usize) {
        let st = request(&mut dev, &mut mem, CMD_READ, first, 0, BATCH as u32);
        assert_eq!(st, STATUS_OK, "post-write read at sector {first}");
        for sec in first..first + BATCH {
            let off = ((sec - first) as usize) * SECTOR_SIZE;
            assert_eq!(
                mem.0[off..off + SECTOR_SIZE],
                image::expected_sector_after_writes(sec),
                "post-write content at sector {sec}"
            );
        }
    }

    // ---- phase 3b: ONE request spanning a dirty->clean cluster
    // boundary (cluster 1 has overlay sectors, cluster 2 is untouched):
    // the device must stitch overlay-chunk + base-chunk in one read ----
    let st = request(&mut dev, &mut mem, CMD_READ, 250, 0, 16);
    assert_eq!(st, STATUS_OK, "dirty->clean boundary read");
    for sec in 250..266u64 {
        let off = ((sec - 250) as usize) * SECTOR_SIZE;
        assert_eq!(
            mem.0[off..off + SECTOR_SIZE],
            image::expected_sector_after_writes(sec),
            "boundary-mix content at sector {sec}"
        );
    }

    // ---- phase 4: the base file never changed ----
    let meta = std::fs::metadata(&path).unwrap();
    assert_eq!(
        meta.modified().unwrap(),
        mtime_before,
        "base mtime must be untouched"
    );
    let on_disk = std::fs::read(&path).unwrap();
    assert_eq!(
        *blake3::hash(&on_disk).as_bytes(),
        image::BASE_IMAGE_BLAKE3,
        "base bytes must be byte-identical to the recorded content hash"
    );
    std::fs::remove_file(&path).ok();
}

#[test]
fn base_image_file_is_what_filebase_serves() {
    // FileBase (pread, zero-fill past EOF) over the fixture must serve
    // exactly the in-memory generator's bytes.
    use dh_devices::blk::BlockBase;
    let path = temp_path("serve");
    let mut f = File::create(&path).unwrap();
    f.write_all(&image::base_image()).unwrap();
    f.sync_all().unwrap();
    drop(f);
    let base = FileBase::open(&path).unwrap();
    let mut buf = vec![0u8; 3 * SECTOR_SIZE];
    base.read_at(5 * SECTOR_SIZE as u64, &mut buf).unwrap();
    assert_eq!(buf[..SECTOR_SIZE], image::base_sector(5));
    assert_eq!(buf[SECTOR_SIZE..2 * SECTOR_SIZE], image::base_sector(6));
    assert_eq!(buf[2 * SECTOR_SIZE..], image::base_sector(7));
    std::fs::remove_file(&path).ok();
}
