//! File-backed pv-blk base image (ARCH §6.5).
//!
//! The production [`dh_devices::blk::BlockBase`]: the base image opened
//! `O_RDONLY` — the fd cannot write, so the CoW contract ("base bytes and
//! mtime never change") holds by construction. The image's content hash is
//! recorded in MachineConfig (config bead); this module only serves bytes.
//!
//! Lives here rather than dh-devices because that crate's deny-list
//! forbids host I/O — the device model sees only the [`BlockBase`] seam.

use dh_devices::blk::{BaseIoError, BlockBase};
use std::fs::File;
use std::os::unix::fs::FileExt;
use std::path::Path;

pub struct FileBase {
    file: File,
    len: u64,
}

impl FileBase {
    /// Open the base image read-only. The fd keeps no append/write modes;
    /// every read goes through positional `pread` (no shared cursor).
    pub fn open(path: &Path) -> std::io::Result<FileBase> {
        let file = File::open(path)?; // read-only by definition
        let len = file.metadata()?.len();
        Ok(FileBase { file, len })
    }

    /// Base image length in bytes.
    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl BlockBase for FileBase {
    fn len_bytes(&self) -> u64 {
        self.len
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), BaseIoError> {
        // Zero-fill past EOF per the BlockBase contract (cluster RMW may
        // extend past a non-cluster-aligned image tail).
        buf.fill(0);
        if offset >= self.len {
            return Ok(());
        }
        let take =
            usize::try_from((self.len - offset).min(buf.len() as u64)).map_err(|_| BaseIoError)?;
        self.file
            .read_exact_at(&mut buf[..take], offset)
            .map_err(|_| BaseIoError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dh_devices::blk::{
        PvBlk, CMD_READ, CMD_WRITE, REG_BUF_GPA, REG_CMD, REG_COUNT, REG_SECTOR, REG_STATUS,
        SECTOR_SIZE, STATUS_OK,
    };
    use dh_devices::ctx::VecGuestMem;
    use dh_devices::{DetDevice, EntropySource};
    use dh_inputlog::dhilog::{LogWriter, SegmentHeader};
    use std::io::Write;

    struct NoEntropy;
    impl EntropySource for NoEntropy {
        fn fill(&mut self, _buf: &mut [u8]) {}
    }

    /// Collision-free temp path without host randomness on the test path.
    fn temp_image(content: &[u8], tag: &str) -> std::path::PathBuf {
        let p =
            std::env::temp_dir().join(format!("dh-blk-base-{}-{}.img", std::process::id(), tag));
        let mut f = File::create(&p).unwrap();
        f.write_all(content).unwrap();
        f.sync_all().unwrap();
        p
    }

    fn request(
        dev: &mut PvBlk,
        mem: &mut VecGuestMem,
        cmd: u32,
        sector: u64,
        gpa: u64,
        n: u32,
    ) -> u32 {
        let mut log = LogWriter::new(SegmentHeader {
            base_snapshot_id: [0; 32],
            entropy_seed: [0; 32],
            machine_config_hash: [0; 32],
            clock_num: 1,
            clock_den: 1,
            encoder_fingerprint: 0,
        });
        let mut ent = NoEntropy;
        let mut q = Vec::new();
        let mut ctx = dh_devices::DevCtx::new(1, 0, &mut log, mem, &mut ent, &mut q);
        dev.mmio_write(REG_SECTOR, &sector.to_le_bytes(), &mut ctx);
        dev.mmio_write(REG_BUF_GPA, &gpa.to_le_bytes(), &mut ctx);
        dev.mmio_write(REG_COUNT, &n.to_le_bytes(), &mut ctx);
        dev.mmio_write(REG_CMD, &cmd.to_le_bytes(), &mut ctx);
        let mut s = [0u8; 4];
        dev.mmio_read(REG_STATUS, &mut s, &mut ctx);
        u32::from_le_bytes(s)
    }

    /// The §6.5 acceptance: after guest writes through the device, the
    /// base image's BYTES and MTIME are unchanged (CoW never touches it).
    #[test]
    fn base_file_bytes_and_mtime_unchanged_after_writes() {
        let content: Vec<u8> = (0..4 * SECTOR_SIZE).map(|i| (i % 253) as u8).collect();
        let path = temp_image(&content, "mtime");
        let mtime_before = std::fs::metadata(&path).unwrap().modified().unwrap();

        {
            let mut dev = PvBlk::new(Box::new(FileBase::open(&path).unwrap()));
            let mut mem = VecGuestMem(vec![0u8; 0x4000]);
            mem.0[0x1000..0x1000 + 2 * SECTOR_SIZE].fill(0xEE);
            assert_eq!(
                request(&mut dev, &mut mem, CMD_WRITE, 1, 0x1000, 2),
                STATUS_OK
            );
            // Read-back through the device sees the overlay…
            assert_eq!(
                request(&mut dev, &mut mem, CMD_READ, 1, 0x3000, 1),
                STATUS_OK
            );
            assert!(mem.0[0x3000..0x3000 + SECTOR_SIZE]
                .iter()
                .all(|&b| b == 0xEE));
            assert_eq!(dev.dirty_clusters(), 1);
        }

        // …but the file on disk is bit-identical, mtime included.
        assert_eq!(std::fs::read(&path).unwrap(), content);
        let mtime_after = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(mtime_before, mtime_after);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn reads_serve_file_content_and_zero_fill_past_eof() {
        // 2.5 sectors: capacity floors to 2; the cluster RMW path reads
        // past EOF internally and must see zeros there.
        let content: Vec<u8> = (0..SECTOR_SIZE * 5 / 2).map(|i| (i % 241) as u8).collect();
        let path = temp_image(&content, "eof");
        let base = FileBase::open(&path).unwrap();
        assert_eq!(base.len(), content.len() as u64);

        let mut dev = PvBlk::new(Box::new(base));
        assert_eq!(dev.capacity_sectors(), 2);
        let mut mem = VecGuestMem(vec![0u8; 0x4000]);
        assert_eq!(
            request(&mut dev, &mut mem, CMD_READ, 0, 0x1000, 2),
            STATUS_OK
        );
        assert_eq!(
            &mem.0[0x1000..0x1000 + 2 * SECTOR_SIZE],
            &content[..2 * SECTOR_SIZE]
        );

        // A write populates the cluster via RMW that spans past EOF.
        mem.0[0x2000..0x2000 + SECTOR_SIZE].fill(0x11);
        assert_eq!(
            request(&mut dev, &mut mem, CMD_WRITE, 0, 0x2000, 1),
            STATUS_OK
        );
        assert_eq!(
            request(&mut dev, &mut mem, CMD_READ, 1, 0x3000, 1),
            STATUS_OK
        );
        assert_eq!(
            &mem.0[0x3000..0x3000 + SECTOR_SIZE],
            &content[SECTOR_SIZE..2 * SECTOR_SIZE],
            "sector 1 still serves base after sector-0 write (RMW)"
        );
        std::fs::remove_file(&path).unwrap();
    }
}
