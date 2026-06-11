//! pv-entropy (ARCH §6.3 + §5, window base 0xD000_2000): the guest's ONLY
//! entropy source, served from a per-segment seeded ChaCha20 stream. The
//! hypervisor never reads host entropy on this path.
//!
//! Register map (window-relative):
//!   0x08 BUF_GPA  RW 8B   guest-RAM destination
//!   0x10 LEN      RW 4B   bytes requested
//!   0x14 DOORBELL WO 4B   write triggers the synchronous fill
//!   0x18 STATUS   RO 4B   see STATUS_* (data is in place when the doorbell
//!                         MMIO write instruction retires)
//!
//! On doorbell: fill LEN bytes at BUF_GPA from the PRNG, set STATUS=OK, and
//! append AUX ENTROPY {icount, len, digest8}. A bad BUF_GPA/LEN sets
//! STATUS=FAULT and serves nothing — deterministically (the same guest
//! state faults identically on replay). The PRNG has still advanced on the
//! fault path (bytes were drawn before the write failed); that too is
//! deterministic and therefore harmless.

use crate::ctx::DevCtx;
use crate::{DetDevice, EntropySource, RestoreError};
use dh_inputlog::dhilog::LogWriter;
use rand_chacha::rand_core::{RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;

pub const PV_ENTROPY_BASE: u64 = 0xD000_2000;
pub const DEVICE_ID_PV_ENTROPY: u16 = 0x0004;

pub const REG_BUF_GPA: u64 = 0x08;
pub const REG_LEN: u64 = 0x10;
pub const REG_DOORBELL: u64 = 0x14;
pub const REG_STATUS: u64 = 0x18;

pub const STATUS_IDLE: u32 = 0;
pub const STATUS_OK: u32 = 1;
pub const STATUS_FAULT: u32 = 2;

/// Single fill cap — bounds the HOST allocation per doorbell (served bytes
/// never enter the DHILOG; only the digest does). Larger LEN faults
/// deterministically; raise it if a legitimate guest ever needs more per
/// draw.
pub const MAX_FILL: u32 = 1 << 20;

const SECTION_VERSION: u16 = 1;
/// Device registers only: buf_gpa u64 || len u32 || status u32 (16 bytes).
/// The PRNG state is NOT here — it is the VMM-owned ENTR section (§5).
const SECTION_LEN: usize = 16;

/// The per-segment deterministic PRNG (ARCH §5). Owned by the slot (the
/// VMM), passed to devices as `&mut dyn EntropySource`. State is exactly
/// {seed[32], stream u64, word_pos u128} — exportable for the ENTR section
/// so a fork resumes the stream exactly.
pub struct DetEntropy {
    rng: ChaCha20Rng,
}

/// The 56-byte ENTR state tuple (ARCH §5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntropyState {
    pub seed: [u8; 32],
    pub stream: u64,
    pub word_pos: u128,
}

impl DetEntropy {
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self {
            rng: ChaCha20Rng::from_seed(seed),
        }
    }

    pub fn state(&self) -> EntropyState {
        EntropyState {
            seed: self.rng.get_seed(),
            stream: self.rng.get_stream(),
            word_pos: self.rng.get_word_pos(),
        }
    }

    pub fn restore(state: EntropyState) -> Self {
        let mut rng = ChaCha20Rng::from_seed(state.seed);
        rng.set_stream(state.stream);
        rng.set_word_pos(state.word_pos);
        Self { rng }
    }
}

impl EntropySource for DetEntropy {
    /// WORD-GRANULARITY INVARIANT (rand_chacha 0.3): the stream position is
    /// granular to 32-bit words. A fill that is not a multiple of 4 bytes
    /// consumes whole words and DISCARDS the sub-word remainder — a
    /// 37-byte fill advances word_pos by 10 (40 bytes of stream).
    /// Deterministic (both replays discard identically) and state() always
    /// captures a word-aligned position, so ENTR resume is exact.
    fn fill(&mut self, buf: &mut [u8]) {
        self.rng.fill_bytes(buf);
    }
}

