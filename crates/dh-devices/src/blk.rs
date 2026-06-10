//! pv-blk (ARCH §6.5): read-only base image + CoW overlay.
//!
//! Simplified virtio-blk-like device: a ONE-DEEP request register set (no
//! rings — single vCPU), completion synchronous inside the MMIO-write
//! emulation, so there is zero timing variance. The guest writes SECTOR /
//! BUF_GPA / COUNT, then CMD; STATUS is valid when the CMD write's VM exit
//! returns.
//!
//! Backend split (deny-list rule, lib.rs): this crate never does host I/O,
//! so the device reads the base image through the [`BlockBase`] seam. The
//! production `O_RDONLY` file implementation lives in dh-vmm (its content
//! hash is recorded in MachineConfig); tests use an in-memory base. The
//! base is IMMUTABLE — all writes land in the overlay: 64 KiB clusters
//! populated on first write by read-modify-write from the base.
//!
//! Determinism notes: reads/writes are pure functions of (base content,
//! overlay state, guest RAM, request registers) — no DHILOG records are
//! produced here (nothing is an input). The overlay snapshot serializes
//! dirty clusters SORTED by index so identical states yield identical
//! bytes ("pure function of device state"; full incremental codec is M4).

use crate::ctx::DevCtx;
use crate::{DetDevice, RestoreError};
use std::collections::HashMap;

pub const DEVICE_ID_PV_BLK: u16 = 0x0005;

pub const SECTOR_SIZE: usize = 512;
pub const CLUSTER_SIZE: usize = 64 * 1024;
pub const SECTORS_PER_CLUSTER: u64 = (CLUSTER_SIZE / SECTOR_SIZE) as u64;

pub const REG_SECTOR: u64 = 0x08; // 8B RW
pub const REG_BUF_GPA: u64 = 0x10; // 8B RW
pub const REG_COUNT: u64 = 0x18; // 4B RW (sectors)
pub const REG_CMD: u64 = 0x1C; // 4B WO: 1=read 2=write 3=flush
pub const REG_STATUS: u64 = 0x20; // 4B RO

pub const CMD_READ: u32 = 1;
pub const CMD_WRITE: u32 = 2;
pub const CMD_FLUSH: u32 = 3;

/// STATUS values (this repo's ABI; ARCH §6.5 leaves codes to the model).
///
/// PARTIAL-COMPLETION CONTRACT: on a nonzero status the request may have
/// partially executed — earlier chunks of the guest buffer (reads) or
/// overlay clusters (writes) are already mutated. That partial state is a
/// pure function of (registers, base, overlay, guest RAM), so record and
/// replay mutate identically; STATUS is the only completion signal and
/// the guest must treat the buffer as undefined on error.
pub const STATUS_OK: u32 = 0;
/// Request outside the device (sector range, zero/overflowing count) or an
/// unknown CMD value.
pub const STATUS_BAD_REQUEST: u32 = 1;
/// Guest buffer access faulted (BUF_GPA range not mapped guest RAM).
pub const STATUS_MEM_FAULT: u32 = 2;
/// The base backend failed a read. The base is immutable and was readable
/// at startup, so this is a HOST fault (dying disk, truncated image) —
/// guest-visible only to keep the device total; run control must treat a
/// nonzero `DetChannelMetrics`-style check of [`PvBlk::host_io_errors`]
/// after the exit as slot-fatal, since a host I/O error cannot be
/// guaranteed to reproduce on replay.
pub const STATUS_HOST_IO: u32 = 0xFE;

const SECTION_VERSION: u16 = 1;
/// Fixed-width section prefix: sector u64 | buf_gpa u64 | count u32 |
/// status u32 | host_io_errors u64 | dirty_cluster_count u32 (then
/// per-cluster entries).
const SECTION_FIXED: usize = 8 + 8 + 4 + 4 + 8 + 4;
/// Per dirty cluster: idx u64 | blake3 [u8;32] | bytes [u8; CLUSTER_SIZE].
const SECTION_PER_CLUSTER: usize = 8 + 32 + CLUSTER_SIZE;

