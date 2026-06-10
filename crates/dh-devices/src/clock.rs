//! pv-clock (ARCH §6.2, window base 0xD000_0000): the guest's ONLY time
//! source. Every read exits to the VMM and is computed from the live
//! counter at that exit — a pure function of icount, so no log record is
//! needed.
//!
//! Register map (window-relative):
//!   0x08 VNS_LO/HI      RO 8B   current virtual nanoseconds
//!   0x10 ICOUNT         RO 8B   current icount (guest-sdk diagnostics)
//!   0x18 TIMER_DEADLINE RW 8B   vns deadline; write 0 disarms; one-shot
//!   0x20 TIMER_VECTOR   RW 4B   vector to inject (guest picks, default 0x30)
//!   0x24 FREQ_NUM       RO 4B   clock rational numerator
//!   0x28 FREQ_DEN       RO 4B   clock rational denominator
//!
//! The clock rational is per-VM-immutable MachineConfig state: the device
//! owns a copy from construction (the iteration-10 DevCtx ruling — DevCtx
//! carries only icount). `dh_vmm::vt::ClockRatio` is the authoritative
//! conversion type; the formula here is the same ARCH §4 pure function
//! (u128 intermediate, truncating), duplicated because dh-vmm depends on
//! this crate, not vice versa.

use crate::ctx::DevCtx;
use crate::{DetDevice, RestoreError};

pub const PV_CLOCK_BASE: u64 = 0xD000_0000;
pub const DEVICE_ID_PV_CLOCK: u16 = 0x0002;

pub const REG_VNS: u64 = 0x08;
pub const REG_ICOUNT: u64 = 0x10;
pub const REG_TIMER_DEADLINE: u64 = 0x18;
pub const REG_TIMER_VECTOR: u64 = 0x20;
pub const REG_FREQ_NUM: u64 = 0x24;
pub const REG_FREQ_DEN: u64 = 0x28;

pub const DEFAULT_TIMER_VECTOR: u32 = 0x30;

const SECTION_VERSION: u16 = 1;
/// Section: timer_deadline_vns u64 || timer_vector u32 (12 bytes). The
/// rational is NOT here — it is MachineConfig (MCFG section), immutable.
const SECTION_LEN: usize = 12;

pub struct PvClock {
    num: u32,
    den: u32,
    timer_deadline_vns: u64,
    timer_vector: u32,
}

impl PvClock {
    /// `num`/`den` from the validated MachineConfig (both nonzero).
    pub fn new(num: u32, den: u32) -> Self {
        Self {
            num,
            den,
            timer_deadline_vns: 0,
            timer_vector: DEFAULT_TIMER_VECTOR,
        }
    }

    /// ARCH §4 conversion at this boundary. Saturates at u64::MAX rather
    /// than wrapping — deterministic, monotone, and unreachable for sane
    /// rationals within a segment's icount range.
    fn vns(&self, icount: u64) -> u64 {
        let v = u128::from(icount) * u128::from(self.num) / u128::from(self.den);
        u64::try_from(v).unwrap_or(u64::MAX)
    }

    /// Armed one-shot deadline for run control's agenda compilation:
    /// `Some((deadline_vns, vector))` if armed. Run control converts the
    /// deadline to a target icount (vt::icount_for_vns_target), inserts the
    /// agenda point, and calls [`Self::disarm`] when the timer fires (it is
    /// one-shot); it also logs the AUX TIMER_FIRE record at delivery.
    pub fn armed(&self) -> Option<(u64, u8)> {
        (self.timer_deadline_vns != 0).then_some((self.timer_deadline_vns, self.timer_vector as u8))
    }

    /// One-shot consumption at fire time (run control only).
    pub fn disarm(&mut self) {
        self.timer_deadline_vns = 0;
    }

    /// Serve an 8-byte register as a full read or either 4-byte half
    /// (§6.2 names the halves VNS_LO/HI; each half is computed at its own
    /// exit boundary — a guest wanting an untorn value reads 8 bytes).
    fn serve_u64(value: u64, off_in_reg: u64, data: &mut [u8]) {
        let bytes = value.to_le_bytes();
        match (off_in_reg, data.len()) {
            (0, 8) => data.copy_from_slice(&bytes),
            (0, 4) => data.copy_from_slice(&bytes[..4]),
            (4, 4) => data.copy_from_slice(&bytes[4..]),
            _ => data.fill(0),
        }
    }
}

