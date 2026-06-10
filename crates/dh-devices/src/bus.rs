//! MmioBus (ARCH §6/§6.1): dispatches guest MMIO exits to device windows
//! and enforces the register convention uniformly — 4 KiB window per
//! device, `0x00 MAGIC` (RO, device id) / `0x04 VERSION` (RO) served by the
//! bus itself, 4- or 8-byte naturally aligned accesses only, anything else
//! is a guest fault (returned to the caller; the VMM injects the fault).

use crate::ctx::DevCtx;
use crate::DetDevice;

pub const WINDOW_LEN: u64 = 4096;
pub const REG_MAGIC: u64 = 0x00;
pub const REG_VERSION: u64 = 0x04;
/// First device-specific register (§6.1).
pub const REG_DEVICE_BASE: u64 = 0x08;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BusError {
    /// No device window covers the address.
    Unmapped,
    /// Access size not 4/8 bytes, or not naturally aligned, or crossing the
    /// window end — guest fault per §6.1.
    GuestFault,
    /// MAGIC/VERSION are read-only.
    ReadOnly,
    /// Registration: window overlaps an existing device or is misaligned.
    BadWindow,
}

pub struct MmioBus {
    /// (base, device), sorted by base; windows are WINDOW_LEN each.
    devices: Vec<(u64, Box<dyn DetDevice>)>,
}

impl Default for MmioBus {
    fn default() -> Self {
        Self::new()
    }
}

impl MmioBus {
    pub fn new() -> Self {
        Self {
            devices: Vec::new(),
        }
    }

    /// Register a device at `base` (must be 4 KiB-aligned, non-overlapping).
    pub fn register(&mut self, base: u64, dev: Box<dyn DetDevice>) -> Result<(), BusError> {
        if !base.is_multiple_of(WINDOW_LEN) || base.checked_add(WINDOW_LEN).is_none() {
            return Err(BusError::BadWindow);
        }
        let idx = match self.devices.binary_search_by_key(&base, |(b, _)| *b) {
            Ok(_) => return Err(BusError::BadWindow),
            Err(i) => i,
        };
        self.devices.insert(idx, (base, dev));
        Ok(())
    }

    fn lookup(&mut self, gpa: u64) -> Result<(u64, &mut Box<dyn DetDevice>), BusError> {
        let idx = match self.devices.binary_search_by_key(&gpa, |(b, _)| *b) {
            Ok(i) => i,
            Err(0) => return Err(BusError::Unmapped),
            Err(i) => i - 1,
        };
        let (base, dev) = &mut self.devices[idx];
        if gpa - *base >= WINDOW_LEN {
            return Err(BusError::Unmapped);
        }
        Ok((*base, dev))
    }

    /// §6.1 access check: 4/8 bytes, naturally aligned, within the window.
    fn check_access(off: u64, len: usize) -> Result<(), BusError> {
        let len64 = len as u64;
        if !(len == 4 || len == 8) || !off.is_multiple_of(len64) {
            return Err(BusError::GuestFault);
        }
        if off + len64 > WINDOW_LEN {
            return Err(BusError::GuestFault);
        }
        Ok(())
    }

    pub fn read(&mut self, gpa: u64, data: &mut [u8], ctx: &mut DevCtx) -> Result<(), BusError> {
        let (base, dev) = self.lookup(gpa)?;
        let off = gpa - base;
        Self::check_access(off, data.len())?;
        match off {
            REG_MAGIC => {
                // 8-byte read at 0x00 would span MAGIC+VERSION; §6.1 keeps
                // both 4-byte registers, so serve them as such.
                if data.len() != 4 {
                    return Err(BusError::GuestFault);
                }
                data.copy_from_slice(&u32::from(dev.device_id()).to_le_bytes());
            }
            REG_VERSION => {
                if data.len() != 4 {
                    return Err(BusError::GuestFault);
                }
                data.copy_from_slice(&u32::from(dev.section_version()).to_le_bytes());
            }
            _ => dev.mmio_read(off, data, ctx),
        }
        Ok(())
    }

    pub fn write(&mut self, gpa: u64, data: &[u8], ctx: &mut DevCtx) -> Result<(), BusError> {
        let (base, dev) = self.lookup(gpa)?;
        let off = gpa - base;
        Self::check_access(off, data.len())?;
        if off < REG_DEVICE_BASE {
            return Err(BusError::ReadOnly);
        }
        dev.mmio_write(off, data, ctx);
        Ok(())
    }

