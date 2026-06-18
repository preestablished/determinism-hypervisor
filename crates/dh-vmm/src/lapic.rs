//! Minimal deterministic xAPIC/lAPIC model for Linux early boot.
//!
//! This is plain Rust state: no KVM irqchip, no PIT, no host clock, and no
//! TSC-deadline surface. It covers the xAPIC MMIO registers Linux probes
//! before the full interrupt/timer persistence bead lands.

use crate::msr::{MSR_IA32_APIC_BASE, MSR_X2APIC_BASE, MSR_X2APIC_END};
use dh_snapshot::dhsnap::LapcSection;

pub const XAPIC_MMIO_BASE: u64 = 0xfee0_0000;
pub const XAPIC_MMIO_LEN: u64 = 0x1000;

pub const APIC_BASE_BSP: u64 = 1 << 8;
pub const APIC_BASE_X2APIC: u64 = 1 << 10;
pub const APIC_BASE_ENABLE: u64 = 1 << 11;
const APIC_BASE_ADDR_MASK: u64 = 0x000f_ffff_ffff_f000;
pub const RESET_APIC_BASE_MSR: u64 = XAPIC_MMIO_BASE | APIC_BASE_BSP | APIC_BASE_ENABLE;

const REG_ID: u64 = 0x020;
const REG_VERSION: u64 = 0x030;
const REG_TPR: u64 = 0x080;
const REG_APR: u64 = 0x090;
const REG_PPR: u64 = 0x0a0;
const REG_EOI: u64 = 0x0b0;
const REG_LDR: u64 = 0x0d0;
const REG_DFR: u64 = 0x0e0;
const REG_SVR: u64 = 0x0f0;
const REG_ISR_BASE: u64 = 0x100;
const REG_ISR_END: u64 = 0x170;
const REG_TMR_BASE: u64 = 0x180;
const REG_TMR_END: u64 = 0x1f0;
const REG_IRR_BASE: u64 = 0x200;
const REG_IRR_END: u64 = 0x270;
const REG_ESR: u64 = 0x280;
const REG_ICR_LOW: u64 = 0x300;
const REG_ICR_HIGH: u64 = 0x310;
const REG_LVT_TIMER: u64 = 0x320;
const REG_LVT_THERMAL: u64 = 0x330;
const REG_LVT_PERF: u64 = 0x340;
const REG_LVT_LINT0: u64 = 0x350;
const REG_LVT_LINT1: u64 = 0x360;
const REG_LVT_ERROR: u64 = 0x370;
const REG_TIMER_INITIAL: u64 = 0x380;
const REG_TIMER_CURRENT: u64 = 0x390;
const REG_TIMER_DIVIDE: u64 = 0x3e0;

const APIC_VERSION: u32 = (5 << 16) | 0x14;
const SVR_SOFTWARE_ENABLE: u32 = 1 << 8;
const LVT_MASKED: u32 = 1 << 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalApic {
    apic_base_msr: u64,
    id: u8,
    tpr: u8,
    ldr: u32,
    dfr: u32,
    svr: u32,
    isr: [u32; 8],
    tmr: [u32; 8],
    irr: [u32; 8],
    esr: u32,
    icr_low: u32,
    icr_high: u32,
    lvt_timer: u32,
    lvt_thermal: u32,
    lvt_perf: u32,
    lvt_lint0: u32,
    lvt_lint1: u32,
    lvt_error: u32,
    timer_initial: u32,
    timer_divide: u32,
}