/// Read-only access to the base image. Implementations MUST be immutable
/// and deterministic: same offset → same bytes, forever (the content hash
/// is recorded in MachineConfig). Reads past `len_bytes` zero-fill.
pub trait BlockBase {
    /// Base image length in bytes (capacity = `len_bytes / 512` sectors;
    /// a trailing partial sector is not addressable).
    fn len_bytes(&self) -> u64;
    /// Fill `buf` from `offset`, zero-filling any part past `len_bytes`.
    /// `Err` means the host could not read its own immutable image — a
    /// host fault, surfaced as [`STATUS_HOST_IO`].
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), BaseIoError>;
}

/// Host-side base read failure (see [`STATUS_HOST_IO`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BaseIoError;

pub struct PvBlk {
    base: Box<dyn BlockBase>,
    overlay: HashMap<u64, Box<[u8; CLUSTER_SIZE]>>,
    sector: u64,
    buf_gpa: u64,
    count: u32,
    status: u32,
    /// Count of STATUS_HOST_IO completions — run control checks this after
    /// dispatch and faults the slot (host I/O errors do not replay).
    pub host_io_errors: u64,
}

impl PvBlk {
    pub fn new(base: Box<dyn BlockBase>) -> Self {
        Self {
            base,
            overlay: HashMap::new(),
            sector: 0,
            buf_gpa: 0,
            count: 0,
            status: STATUS_OK,
            host_io_errors: 0,
        }
    }

    /// Device capacity in sectors (whole sectors of the base image).
    pub fn capacity_sectors(&self) -> u64 {
        self.base.len_bytes() / SECTOR_SIZE as u64
    }

    /// Dirty cluster count (overlay population; test/observability).
    pub fn dirty_clusters(&self) -> usize {
        self.overlay.len()
    }

    fn execute(&mut self, cmd: u32, ctx: &mut DevCtx) {
        self.status = match cmd {
            CMD_READ => self.do_read(ctx),
            CMD_WRITE => self.do_write(ctx),
            CMD_FLUSH => STATUS_OK, // overlay is host RAM; nothing to sync
            _ => STATUS_BAD_REQUEST,
        };
        if self.status == STATUS_HOST_IO {
            self.host_io_errors += 1;
        }
    }

    /// Validate the latched request; returns the byte range on success.
    fn request_range(&self) -> Result<(u64, usize), u32> {
        if self.count == 0 {
            return Err(STATUS_BAD_REQUEST);
        }
        let end_sector = self
            .sector
            .checked_add(u64::from(self.count))
            .ok_or(STATUS_BAD_REQUEST)?;
        if end_sector > self.capacity_sectors() {
            return Err(STATUS_BAD_REQUEST);
        }
        Ok((
            self.sector * SECTOR_SIZE as u64,
            self.count as usize * SECTOR_SIZE,
        ))
    }

    fn do_read(&mut self, ctx: &mut DevCtx) -> u32 {
        let (mut off, mut remaining) = match self.request_range() {
            Ok(r) => r,
            Err(s) => return s,
        };
        let mut gpa = self.buf_gpa;
        while remaining > 0 {
            let cluster = off / CLUSTER_SIZE as u64;
            let within = (off % CLUSTER_SIZE as u64) as usize;
            let take = remaining.min(CLUSTER_SIZE - within);
            let mut chunk = vec![0u8; take];
            if let Some(c) = self.overlay.get(&cluster) {
                chunk.copy_from_slice(&c[within..within + take]);
            } else if self.base.read_at(off, &mut chunk).is_err() {
                return STATUS_HOST_IO;
            }
            if ctx.mem.write(gpa, &chunk).is_err() {
                return STATUS_MEM_FAULT;
            }
            off += take as u64;
            gpa += take as u64;
            remaining -= take;
        }
        STATUS_OK
    }