impl DetDevice for PvClock {
    fn device_id(&self) -> u16 {
        DEVICE_ID_PV_CLOCK
    }

    fn section_version(&self) -> u16 {
        SECTION_VERSION
    }

    fn mmio_read(&mut self, off: u64, data: &mut [u8], ctx: &mut DevCtx) {
        match off {
            REG_VNS | 0x0C => Self::serve_u64(self.vns(ctx.icount), off - REG_VNS, data),
            REG_ICOUNT | 0x14 => Self::serve_u64(ctx.icount, off - REG_ICOUNT, data),
            REG_TIMER_DEADLINE | 0x1C => {
                Self::serve_u64(self.timer_deadline_vns, off - REG_TIMER_DEADLINE, data)
            }
            REG_TIMER_VECTOR if data.len() == 4 => {
                data.copy_from_slice(&self.timer_vector.to_le_bytes())
            }
            REG_FREQ_NUM if data.len() == 4 => data.copy_from_slice(&self.num.to_le_bytes()),
            REG_FREQ_DEN if data.len() == 4 => data.copy_from_slice(&self.den.to_le_bytes()),
            _ => data.fill(0),
        }
    }

    fn mmio_write(&mut self, off: u64, data: &[u8], _ctx: &mut DevCtx) {
        match off {
            // RW: deadline (8B write; write 0 disarms).
            REG_TIMER_DEADLINE if data.len() == 8 => {
                self.timer_deadline_vns = u64::from_le_bytes(data.try_into().unwrap());
            }
            REG_TIMER_VECTOR if data.len() == 4 => {
                self.timer_vector = u32::from_le_bytes(data.try_into().unwrap());
            }
            // RO registers and unknown offsets: writes ignored
            // (deterministic no-op; reads remain pure functions).
            _ => {}
        }
    }