impl Default for LocalApic {
    fn default() -> Self {
        Self {
            apic_base_msr: RESET_APIC_BASE_MSR,
            id: 0,
            tpr: 0,
            ldr: 0,
            dfr: 0xffff_ffff,
            svr: 0xff,
            isr: [0; 8],
            tmr: [0; 8],
            irr: [0; 8],
            esr: 0,
            icr_low: 0,
            icr_high: 0,
            lvt_timer: LVT_MASKED,
            lvt_thermal: LVT_MASKED,
            lvt_perf: LVT_MASKED,
            lvt_lint0: LVT_MASKED,
            lvt_lint1: LVT_MASKED,
            lvt_error: LVT_MASKED,
            timer_initial: 0,
            timer_divide: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LapicError {
    BadAccess { offset: u64, len: usize },
    X2ApicUnsupported,
    UnsupportedApicBase { value: u64 },
    UnsupportedTimer { offset: u64, value: u32 },
    UnsupportedIcr { offset: u64, value: u32 },
    MalformedSnapshot(&'static str),
    InterruptsDisabled,
    BadVector { vector: u8 },
}

impl LocalApic {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_lapic_msr(index: u32) -> bool {
        index == MSR_IA32_APIC_BASE || (MSR_X2APIC_BASE..=MSR_X2APIC_END).contains(&index)
    }

    pub fn contains_mmio(gpa: u64) -> bool {
        (XAPIC_MMIO_BASE..XAPIC_MMIO_BASE + XAPIC_MMIO_LEN).contains(&gpa)
    }

    pub fn read_msr(&self, index: u32) -> Result<u64, LapicError> {
        match index {
            MSR_IA32_APIC_BASE => Ok(self.apic_base_msr),
            MSR_X2APIC_BASE..=MSR_X2APIC_END => Err(LapicError::X2ApicUnsupported),
            _ => Err(LapicError::BadAccess {
                offset: u64::from(index),
                len: 8,
            }),
        }
    }

    pub fn write_msr(&mut self, index: u32, value: u64) -> Result<(), LapicError> {
        match index {
            MSR_IA32_APIC_BASE => self.write_apic_base_msr(value),
            MSR_X2APIC_BASE..=MSR_X2APIC_END => Err(LapicError::X2ApicUnsupported),
            _ => Err(LapicError::BadAccess {
                offset: u64::from(index),
                len: 8,
            }),
        }
    }

    pub fn read_mmio(&self, gpa: u64, data: &mut [u8]) -> Result<(), LapicError> {
        let off = checked_mmio_offset(gpa, data.len())?;
        let value = if off.is_multiple_of(0x10) {
            self.read_reg32(off)
        } else {
            0
        };
        write_le(data, value);
        Ok(())
    }

    pub fn write_mmio(&mut self, gpa: u64, data: &[u8]) -> Result<(), LapicError> {
        let off = checked_mmio_offset(gpa, data.len())?;
        if !off.is_multiple_of(0x10) {
            return Ok(());
        }
        self.write_reg32(off, read_le32(data))
    }

    pub fn accept_interrupt(&mut self, vector: u8) -> Result<(), LapicError> {
        if vector < 32 {
            return Err(LapicError::BadVector { vector });
        }
        if self.apic_base_msr & APIC_BASE_ENABLE == 0 || self.svr & SVR_SOFTWARE_ENABLE == 0 {
            return Err(LapicError::InterruptsDisabled);
        }
        set_vector(&mut self.irr, vector);
        Ok(())
    }

    pub fn is_reset(&self) -> bool {
        self == &Self::default()
    }

    pub fn to_lapc_section(&self) -> LapcSection {
        LapcSection {
            apic_base_msr: self.apic_base_msr,
            id: self.id,
            tpr: self.tpr,
            ldr: self.ldr,
            dfr: self.dfr,
            svr: self.svr,
            isr: self.isr,
            tmr: self.tmr,
            irr: self.irr,
            esr: self.esr,
            icr_low: self.icr_low,
            icr_high: self.icr_high,
            lvt_timer: self.lvt_timer,
            lvt_thermal: self.lvt_thermal,
            lvt_perf: self.lvt_perf,
            lvt_lint0: self.lvt_lint0,
            lvt_lint1: self.lvt_lint1,
            lvt_error: self.lvt_error,
            timer_initial: self.timer_initial,
            timer_divide: self.timer_divide,
        }
    }

    pub fn from_lapc_section(section: LapcSection) -> Result<Self, LapicError> {
        let apic_base = section.apic_base_msr;
        if apic_base & APIC_BASE_X2APIC != 0 {
            return Err(LapicError::MalformedSnapshot(
                "x2APIC mode is not persisted",
            ));
        }
        if apic_base & APIC_BASE_ADDR_MASK != XAPIC_MMIO_BASE {
            return Err(LapicError::MalformedSnapshot(
                "APIC base address is not xAPIC",
            ));
        }
        if apic_base & !(APIC_BASE_ADDR_MASK | APIC_BASE_BSP | APIC_BASE_ENABLE) != 0 {
            return Err(LapicError::MalformedSnapshot(
                "APIC base has unsupported bits",
            ));
        }
        if apic_base & APIC_BASE_BSP == 0 {
            return Err(LapicError::MalformedSnapshot("BSP bit must remain set"));
        }
        if section.lvt_timer & LVT_MASKED == 0 {
            return Err(LapicError::MalformedSnapshot("unmasked lAPIC timer"));
        }
        if section.timer_initial != 0 {
            return Err(LapicError::MalformedSnapshot("armed lAPIC timer"));
        }
        if section.icr_low != 0 || section.icr_high != 0 {
            return Err(LapicError::MalformedSnapshot("pending ICR delivery"));
        }
        Ok(Self {
            apic_base_msr: section.apic_base_msr,
            id: section.id,
            tpr: section.tpr,
            ldr: section.ldr,
            dfr: section.dfr,
            svr: section.svr,
            isr: section.isr,
            tmr: section.tmr,
            irr: section.irr,
            esr: section.esr,
            icr_low: section.icr_low,
            icr_high: section.icr_high,
            lvt_timer: section.lvt_timer,
            lvt_thermal: section.lvt_thermal,
            lvt_perf: section.lvt_perf,
            lvt_lint0: section.lvt_lint0,
            lvt_lint1: section.lvt_lint1,
            lvt_error: section.lvt_error,
            timer_initial: section.timer_initial,
            timer_divide: section.timer_divide,
        })
    }

    pub fn next_pending_interrupt(&mut self) -> Option<u8> {
        let min_priority = self.tpr & 0xf0;
        for vector in (32u8..=255).rev() {
            if vector & 0xf0 <= min_priority {
                continue;
            }
            if vector_is_set(&self.irr, vector) {
                clear_vector(&mut self.irr, vector);
                set_vector(&mut self.isr, vector);
                return Some(vector);
            }
        }
        None
    }

    pub fn eoi(&mut self) {
        for vector in (32u8..=255).rev() {
            if vector_is_set(&self.isr, vector) {
                clear_vector(&mut self.isr, vector);
                return;
            }
        }
    }

    fn write_apic_base_msr(&mut self, value: u64) -> Result<(), LapicError> {
        if value & APIC_BASE_X2APIC != 0 {
            return Err(LapicError::X2ApicUnsupported);
        }
        let base = value & APIC_BASE_ADDR_MASK;
        if base != XAPIC_MMIO_BASE {
            return Err(LapicError::UnsupportedApicBase { value });
        }
        self.apic_base_msr = base | APIC_BASE_BSP | (value & APIC_BASE_ENABLE);
        Ok(())
    }

    fn read_reg32(&self, off: u64) -> u32 {
        match off {
            REG_ID => u32::from(self.id) << 24,
            REG_VERSION => APIC_VERSION,
            REG_TPR => u32::from(self.tpr),
            REG_APR | REG_PPR => 0,
            REG_LDR => self.ldr,
            REG_DFR => self.dfr,
            REG_SVR => self.svr,
            REG_ISR_BASE..=REG_ISR_END => self.isr[((off - REG_ISR_BASE) / 0x10) as usize],
            REG_TMR_BASE..=REG_TMR_END => self.tmr[((off - REG_TMR_BASE) / 0x10) as usize],
            REG_IRR_BASE..=REG_IRR_END => self.irr[((off - REG_IRR_BASE) / 0x10) as usize],
            REG_ESR => self.esr,
            REG_ICR_LOW => self.icr_low,
            REG_ICR_HIGH => self.icr_high,
            REG_LVT_TIMER => self.lvt_timer,
            REG_LVT_THERMAL => self.lvt_thermal,
            REG_LVT_PERF => self.lvt_perf,
            REG_LVT_LINT0 => self.lvt_lint0,
            REG_LVT_LINT1 => self.lvt_lint1,
            REG_LVT_ERROR => self.lvt_error,
            REG_TIMER_INITIAL | REG_TIMER_CURRENT => 0,
            REG_TIMER_DIVIDE => self.timer_divide,
            _ => 0,
        }
    }

    fn write_reg32(&mut self, off: u64, value: u32) -> Result<(), LapicError> {
        match off {
            REG_ID | REG_VERSION | REG_APR | REG_PPR | REG_TIMER_CURRENT => {}
            REG_TPR => self.tpr = (value & 0xff) as u8,
            REG_EOI => self.eoi(),
            REG_LDR => self.ldr = value,
            REG_DFR => self.dfr = value,
            REG_SVR => self.svr = value,
            REG_ESR => self.esr = 0,
            REG_ICR_LOW | REG_ICR_HIGH => {
                if value != 0 {
                    return Err(LapicError::UnsupportedIcr { offset: off, value });
                }
                if off == REG_ICR_LOW {
                    self.icr_low = 0;
                } else {
                    self.icr_high = 0;
                }
            }
            REG_LVT_TIMER => {
                if value & LVT_MASKED == 0 {
                    return Err(LapicError::UnsupportedTimer { offset: off, value });
                }
                self.lvt_timer = value;
            }
            REG_LVT_THERMAL => self.lvt_thermal = value,
            REG_LVT_PERF => self.lvt_perf = value,
            REG_LVT_LINT0 => self.lvt_lint0 = value,
            REG_LVT_LINT1 => self.lvt_lint1 = value,
            REG_LVT_ERROR => self.lvt_error = value,
            REG_TIMER_INITIAL => {
                if value != 0 {
                    return Err(LapicError::UnsupportedTimer { offset: off, value });
                }
                self.timer_initial = 0;
            }
            REG_TIMER_DIVIDE => self.timer_divide = value,
            _ => {}
        }
        Ok(())
    }
}

fn checked_mmio_offset(gpa: u64, len: usize) -> Result<u64, LapicError> {
    if !(len == 4 || len == 8) || !LocalApic::contains_mmio(gpa) {
        return Err(LapicError::BadAccess {
            offset: gpa.saturating_sub(XAPIC_MMIO_BASE),
            len,
        });
    }
    let off = gpa - XAPIC_MMIO_BASE;
    if !off.is_multiple_of(len as u64) || off + len as u64 > XAPIC_MMIO_LEN {
        return Err(LapicError::BadAccess { offset: off, len });
    }
    Ok(off)
}

fn read_le32(data: &[u8]) -> u32 {
    let mut bytes = [0u8; 4];
    bytes.copy_from_slice(&data[..4]);
    u32::from_le_bytes(bytes)
}

fn write_le(data: &mut [u8], value: u32) {
    data.fill(0);
    data[..4].copy_from_slice(&value.to_le_bytes());
}

fn vector_word(vector: u8) -> (usize, u32) {
    let word = usize::from(vector / 32);
    let bit = u32::from(vector % 32);
    (word, 1u32 << bit)
}

fn set_vector(words: &mut [u32; 8], vector: u8) {
    let (word, bit) = vector_word(vector);
    words[word] |= bit;
}

fn clear_vector(words: &mut [u32; 8], vector: u8) {
    let (word, bit) = vector_word(vector);
    words[word] &= !bit;
}

fn vector_is_set(words: &[u32; 8], vector: u8) -> bool {
    let (word, bit) = vector_word(vector);
    words[word] & bit != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read4(apic: &LocalApic, off: u64) -> u32 {
        let mut data = [0u8; 4];
        apic.read_mmio(XAPIC_MMIO_BASE + off, &mut data).unwrap();
        u32::from_le_bytes(data)
    }

    fn write4(apic: &mut LocalApic, off: u64, value: u32) -> Result<(), LapicError> {
        apic.write_mmio(XAPIC_MMIO_BASE + off, &value.to_le_bytes())
    }

    #[test]
    fn linux_lapic_reset_values_are_fixed() {
        let apic = LocalApic::new();
        assert_eq!(apic.read_msr(MSR_IA32_APIC_BASE), Ok(RESET_APIC_BASE_MSR));
        assert_eq!(read4(&apic, REG_ID), 0);
        assert_eq!(read4(&apic, REG_VERSION), APIC_VERSION);
        assert_eq!(read4(&apic, REG_SVR), 0xff);
        assert_eq!(read4(&apic, REG_LVT_TIMER), LVT_MASKED);
        assert_eq!(read4(&apic, REG_LVT_LINT0), LVT_MASKED);
        assert_eq!(read4(&apic, REG_TIMER_CURRENT), 0);
        assert_eq!(read4(&apic, REG_IRR_BASE), 0);
        assert_eq!(read4(&apic, REG_ISR_BASE), 0);
    }

    #[test]
    fn linux_lapic_serves_register_reads_and_writes() {
        let mut apic = LocalApic::new();
        write4(&mut apic, REG_TPR, 0x44).unwrap();
        write4(&mut apic, REG_SVR, 0x1ff).unwrap();
        write4(&mut apic, REG_LVT_LINT0, LVT_MASKED | 0x31).unwrap();
        write4(&mut apic, REG_LVT_LINT1, LVT_MASKED | 0x32).unwrap();
        write4(&mut apic, REG_ESR, 0xffff_ffff).unwrap();

        assert_eq!(read4(&apic, REG_TPR), 0x44);
        assert_eq!(read4(&apic, REG_SVR), 0x1ff);
        assert_eq!(read4(&apic, REG_LVT_LINT0), LVT_MASKED | 0x31);
        assert_eq!(read4(&apic, REG_LVT_LINT1), LVT_MASKED | 0x32);
        assert_eq!(read4(&apic, REG_ESR), 0);
        assert_eq!(read4(&apic, REG_ID + 4), 0, "reserved dword reads as zero");
    }

    #[test]
    fn linux_lapic_accepts_interrupts_without_host_irqchip() {
        let mut apic = LocalApic::new();
        assert_eq!(
            apic.accept_interrupt(0x41),
            Err(LapicError::InterruptsDisabled)
        );
        write4(&mut apic, REG_SVR, 0x1ff).unwrap();
        apic.accept_interrupt(0x41).unwrap();
        assert_eq!(read4(&apic, REG_IRR_BASE + 0x20), 1 << 1);
        assert_eq!(apic.next_pending_interrupt(), Some(0x41));
        assert_eq!(read4(&apic, REG_IRR_BASE + 0x20), 0);
        assert_eq!(read4(&apic, REG_ISR_BASE + 0x20), 1 << 1);
        write4(&mut apic, REG_EOI, 0).unwrap();
        assert_eq!(read4(&apic, REG_ISR_BASE + 0x20), 0);
    }

    #[test]
    fn linux_lapic_rejects_timers_and_x2apic_without_host_time() {
        let mut apic = LocalApic::new();
        assert_eq!(
            write4(&mut apic, REG_LVT_TIMER, 0x40),
            Err(LapicError::UnsupportedTimer {
                offset: REG_LVT_TIMER,
                value: 0x40
            })
        );
        assert_eq!(
            write4(&mut apic, REG_TIMER_INITIAL, 1),
            Err(LapicError::UnsupportedTimer {
                offset: REG_TIMER_INITIAL,
                value: 1
            })
        );
        write4(&mut apic, REG_LVT_TIMER, LVT_MASKED | 0x40).unwrap();
        assert_eq!(
            apic.write_msr(MSR_IA32_APIC_BASE, RESET_APIC_BASE_MSR | APIC_BASE_X2APIC),
            Err(LapicError::X2ApicUnsupported)
        );
    }

    #[test]
    fn linux_lapic_rejects_icr_delivery_without_silent_ack() {
        let mut apic = LocalApic::new();
        assert_eq!(
            write4(&mut apic, REG_ICR_LOW, 0x40),
            Err(LapicError::UnsupportedIcr {
                offset: REG_ICR_LOW,
                value: 0x40
            })
        );
        assert_eq!(
            write4(&mut apic, REG_ICR_HIGH, 0x0100_0000),
            Err(LapicError::UnsupportedIcr {
                offset: REG_ICR_HIGH,
                value: 0x0100_0000
            })
        );
        write4(&mut apic, REG_ICR_LOW, 0).unwrap();
        write4(&mut apic, REG_ICR_HIGH, 0).unwrap();
        assert!(apic.is_reset());
    }

    #[test]
    fn linux_lapic_lapc_section_roundtrips_and_rejects_malformed_state() {
        let mut apic = LocalApic::new();
        write4(&mut apic, REG_TPR, 0x44).unwrap();
        write4(&mut apic, REG_LDR, 0x0102_0304).unwrap();
        write4(&mut apic, REG_SVR, 0x0000_01ff).unwrap();
        apic.accept_interrupt(0x41).unwrap();
        assert_eq!(
            LocalApic::from_lapc_section(apic.to_lapc_section()).unwrap(),
            apic
        );

        let mut bad = apic.to_lapc_section();
        bad.apic_base_msr = RESET_APIC_BASE_MSR | APIC_BASE_X2APIC;
        assert_eq!(
            LocalApic::from_lapc_section(bad),
            Err(LapicError::MalformedSnapshot(
                "x2APIC mode is not persisted"
            ))
        );

        let mut bad = apic.to_lapc_section();
        bad.lvt_timer = 0;
        assert_eq!(
            LocalApic::from_lapc_section(bad),
            Err(LapicError::MalformedSnapshot("unmasked lAPIC timer"))
        );
    }

    #[test]
    fn linux_lapic_rejects_noncanonical_apic_base() {
        let mut apic = LocalApic::new();
        assert_eq!(
            apic.write_msr(MSR_IA32_APIC_BASE, 0xfee1_0000 | APIC_BASE_ENABLE),
            Err(LapicError::UnsupportedApicBase {
                value: 0xfee1_0000 | APIC_BASE_ENABLE
            })
        );
        apic.write_msr(MSR_IA32_APIC_BASE, XAPIC_MMIO_BASE | APIC_BASE_ENABLE)
            .unwrap();
        assert_eq!(apic.read_msr(MSR_IA32_APIC_BASE), Ok(RESET_APIC_BASE_MSR));
    }
}