/// The pv-entropy device: registers + doorbell behavior. Draws bytes from
/// `ctx.entropy` (the slot's DetEntropy), never from anything else.
pub struct PvEntropy {
    buf_gpa: u64,
    len: u32,
    status: u32,
}

impl Default for PvEntropy {
    fn default() -> Self {
        Self::new()
    }
}

impl PvEntropy {
    pub fn new() -> Self {
        Self {
            buf_gpa: 0,
            len: 0,
            status: STATUS_IDLE,
        }
    }

    fn doorbell(&mut self, ctx: &mut DevCtx) {
        if self.len > MAX_FILL {
            self.status = STATUS_FAULT;
            return;
        }
        let mut bytes = vec![0u8; self.len as usize];
        ctx.entropy.fill(&mut bytes);
        if ctx.mem.write(self.buf_gpa, &bytes).is_err() {
            self.status = STATUS_FAULT;
            return;
        }
        let digest8 = LogWriter::digest8(&bytes);
        ctx.log_entropy(self.len, digest8);
        self.status = STATUS_OK;
    }
}

impl DetDevice for PvEntropy {
    fn device_id(&self) -> u16 {
        DEVICE_ID_PV_ENTROPY
    }

    fn section_version(&self) -> u16 {
        SECTION_VERSION
    }

    fn mmio_read(&mut self, off: u64, data: &mut [u8], _ctx: &mut DevCtx) {
        match (off, data.len()) {
            (REG_BUF_GPA, 8) => data.copy_from_slice(&self.buf_gpa.to_le_bytes()),
            (REG_LEN, 4) => data.copy_from_slice(&self.len.to_le_bytes()),
            (REG_STATUS, 4) => data.copy_from_slice(&self.status.to_le_bytes()),
            // DOORBELL is write-only; everything else unknown.
            _ => data.fill(0),
        }
    }

    fn mmio_write(&mut self, off: u64, data: &[u8], ctx: &mut DevCtx) {
        match (off, data.len()) {
            (REG_BUF_GPA, 8) => self.buf_gpa = u64::from_le_bytes(data.try_into().unwrap()),
            (REG_LEN, 4) => self.len = u32::from_le_bytes(data.try_into().unwrap()),
            (REG_DOORBELL, 4) => self.doorbell(ctx),
            // STATUS is RO; unknown offsets ignored.
            _ => {}
        }
    }