    fn do_write(&mut self, ctx: &mut DevCtx) -> u32 {
        let (mut off, mut remaining) = match self.request_range() {
            Ok(r) => r,
            Err(s) => return s,
        };
        let mut gpa = self.buf_gpa;
        while remaining > 0 {
            let cluster = off / CLUSTER_SIZE as u64;
            let within = (off % CLUSTER_SIZE as u64) as usize;
            let take = remaining.min(CLUSTER_SIZE - within);
            // Populate-on-first-write: RMW the whole cluster from base.
            if !self.overlay.contains_key(&cluster) {
                let mut fresh = Box::new([0u8; CLUSTER_SIZE]);
                if self
                    .base
                    .read_at(cluster * CLUSTER_SIZE as u64, fresh.as_mut_slice())
                    .is_err()
                {
                    return STATUS_HOST_IO;
                }
                self.overlay.insert(cluster, fresh);
            }
            let c = self.overlay.get_mut(&cluster).expect("just inserted");
            if ctx.mem.read(gpa, &mut c[within..within + take]).is_err() {
                // The cluster stays populated (RMW already happened) — a
                // deterministic function of the same failing request.
                return STATUS_MEM_FAULT;
            }
            off += take as u64;
            gpa += take as u64;
            remaining -= take;
        }
        STATUS_OK
    }
}

impl DetDevice for PvBlk {
    fn device_id(&self) -> u16 {
        DEVICE_ID_PV_BLK
    }

    fn section_version(&self) -> u16 {
        SECTION_VERSION
    }

    fn mmio_read(&mut self, off: u64, data: &mut [u8], _ctx: &mut DevCtx) {
        match (off, data.len()) {
            (REG_SECTOR, 8) => data.copy_from_slice(&self.sector.to_le_bytes()),
            (REG_BUF_GPA, 8) => data.copy_from_slice(&self.buf_gpa.to_le_bytes()),
            (REG_COUNT, 4) => data.copy_from_slice(&self.count.to_le_bytes()),
            (REG_STATUS, 4) => data.copy_from_slice(&self.status.to_le_bytes()),
            // CMD is write-only; everything else (incl. size-mismatched
            // access to known registers) reads as zeros — never host state.
            _ => data.fill(0),
        }
    }

    fn mmio_write(&mut self, off: u64, data: &[u8], ctx: &mut DevCtx) {
        match (off, data.len()) {
            (REG_SECTOR, 8) => self.sector = u64::from_le_bytes(data.try_into().unwrap()),
            (REG_BUF_GPA, 8) => self.buf_gpa = u64::from_le_bytes(data.try_into().unwrap()),
            (REG_COUNT, 4) => self.count = u32::from_le_bytes(data.try_into().unwrap()),
            (REG_CMD, 4) => {
                let cmd = u32::from_le_bytes(data.try_into().unwrap());
                self.execute(cmd, ctx);
            }
            // STATUS is read-only; unknown offsets / sizes ignored.
            _ => {}
        }
    }