    fn snapshot(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.timer_deadline_vns.to_le_bytes());
        out.extend_from_slice(&self.timer_vector.to_le_bytes());
    }

    fn restore(&mut self, bytes: &[u8], sec_version: u16) -> Result<(), RestoreError> {
        if sec_version != SECTION_VERSION || bytes.len() != SECTION_LEN {
            return Err(RestoreError);
        }
        self.timer_deadline_vns = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
        self.timer_vector = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::test_support::FakeEntropy;
    use crate::ctx::{DevCtx, IrqRequest, VecGuestMem};
    use crate::MmioBus;
    use dh_inputlog::dhilog::{LogWriter, SegmentHeader};

    fn harness() -> (LogWriter, VecGuestMem, FakeEntropy, Vec<IrqRequest>) {
        (
            LogWriter::new(SegmentHeader {
                base_snapshot_id: [0; 32],
                entropy_seed: [0; 32],
                machine_config_hash: [0; 32],
                clock_num: 1,
                clock_den: 1,
            }),
            VecGuestMem(vec![0u8; 16]),
            FakeEntropy(0),
            Vec::new(),
        )
    }

    fn read8(bus: &mut MmioBus, gpa: u64, icount: u64) -> u64 {
        let (mut l, mut m, mut e, mut q) = harness();
        let mut ctx = DevCtx::new(icount, 0, &mut l, &mut m, &mut e, &mut q);
        let mut v = [0u8; 8];
        bus.read(gpa, &mut v, &mut ctx).unwrap();
        u64::from_le_bytes(v)
    }

    fn read4(bus: &mut MmioBus, gpa: u64, icount: u64) -> u32 {
        let (mut l, mut m, mut e, mut q) = harness();
        let mut ctx = DevCtx::new(icount, 0, &mut l, &mut m, &mut e, &mut q);
        let mut v = [0u8; 4];
        bus.read(gpa, &mut v, &mut ctx).unwrap();
        u32::from_le_bytes(v)
    }

    fn write(bus: &mut MmioBus, gpa: u64, data: &[u8]) {
        let (mut l, mut m, mut e, mut q) = harness();
        let mut ctx = DevCtx::new(0, 0, &mut l, &mut m, &mut e, &mut q);
        bus.write(gpa, data, &mut ctx).unwrap();
    }

    fn bus_with_clock(num: u32, den: u32) -> MmioBus {
        let mut bus = MmioBus::new();
        bus.register(PV_CLOCK_BASE, Box::new(PvClock::new(num, den)))
            .unwrap();
        bus
    }

    const B: u64 = PV_CLOCK_BASE;

    #[test]
    fn vns_is_pure_function_of_icount() {
        let mut bus = bus_with_clock(1, 3);
        assert_eq!(read8(&mut bus, B + 0x08, 8), 2); // ARCH §4: 8/3 truncates
        assert_eq!(read8(&mut bus, B + 0x08, 8), 2); // same icount ⇒ same vns
        assert_eq!(read8(&mut bus, B + 0x08, 9), 3);
        assert_eq!(read8(&mut bus, B + 0x10, 9), 9); // ICOUNT mirrors ctx
    }

    #[test]
    fn vns_halves_match_full_read() {
        let mut bus = bus_with_clock(u32::MAX, 1);
        let icount = 1 << 40;
        let full = read8(&mut bus, B + 0x08, icount);
        let lo = read4(&mut bus, B + 0x08, icount);
        let hi = read4(&mut bus, B + 0x0C, icount);
        assert_eq!(full, u64::from(lo) | (u64::from(hi) << 32));
    }

    #[test]
    fn timer_arm_disarm_one_shot() {
        let mut bus = bus_with_clock(1, 1);
        write(&mut bus, B + 0x18, &5000u64.to_le_bytes());
        write(&mut bus, B + 0x20, &0x31u32.to_le_bytes());
        assert_eq!(read8(&mut bus, B + 0x18, 0), 5000);
        assert_eq!(read4(&mut bus, B + 0x20, 0), 0x31);
        // write 0 disarms
        write(&mut bus, B + 0x18, &0u64.to_le_bytes());
        assert_eq!(read8(&mut bus, B + 0x18, 0), 0);
    }

    #[test]
    fn armed_handoff_for_run_control() {
        let mut clk = PvClock::new(1, 1);
        assert_eq!(clk.armed(), None);
        clk.timer_deadline_vns = 9000;
        assert_eq!(clk.armed(), Some((9000, DEFAULT_TIMER_VECTOR as u8)));
        clk.disarm();
        assert_eq!(clk.armed(), None);
    }

    #[test]
    fn ro_registers_ignore_writes() {
        let mut bus = bus_with_clock(7, 9);
        write(&mut bus, B + 0x24, &1u32.to_le_bytes());
        write(&mut bus, B + 0x28, &1u32.to_le_bytes());
        write(&mut bus, B + 0x08, &u64::MAX.to_le_bytes());
        assert_eq!(read4(&mut bus, B + 0x24, 0), 7);
        assert_eq!(read4(&mut bus, B + 0x28, 0), 9);
        assert_eq!(read8(&mut bus, B + 0x08, 0), 0);
    }

    #[test]
    fn magic_version_freq_via_bus() {
        let mut bus = bus_with_clock(2, 5);
        assert_eq!(read4(&mut bus, B, 0), u32::from(DEVICE_ID_PV_CLOCK));
        assert_eq!(read4(&mut bus, B + 4, 0), 1); // section version
        assert_eq!(read4(&mut bus, B + 0x24, 0), 2);
        assert_eq!(read4(&mut bus, B + 0x28, 0), 5);
    }

    #[test]
    fn snapshot_restore_roundtrip() {
        let mut a = PvClock::new(3, 7);
        a.timer_deadline_vns = 1234;
        a.timer_vector = 0x55;
        let mut section = Vec::new();
        a.snapshot(&mut section);
        assert_eq!(section.len(), SECTION_LEN);

        let mut b = PvClock::new(3, 7); // rational comes from MCFG, not the section
        b.restore(&section, SECTION_VERSION).unwrap();
        assert_eq!(b.timer_deadline_vns, 1234);
        assert_eq!(b.timer_vector, 0x55);

        assert_eq!(b.restore(&section, 2), Err(RestoreError));
        assert_eq!(b.restore(&section[..8], SECTION_VERSION), Err(RestoreError));
    }

    #[test]
    fn unknown_offsets_read_zero() {
        let mut bus = bus_with_clock(1, 1);
        assert_eq!(read8(&mut bus, B + 0x30, 99), 0);
        assert_eq!(read4(&mut bus, B + 0x2C, 99), 0);
    }
}