    fn snapshot(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.buf_gpa.to_le_bytes());
        out.extend_from_slice(&self.len.to_le_bytes());
        out.extend_from_slice(&self.status.to_le_bytes());
    }

    fn restore(&mut self, bytes: &[u8], sec_version: u16) -> Result<(), RestoreError> {
        if sec_version != SECTION_VERSION || bytes.len() != SECTION_LEN {
            return Err(RestoreError);
        }
        self.buf_gpa = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
        self.len = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        self.status = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::{DevCtx, IrqRequest, VecGuestMem};
    use crate::MmioBus;
    use dh_inputlog::dhilog::{SealParams, SegmentHeader, KIND_ENTROPY};

    fn log() -> LogWriter {
        LogWriter::new(SegmentHeader {
            base_snapshot_id: [0; 32],
            entropy_seed: [0x42; 32],
            machine_config_hash: [0; 32],
            clock_num: 1,
            clock_den: 1,
            encoder_fingerprint: 0,
        })
    }

    const B: u64 = PV_ENTROPY_BASE;

    fn bus() -> MmioBus {
        let mut bus = MmioBus::new();
        bus.register(B, Box::new(PvEntropy::new())).unwrap();
        bus
    }

    /// Drive: set BUF_GPA/LEN, ring doorbell, return (status, mem, log).
    fn serve(
        bus: &mut MmioBus,
        ent: &mut DetEntropy,
        gpa: u64,
        len: u32,
        icount: u64,
    ) -> (u32, Vec<u8>, LogWriter) {
        let mut l = log();
        let mut m = VecGuestMem(vec![0u8; 64]);
        let mut q: Vec<IrqRequest> = Vec::new();
        let mut ctx = DevCtx::new(icount, 0x9999, &mut l, &mut m, ent, &mut q);
        bus.write(B + 0x08, &gpa.to_le_bytes(), &mut ctx).unwrap();
        bus.write(B + 0x10, &len.to_le_bytes(), &mut ctx).unwrap();
        bus.write(B + 0x14, &1u32.to_le_bytes(), &mut ctx).unwrap();
        let mut status = [0u8; 4];
        bus.read(B + 0x18, &mut status, &mut ctx).unwrap();
        assert_eq!(ctx.log_fault(), None);
        (u32::from_le_bytes(status), m.0, l)
    }

    #[test]
    fn doorbell_fills_deterministically_and_logs() {
        let mut bus1 = bus();
        let mut bus2 = bus();
        let mut e1 = DetEntropy::from_seed([7; 32]);
        let mut e2 = DetEntropy::from_seed([7; 32]);
        let (s1, m1, l1) = serve(&mut bus1, &mut e1, 16, 24, 100);
        let (s2, m2, l2) = serve(&mut bus2, &mut e2, 16, 24, 100);
        assert_eq!(s1, STATUS_OK);
        assert_eq!(m1, m2, "same seed ⇒ identical bytes");
        assert_eq!(s1, s2);
        assert_ne!(&m1[16..40], &[0u8; 24][..], "bytes actually written");

        // ENTROPY AUX record: kind, icount, len, digest8 of served bytes.
        let bytes = l1
            .seal(SealParams {
                end_snapshot_id: [0; 32],
                end_icount: 200,
                end_vns: 200,
                end_state_hash: [0; 32],
                stop_reason: 1,
            })
            .unwrap();
        let r = &bytes[256..];
        assert_eq!(r[0], KIND_ENTROPY);
        assert_eq!(u64::from_le_bytes(r[8..16].try_into().unwrap()), 100);
        let p = &r[24..];
        assert_eq!(u32::from_le_bytes(p[0..4].try_into().unwrap()), 24);
        assert_eq!(
            u64::from_le_bytes(p[8..16].try_into().unwrap()),
            LogWriter::digest8(&m1[16..40])
        );
        drop(l2);
    }

    #[test]
    fn bad_gpa_faults_without_serving() {
        let mut bus = bus();
        let mut e = DetEntropy::from_seed([1; 32]);
        let (status, mem, _l) = serve(&mut bus, &mut e, 60, 16, 5); // 60+16 > 64
        assert_eq!(status, STATUS_FAULT);
        assert_eq!(mem, vec![0u8; 64], "nothing written on fault");
    }

    #[test]
    fn len_zero_doorbell_is_ok_and_logs_len_zero() {
        let mut bus = bus();
        let mut e = DetEntropy::from_seed([5; 32]);
        let before = e.state();
        let (status, mem, _l) = serve(&mut bus, &mut e, 0, 0, 5);
        assert_eq!(status, STATUS_OK);
        assert_eq!(mem, vec![0u8; 64]);
        assert_eq!(e.state(), before, "zero-length fill must not advance");
    }

    #[test]
    fn oversized_len_faults() {
        let mut bus = bus();
        let mut e = DetEntropy::from_seed([1; 32]);
        let (status, _, _) = serve(&mut bus, &mut e, 0, MAX_FILL + 1, 5);
        assert_eq!(status, STATUS_FAULT);
    }

    #[test]
    fn state_roundtrip_resumes_stream_exactly() {
        // The M4 ENTR golden property: restore reproduces the next N draws
        // bit-identically. Note the position is ALWAYS word-aligned: a
        // 37-byte fill consumes 10 whole words (40 bytes), discarding the
        // sub-word remainder — pinned below.
        let mut a = DetEntropy::from_seed([9; 32]);
        let mut burn = [0u8; 37];
        a.fill(&mut burn);
        let state = a.state();
        assert_eq!(state.seed, [9; 32]);
        assert_eq!(state.word_pos, 10, "37-byte fill quantizes to 10 words");
        // Same position as a 40-byte fill from the same seed: the
        // remainder of the 10th word was discarded, not buffered.
        let mut c = DetEntropy::from_seed([9; 32]);
        c.fill(&mut [0u8; 40]);
        assert_eq!(state, c.state());

        let mut b = DetEntropy::restore(state);
        let mut next_a = [0u8; 64];
        let mut next_b = [0u8; 64];
        a.fill(&mut next_a);
        b.fill(&mut next_b);
        assert_eq!(next_a, next_b);
        assert_eq!(a.state(), b.state());
    }

    #[test]
    fn device_registers_snapshot_roundtrip() {
        let mut a = PvEntropy::new();
        a.buf_gpa = 0x1234;
        a.len = 99;
        a.status = STATUS_OK;
        let mut section = Vec::new();
        a.snapshot(&mut section);
        assert_eq!(section.len(), SECTION_LEN);
        let mut b = PvEntropy::new();
        b.restore(&section, SECTION_VERSION).unwrap();
        assert_eq!((b.buf_gpa, b.len, b.status), (0x1234, 99, STATUS_OK));
        assert_eq!(b.restore(&section, 2), Err(RestoreError));
    }

    #[test]
    fn doorbell_is_write_only_status_is_read_only() {
        let mut bus = bus();
        let mut e = DetEntropy::from_seed([3; 32]);
        let mut l = log();
        let mut m = VecGuestMem(vec![0u8; 16]);
        let mut q: Vec<IrqRequest> = Vec::new();
        let mut ctx = DevCtx::new(1, 0, &mut l, &mut m, &mut e, &mut q);
        let mut v = [0u8; 4];
        bus.read(B + 0x14, &mut v, &mut ctx).unwrap();
        assert_eq!(u32::from_le_bytes(v), 0); // doorbell reads as zero
        bus.write(B + 0x18, &7u32.to_le_bytes(), &mut ctx).unwrap();
        bus.read(B + 0x18, &mut v, &mut ctx).unwrap();
        assert_eq!(u32::from_le_bytes(v), STATUS_IDLE); // write ignored
    }
}