    fn snapshot(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.sector.to_le_bytes());
        out.extend_from_slice(&self.buf_gpa.to_le_bytes());
        out.extend_from_slice(&self.count.to_le_bytes());
        out.extend_from_slice(&self.status.to_le_bytes());
        // host_io_errors travels with status: a restored STATUS_HOST_IO
        // must keep its nonzero counter or the run-control check breaks.
        out.extend_from_slice(&self.host_io_errors.to_le_bytes());
        // Sorted: HashMap order must never leak into snapshot bytes.
        let mut idxs: Vec<u64> = self.overlay.keys().copied().collect();
        idxs.sort_unstable();
        out.extend_from_slice(&(idxs.len() as u32).to_le_bytes());
        for idx in idxs {
            let c = &self.overlay[&idx];
            out.extend_from_slice(&idx.to_le_bytes());
            out.extend_from_slice(&dh_inputlog::payload_digest(c.as_slice()));
            out.extend_from_slice(c.as_slice());
        }
    }

    fn restore(&mut self, bytes: &[u8], sec_version: u16) -> Result<(), RestoreError> {
        if sec_version != SECTION_VERSION || bytes.len() < SECTION_FIXED {
            return Err(RestoreError);
        }
        let sector = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
        let buf_gpa = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
        let count = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
        let status = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
        let host_io_errors = u64::from_le_bytes(bytes[24..32].try_into().unwrap());
        let n = u32::from_le_bytes(bytes[32..36].try_into().unwrap()) as usize;
        // Length equality BEFORE any allocation: a hostile n cannot
        // pre-allocate (n is bounded by the actual byte length here).
        if bytes.len() != SECTION_FIXED + n * SECTION_PER_CLUSTER {
            return Err(RestoreError);
        }
        let mut overlay = HashMap::with_capacity(n);
        for i in 0..n {
            let at = SECTION_FIXED + i * SECTION_PER_CLUSTER;
            let idx = u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
            let digest: [u8; 32] = bytes[at + 8..at + 40].try_into().unwrap();
            let data = &bytes[at + 40..at + 40 + CLUSTER_SIZE];
            if dh_inputlog::payload_digest(data) != digest {
                return Err(RestoreError); // bit rot / truncated section
            }
            let mut c = Box::new([0u8; CLUSTER_SIZE]);
            c.copy_from_slice(data);
            overlay.insert(idx, c);
        }
        self.sector = sector;
        self.buf_gpa = buf_gpa;
        self.count = count;
        self.status = status;
        self.host_io_errors = host_io_errors;
        self.overlay = overlay;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::MmioBus;
    use crate::ctx::test_support::FakeEntropy;
    use crate::ctx::VecGuestMem;
    use dh_inputlog::dhilog::{LogWriter, SegmentHeader};
    use std::rc::Rc;

    /// Immutable shared base: the Rc proves the device cannot mutate it.
    #[derive(Clone)]
    struct VecBase(Rc<Vec<u8>>);

    impl BlockBase for VecBase {
        fn len_bytes(&self) -> u64 {
            self.0.len() as u64
        }
        fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), BaseIoError> {
            buf.fill(0);
            let off = offset as usize;
            if off < self.0.len() {
                let take = buf.len().min(self.0.len() - off);
                buf[..take].copy_from_slice(&self.0[off..off + take]);
            }
            Ok(())
        }
    }

    /// A base whose every read fails (host disk death).
    struct DeadBase;
    impl BlockBase for DeadBase {
        fn len_bytes(&self) -> u64 {
            CLUSTER_SIZE as u64 * 4
        }
        fn read_at(&self, _offset: u64, _buf: &mut [u8]) -> Result<(), BaseIoError> {
            Err(BaseIoError)
        }
    }

    fn log() -> LogWriter {
        LogWriter::new(SegmentHeader {
            base_snapshot_id: [0; 32],
            entropy_seed: [0; 32],
            machine_config_hash: [0; 32],
            clock_num: 1,
            clock_den: 1,
        })
    }

    /// Patterned base: 3 clusters, byte = (absolute_offset % 251) as u8.
    fn patterned_base() -> VecBase {
        VecBase(Rc::new(
            (0..3 * CLUSTER_SIZE).map(|i| (i % 251) as u8).collect(),
        ))
    }

    /// Drive one request through the register protocol via guest RAM.
    fn request(
        dev: &mut PvBlk,
        mem: &mut VecGuestMem,
        cmd: u32,
        sector: u64,
        buf_gpa: u64,
        count: u32,
    ) -> u32 {
        let mut l = log();
        let mut ent = FakeEntropy(0);
        let mut q = Vec::new();
        let mut ctx = DevCtx::new(1, 0, &mut l, mem, &mut ent, &mut q);
        dev.mmio_write(REG_SECTOR, &sector.to_le_bytes(), &mut ctx);
        dev.mmio_write(REG_BUF_GPA, &buf_gpa.to_le_bytes(), &mut ctx);
        dev.mmio_write(REG_COUNT, &count.to_le_bytes(), &mut ctx);
        dev.mmio_write(REG_CMD, &cmd.to_le_bytes(), &mut ctx);
        let mut status = [0u8; 4];
        dev.mmio_read(REG_STATUS, &mut status, &mut ctx);
        u32::from_le_bytes(status)
    }

    #[test]
    fn reads_come_from_base_until_overwritten() {
        let base = patterned_base();
        let expect: Vec<u8> = base.0[..SECTOR_SIZE].to_vec();
        let mut dev = PvBlk::new(Box::new(base.clone()));
        let mut mem = VecGuestMem(vec![0u8; 0x4000]);

        assert_eq!(
            request(&mut dev, &mut mem, CMD_READ, 0, 0x1000, 1),
            STATUS_OK
        );
        assert_eq!(&mem.0[0x1000..0x1000 + SECTOR_SIZE], &expect[..]);
        assert_eq!(dev.dirty_clusters(), 0, "reads must not populate overlay");

        // Overwrite sector 0 with 0xAA from guest RAM, read it back.
        mem.0[0x2000..0x2000 + SECTOR_SIZE].fill(0xAA);
        assert_eq!(
            request(&mut dev, &mut mem, CMD_WRITE, 0, 0x2000, 1),
            STATUS_OK
        );
        assert_eq!(dev.dirty_clusters(), 1);
        assert_eq!(
            request(&mut dev, &mut mem, CMD_READ, 0, 0x3000, 1),
            STATUS_OK
        );
        assert!(mem.0[0x3000..0x3000 + SECTOR_SIZE]
            .iter()
            .all(|&b| b == 0xAA));

        // The base itself is bit-identical (CoW, never written).
        assert_eq!(&base.0[..SECTOR_SIZE], &expect[..]);
    }

    #[test]
    fn rmw_preserves_cluster_neighbors() {
        let base = patterned_base();
        let neighbor: Vec<u8> = base.0[SECTOR_SIZE..2 * SECTOR_SIZE].to_vec();
        let mut dev = PvBlk::new(Box::new(base));
        let mut mem = VecGuestMem(vec![0u8; 0x4000]);

        // Write sector 0; sector 1 (same cluster) must still read base
        // content — proving populate-on-first-write RMW'd from base.
        mem.0[0x1000..0x1000 + SECTOR_SIZE].fill(0xBB);
        assert_eq!(
            request(&mut dev, &mut mem, CMD_WRITE, 0, 0x1000, 1),
            STATUS_OK
        );
        assert_eq!(
            request(&mut dev, &mut mem, CMD_READ, 1, 0x2000, 1),
            STATUS_OK
        );
        assert_eq!(&mem.0[0x2000..0x2000 + SECTOR_SIZE], &neighbor[..]);
    }

    #[test]
    fn cross_cluster_requests_split_correctly() {
        let base = patterned_base();
        let mut dev = PvBlk::new(Box::new(base.clone()));
        let mut mem = VecGuestMem(vec![0u8; 0x10000]);

        // Write 4 sectors straddling the cluster 0/1 boundary.
        let start = SECTORS_PER_CLUSTER - 2;
        mem.0[0x1000..0x1000 + 4 * SECTOR_SIZE].fill(0xCC);
        assert_eq!(
            request(&mut dev, &mut mem, CMD_WRITE, start, 0x1000, 4),
            STATUS_OK
        );
        assert_eq!(dev.dirty_clusters(), 2, "both clusters populate");

        // Read the same span back plus one base sector on each side.
        assert_eq!(
            request(&mut dev, &mut mem, CMD_READ, start - 1, 0x4000, 6),
            STATUS_OK
        );
        let span = &mem.0[0x4000..0x4000 + 6 * SECTOR_SIZE];
        let base_off = (start - 1) as usize * SECTOR_SIZE;
        assert_eq!(
            &span[..SECTOR_SIZE],
            &base.0[base_off..base_off + SECTOR_SIZE]
        );
        assert!(span[SECTOR_SIZE..5 * SECTOR_SIZE]
            .iter()
            .all(|&b| b == 0xCC));
        let tail_off = base_off + 5 * SECTOR_SIZE;
        assert_eq!(
            &span[5 * SECTOR_SIZE..],
            &base.0[tail_off..tail_off + SECTOR_SIZE]
        );
    }

    #[test]
    fn bad_requests_and_unknown_cmds_set_status() {
        let mut dev = PvBlk::new(Box::new(patterned_base()));
        let mut mem = VecGuestMem(vec![0u8; 0x1000]);
        let cap = dev.capacity_sectors();

        assert_eq!(
            request(&mut dev, &mut mem, CMD_READ, 0, 0, 0),
            STATUS_BAD_REQUEST
        );
        assert_eq!(
            request(&mut dev, &mut mem, CMD_READ, cap, 0, 1),
            STATUS_BAD_REQUEST
        );
        assert_eq!(
            request(&mut dev, &mut mem, CMD_READ, u64::MAX, 0, 2),
            STATUS_BAD_REQUEST
        );
        assert_eq!(
            request(&mut dev, &mut mem, 0xBEEF, 0, 0, 1),
            STATUS_BAD_REQUEST
        );
        assert_eq!(request(&mut dev, &mut mem, CMD_FLUSH, 0, 0, 0), STATUS_OK);
    }

    #[test]
    fn guest_buffer_faults_set_mem_fault() {
        let mut dev = PvBlk::new(Box::new(patterned_base()));
        let mut mem = VecGuestMem(vec![0u8; 0x1000]);
        // Buffer outside guest RAM.
        assert_eq!(
            request(&mut dev, &mut mem, CMD_READ, 0, 0x10_0000, 1),
            STATUS_MEM_FAULT
        );
        assert_eq!(
            request(&mut dev, &mut mem, CMD_WRITE, 0, 0x10_0000, 1),
            STATUS_MEM_FAULT
        );
    }

    #[test]
    fn dead_base_surfaces_host_io_and_counts() {
        let mut dev = PvBlk::new(Box::new(DeadBase));
        let mut mem = VecGuestMem(vec![0u8; 0x1000]);
        assert_eq!(
            request(&mut dev, &mut mem, CMD_READ, 0, 0, 1),
            STATUS_HOST_IO
        );
        assert_eq!(
            request(&mut dev, &mut mem, CMD_WRITE, 0, 0, 1),
            STATUS_HOST_IO
        );
        assert_eq!(dev.host_io_errors, 2);
    }

    #[test]
    fn snapshot_is_sorted_deterministic_and_roundtrips() {
        let base = patterned_base();
        let mut mem = VecGuestMem(vec![0u8; 0x4000]);
        mem.0[0x1000..0x1000 + SECTOR_SIZE].fill(0xDD);

        // Two devices dirty the same clusters in OPPOSITE order.
        let mut a = PvBlk::new(Box::new(base.clone()));
        request(&mut a, &mut mem, CMD_WRITE, 0, 0x1000, 1);
        request(
            &mut a,
            &mut mem,
            CMD_WRITE,
            2 * SECTORS_PER_CLUSTER,
            0x1000,
            1,
        );
        let mut b = PvBlk::new(Box::new(base.clone()));
        request(
            &mut b,
            &mut mem,
            CMD_WRITE,
            2 * SECTORS_PER_CLUSTER,
            0x1000,
            1,
        );
        request(&mut b, &mut mem, CMD_WRITE, 0, 0x1000, 1);

        let (mut sa, mut sb) = (Vec::new(), Vec::new());
        a.snapshot(&mut sa);
        b.snapshot(&mut sb);
        // Same dirty state — register latches differ (sector), so compare
        // the overlay part only after normalizing the prefix.
        assert_eq!(
            sa[24..],
            sb[24..],
            "overlay serialization must be order-free"
        );

        // Roundtrip: a fresh device over the same base restores and serves
        // the overlay content.
        let mut c = PvBlk::new(Box::new(base));
        c.restore(&sa, 1).unwrap();
        assert_eq!(c.dirty_clusters(), 2);
        assert_eq!(request(&mut c, &mut mem, CMD_READ, 0, 0x2000, 1), STATUS_OK);
        assert!(mem.0[0x2000..0x2000 + SECTOR_SIZE]
            .iter()
            .all(|&b| b == 0xDD));

        // Tampered cluster bytes fail the digest check.
        let mut bad = sa.clone();
        let last = bad.len() - 1;
        bad[last] ^= 0xFF;
        let mut d = PvBlk::new(Box::new(patterned_base()));
        assert_eq!(d.restore(&bad, 1), Err(RestoreError));
        // Wrong version / truncated input also refuse.
        assert_eq!(d.restore(&sa, 2), Err(RestoreError));
        assert_eq!(d.restore(&sa[..10], 1), Err(RestoreError));
    }

    #[test]
    fn registers_echo_and_status_is_read_only() {
        let mut dev = PvBlk::new(Box::new(patterned_base()));
        let mut mem = VecGuestMem(vec![0u8; 0x1000]);
        let mut l = log();
        let mut ent = FakeEntropy(0);
        let mut q = Vec::new();
        let mut ctx = DevCtx::new(1, 0, &mut l, &mut mem, &mut ent, &mut q);

        dev.mmio_write(REG_SECTOR, &0xAABB_CCDDu64.to_le_bytes(), &mut ctx);
        let mut v8 = [0u8; 8];
        dev.mmio_read(REG_SECTOR, &mut v8, &mut ctx);
        assert_eq!(u64::from_le_bytes(v8), 0xAABB_CCDD);

        // STATUS ignores writes; CMD reads as zeros (write-only).
        dev.mmio_write(REG_STATUS, &7u32.to_le_bytes(), &mut ctx);
        let mut v4 = [0u8; 4];
        dev.mmio_read(REG_STATUS, &mut v4, &mut ctx);
        assert_eq!(u32::from_le_bytes(v4), STATUS_OK);
        dev.mmio_read(REG_CMD, &mut v4, &mut ctx);
        assert_eq!(u32::from_le_bytes(v4), 0);

        // Size-mismatched access to a known register: zeros / ignored.
        let mut half = [0xFFu8; 4];
        dev.mmio_read(REG_SECTOR, &mut half, &mut ctx);
        assert_eq!(half, [0; 4]);
    }

    /// Guest memory that faults on any access at or past a cutoff GPA.
    struct FaultyMem {
        inner: VecGuestMem,
        fault_at: u64,
    }
    impl crate::ctx::GuestMem for FaultyMem {
        fn read(&self, gpa: u64, data: &mut [u8]) -> Result<(), crate::ctx::MemError> {
            if gpa + data.len() as u64 > self.fault_at {
                return Err(crate::ctx::MemError);
            }
            self.inner.read(gpa, data)
        }
        fn write(&mut self, gpa: u64, data: &[u8]) -> Result<(), crate::ctx::MemError> {
            if gpa + data.len() as u64 > self.fault_at {
                return Err(crate::ctx::MemError);
            }
            self.inner.write(gpa, data)
        }
    }

    fn request_over(
        dev: &mut PvBlk,
        mem: &mut dyn crate::ctx::GuestMem,
        cmd: u32,
        sector: u64,
        buf_gpa: u64,
        count: u32,
    ) -> u32 {
        let mut l = log();
        let mut ent = FakeEntropy(0);
        let mut q = Vec::new();
        let mut ctx = DevCtx::new(1, 0, &mut l, mem, &mut ent, &mut q);
        dev.mmio_write(REG_SECTOR, &sector.to_le_bytes(), &mut ctx);
        dev.mmio_write(REG_BUF_GPA, &buf_gpa.to_le_bytes(), &mut ctx);
        dev.mmio_write(REG_COUNT, &count.to_le_bytes(), &mut ctx);
        dev.mmio_write(REG_CMD, &cmd.to_le_bytes(), &mut ctx);
        let mut status = [0u8; 4];
        dev.mmio_read(REG_STATUS, &mut status, &mut ctx);
        u32::from_le_bytes(status)
    }

    #[test]
    fn mid_request_fault_leaves_identical_partial_state_twice() {
        // A cross-cluster read whose SECOND chunk faults in guest RAM: the
        // first chunk lands, then MEM_FAULT. Two identical runs must leave
        // bit-identical guest RAM and device state (the partial-completion
        // contract).
        let run = || {
            let base = patterned_base();
            let mut dev = PvBlk::new(Box::new(base));
            let mut mem = FaultyMem {
                inner: VecGuestMem(vec![0u8; 0x40000]),
                // Allow exactly the first cluster-bounded chunk (2 sectors
                // of the 4-sector read start in cluster 0), fault after.
                fault_at: 0x1000 + 2 * SECTOR_SIZE as u64,
            };
            let start = SECTORS_PER_CLUSTER - 2;
            let s = request_over(&mut dev, &mut mem, CMD_READ, start, 0x1000, 4);
            assert_eq!(s, STATUS_MEM_FAULT);
            let mut snap = Vec::new();
            dev.snapshot(&mut snap);
            (mem.inner.0, snap)
        };
        assert_eq!(run(), run());

        // Same for a cross-cluster WRITE: the first cluster populates and
        // mutates, the second chunk's guest read faults — deterministic
        // partial overlay, twice.
        let run_w = || {
            let base = patterned_base();
            let mut dev = PvBlk::new(Box::new(base));
            let mut mem = FaultyMem {
                inner: VecGuestMem(vec![0u8; 0x40000]),
                fault_at: 0x1000 + 2 * SECTOR_SIZE as u64,
            };
            let start = SECTORS_PER_CLUSTER - 2;
            let s = request_over(&mut dev, &mut mem, CMD_WRITE, start, 0x1000, 4);
            assert_eq!(s, STATUS_MEM_FAULT);
            assert_eq!(dev.dirty_clusters(), 2, "RMW populated both clusters");
            let mut snap = Vec::new();
            dev.snapshot(&mut snap);
            snap
        };
        assert_eq!(run_w(), run_w());
    }

    #[test]
    fn restore_then_snapshot_is_byte_identical_and_keeps_host_io_errors() {
        let base = patterned_base();
        let mut mem = VecGuestMem(vec![0u8; 0x4000]);
        let mut a = PvBlk::new(Box::new(base.clone()));
        mem.0[0x1000..0x1000 + SECTOR_SIZE].fill(0x42);
        request(&mut a, &mut mem, CMD_WRITE, 3, 0x1000, 1);
        a.host_io_errors = 7; // pretend an earlier host fault was recorded

        let mut snap_a = Vec::new();
        a.snapshot(&mut snap_a);

        let mut b = PvBlk::new(Box::new(base));
        b.restore(&snap_a, 1).unwrap();
        assert_eq!(b.host_io_errors, 7);
        let mut snap_b = Vec::new();
        b.snapshot(&mut snap_b);
        assert_eq!(snap_a, snap_b, "restore -> snapshot must be identity");
    }

    #[test]
    fn bus_serves_magic_and_version() {
        let mut bus = MmioBus::new();
        bus.register(
            0xD000_4000,
            Box::new(PvBlk::new(Box::new(patterned_base()))),
        )
        .unwrap();
        let mut mem = VecGuestMem(vec![0u8; 0x1000]);
        let mut l = log();
        let mut ent = FakeEntropy(0);
        let mut q = Vec::new();
        let mut ctx = DevCtx::new(1, 0, &mut l, &mut mem, &mut ent, &mut q);
        let mut v4 = [0u8; 4];
        bus.read(0xD000_4000, &mut v4, &mut ctx).unwrap();
        assert_eq!(u32::from_le_bytes(v4), u32::from(DEVICE_ID_PV_BLK));
        bus.read(0xD000_4004, &mut v4, &mut ctx).unwrap();
        assert_eq!(u32::from_le_bytes(v4), 1);
    }
}