    /// Devices in base order — the deterministic iteration order for
    /// snapshotting (DHSNAP section order must be replay-stable).
    pub fn devices(&self) -> impl Iterator<Item = (u64, &dyn DetDevice)> {
        self.devices.iter().map(|(b, d)| (*b, d.as_ref()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::test_support::FakeEntropy;
    use crate::ctx::{DevCtx, VecGuestMem};
    use dh_inputlog::dhilog::{LogWriter, SegmentHeader};

    /// Minimal device: one RW scratch register at 0x08.
    struct Scratch {
        value: u32,
    }
    impl DetDevice for Scratch {
        fn device_id(&self) -> u16 {
            0x00AB
        }
        fn section_version(&self) -> u16 {
            3
        }
        fn mmio_read(&mut self, off: u64, data: &mut [u8], _ctx: &mut DevCtx) {
            if off == 0x08 && data.len() == 4 {
                data.copy_from_slice(&self.value.to_le_bytes());
            } else {
                data.fill(0);
            }
        }
        fn mmio_write(&mut self, off: u64, data: &[u8], ctx: &mut DevCtx) {
            if off == 0x08 && data.len() == 4 {
                self.value = u32::from_le_bytes(data.try_into().unwrap());
                ctx.request_irq(0x40);
            }
        }
        fn snapshot(&self, out: &mut Vec<u8>) {
            out.extend_from_slice(&self.value.to_le_bytes());
        }
        fn restore(&mut self, bytes: &[u8], _sec_version: u16) -> Result<(), crate::RestoreError> {
            self.value = u32::from_le_bytes(bytes.try_into().map_err(|_| crate::RestoreError)?);
            Ok(())
        }
    }

    fn harness() -> (
        LogWriter,
        VecGuestMem,
        FakeEntropy,
        Vec<crate::ctx::IrqRequest>,
    ) {
        (
            LogWriter::new(SegmentHeader {
                base_snapshot_id: [0; 32],
                entropy_seed: [0; 32],
                machine_config_hash: [0; 32],
                clock_num: 1,
                clock_den: 1,
                encoder_fingerprint: 0,
            }),
            VecGuestMem(vec![0u8; 64]),
            FakeEntropy(0),
            Vec::new(),
        )
    }

    const BASE: u64 = 0xD000_1000;

    fn bus() -> MmioBus {
        let mut bus = MmioBus::new();
        bus.register(BASE, Box::new(Scratch { value: 7 })).unwrap();
        bus
    }

    #[test]
    fn magic_and_version_served_by_bus() {
        let (mut l, mut m, mut e, mut q) = harness();
        let mut ctx = DevCtx::new(1, 0, &mut l, &mut m, &mut e, &mut q);
        let mut bus = bus();
        let mut v = [0u8; 4];
        bus.read(BASE, &mut v, &mut ctx).unwrap();
        assert_eq!(u32::from_le_bytes(v), 0x00AB);
        bus.read(BASE + 4, &mut v, &mut ctx).unwrap();
        assert_eq!(u32::from_le_bytes(v), 3);
        // RO: writes to 0x00/0x04 fault.
        assert_eq!(bus.write(BASE, &v, &mut ctx), Err(BusError::ReadOnly));
        assert_eq!(bus.write(BASE + 4, &v, &mut ctx), Err(BusError::ReadOnly));
    }

    #[test]
    fn device_register_roundtrip_and_irq_queue() {
        let (mut l, mut m, mut e, mut q) = harness();
        let mut bus = bus();
        {
            let mut ctx = DevCtx::new(1, 0, &mut l, &mut m, &mut e, &mut q);
            bus.write(BASE + 8, &0xFEED_F00Du32.to_le_bytes(), &mut ctx)
                .unwrap();
            let mut v = [0u8; 4];
            bus.read(BASE + 8, &mut v, &mut ctx).unwrap();
            assert_eq!(u32::from_le_bytes(v), 0xFEED_F00D);
        }
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].vector, 0x40);
    }

    #[test]
    fn alignment_and_size_faults() {
        let (mut l, mut m, mut e, mut q) = harness();
        let mut ctx = DevCtx::new(1, 0, &mut l, &mut m, &mut e, &mut q);
        let mut bus = bus();
        let mut b2 = [0u8; 2];
        let mut b4 = [0u8; 4];
        let mut b8 = [0u8; 8];
        // wrong size
        assert_eq!(
            bus.read(BASE + 8, &mut b2, &mut ctx),
            Err(BusError::GuestFault)
        );
        // unaligned 4B
        assert_eq!(
            bus.read(BASE + 6, &mut b4, &mut ctx),
            Err(BusError::GuestFault)
        );
        // 8B read at 4-aligned-but-not-8-aligned offset
        assert_eq!(
            bus.read(BASE + 12, &mut b8, &mut ctx),
            Err(BusError::GuestFault)
        );
        // crossing window end
        assert_eq!(
            bus.read(BASE + WINDOW_LEN - 4 + 4, &mut b4, &mut ctx),
            Err(BusError::Unmapped)
        );
        // 8B read at 0x00 would span MAGIC+VERSION
        assert_eq!(bus.read(BASE, &mut b8, &mut ctx), Err(BusError::GuestFault));
        // unmapped address entirely
        assert_eq!(bus.read(0x1000, &mut b4, &mut ctx), Err(BusError::Unmapped));
    }

    #[test]
    fn registration_rules() {
        let mut bus = bus();
        // overlap (same base)
        assert_eq!(
            bus.register(BASE, Box::new(Scratch { value: 0 })),
            Err(BusError::BadWindow)
        );
        // misaligned base
        assert_eq!(
            bus.register(BASE + 8, Box::new(Scratch { value: 0 })),
            Err(BusError::BadWindow)
        );
        // adjacent window is fine; iteration order is base order
        bus.register(BASE + WINDOW_LEN, Box::new(Scratch { value: 0 }))
            .unwrap();
        bus.register(BASE - WINDOW_LEN, Box::new(Scratch { value: 0 }))
            .unwrap();
        let bases: Vec<u64> = bus.devices().map(|(b, _)| b).collect();
        assert_eq!(bases, vec![BASE - WINDOW_LEN, BASE, BASE + WINDOW_LEN]);
    }
}