#[cfg(test)]
mod kat_tests {
    use super::*;
    use crate::EntropySource;

    /// Known-answer pins for the ChaCha20 stream (review iteration 71):
    /// rand_chacha is caret-ranged ("0.3"), and ENTR resume depends on its
    /// exact stream semantics — a dep upgrade that changes the bytes or
    /// the stream/word_pos interpretation must fail HERE, loudly, not as
    /// a silent cross-version replay divergence.
    #[test]
    fn chacha_stream_known_answers_are_pinned() {
        let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();

        let mut e = DetEntropy::from_seed([0x42; 32]);
        let mut buf = [0u8; 32];
        e.fill(&mut buf);
        assert_eq!(
            hex(&buf),
            "a4ddf31f7f32ba696f14ce50ecf3f21e3e100e83bdf47966e7b07468e9500b6e"
        );

        // Nonzero stream + word_pos (the restore path's parameter space).
        let mut e2 = DetEntropy::restore(EntropyState {
            seed: [0x42; 32],
            stream: 7,
            word_pos: 100,
        });
        let mut buf2 = [0u8; 32];
        e2.fill(&mut buf2);
        assert_eq!(
            hex(&buf2),
            "65293f16d9340772bc58bcfabb3ef8331b3dffec0ea4cd3d01af0346bc70ff0c"
        );
    }
}
