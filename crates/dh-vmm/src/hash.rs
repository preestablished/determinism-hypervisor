//! StateHashChain (ARCH §8.5, scoped for Phase 1 / M3 run-twice-compare):
//!
//!   H_0   = blake3("dh-statehash-v1" || machine_config_hash || base_snapshot_ref)
//!   H_i+1 = blake3(H_i || vcpu blob || device sections
//!                  || pages ascending: le64(idx) || 4096 bytes
//!                  || le64(icount) || le64(vns))
//!
//! Phase-1 scoping (M4 extends, never replaces — same harvest order):
//! - the page walk is FULL MEMORY (every page, ascending); the dirty-ring
//!   delta arrives with M4's snapshot codec;
//! - the vCPU blob is the §8.1 non-XSAVE subset (REGS/SREGS2/FPU/
//!   VCPU_EVENTS/DEBUGREGS + the explicit MSR list), serialized
//!   field-by-field little-endian below — XSAVE canonicalization is
//!   deferred to M4 (sequencing guard);
//! - IA32_TSC is hashed in NORMALIZED form (vns), matching §8.1's restore
//!   rule — the raw captured TSC is host state until the M2 alignment
//!   bead lands.
//!
//! The chain VALUE (not just the last link) is the state hash exchanged
//! with other services: comparing chains compares execution histories.

use kvm_bindings::{kvm_msr_entry, kvm_segment, Msrs};
use kvm_ioctls::VcpuFd;
use vm_memory::{Bytes, GuestAddress};

use crate::kvm::{KvmError, SlotVm};
use crate::msr::{
    MSR_CSTAR, MSR_EFER, MSR_FS_BASE, MSR_GS_BASE, MSR_KERNEL_GS_BASE, MSR_LSTAR, MSR_PAT,
    MSR_SFMASK, MSR_SPEC_CTRL, MSR_STAR, MSR_SYSENTER_CS, MSR_SYSENTER_EIP, MSR_SYSENTER_ESP,
    MSR_TSC_AUX,
};

pub const PAGE_SIZE: usize = 4096;

/// §8.1 MSR capture list, in the DOC'S order. IA32_TSC is deliberately
/// absent from the GET list — its blob slot (normalized vns) is inserted
/// at the §8.1 position, between TSC_AUX and SPEC_CTRL, so the M4 DHSNAP
/// codec serializing in doc order produces the same preimage.
const MSR_CAPTURE_LIST: &[u32] = &[
    MSR_EFER,
    MSR_STAR,
    MSR_LSTAR,
    MSR_CSTAR,
    MSR_SFMASK,
    MSR_KERNEL_GS_BASE,
    MSR_FS_BASE,
    MSR_GS_BASE,
    MSR_SYSENTER_CS,
    MSR_SYSENTER_ESP,
    MSR_SYSENTER_EIP,
    MSR_PAT,
    MSR_TSC_AUX,
    // <-- normalized IA32_TSC slot is inserted here at serialization time
    MSR_SPEC_CTRL,
];

/// Index in [`MSR_CAPTURE_LIST`] order BEFORE which the normalized
/// IA32_TSC slot is emitted (right after TSC_AUX, per §8.1).
const TSC_SLOT_AT: usize = 13;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateHashChain {
    value: [u8; 32],
}

impl StateHashChain {
    /// H_0.
    pub fn new(machine_config_hash: &[u8; 32], base_snapshot_ref: &[u8; 32]) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(b"dh-statehash-v1");
        h.update(machine_config_hash);
        h.update(base_snapshot_ref);
        StateHashChain {
            value: *h.finalize().as_bytes(),
        }
    }

    /// The chain value (the state hash).
    pub fn value(&self) -> [u8; 32] {
        self.value
    }

    /// Resume a chain from a checkpointed value (snapshot restore / fork:
    /// the child continues the parent's chain — "comparing chains compares
    /// execution histories" requires the restored chain to keep its past).
    pub fn from_value(value: [u8; 32]) -> Self {
        StateHashChain { value }
    }

    /// Append one link from pre-serialized parts. `pages` must come in
    /// strictly ascending index order with exactly PAGE_SIZE bytes each —
    /// violations are a caller bug (panic, not a guest-influenced path).
    pub fn push_link<'a>(
        &mut self,
        vcpu_blob: &[u8],
        device_sections: &[u8],
        pages: impl Iterator<Item = (u64, &'a [u8])>,
        icount: u64,
        vns: u64,
    ) {
        let mut h = blake3::Hasher::new();
        h.update(&self.value);
        // Length-prefixed: the blob/sections boundary must be unambiguous
        // in the preimage (preimage discipline while v1 is young).
        h.update(&(vcpu_blob.len() as u64).to_le_bytes());
        h.update(vcpu_blob);
        h.update(&(device_sections.len() as u64).to_le_bytes());
        h.update(device_sections);
        let mut last: Option<u64> = None;
        for (idx, bytes) in pages {
            assert_eq!(bytes.len(), PAGE_SIZE, "page {idx} is not 4096 bytes");
            assert!(
                last.is_none_or(|l| idx > l),
                "pages must be strictly ascending (got {idx} after {last:?})"
            );
            last = Some(idx);
            h.update(&idx.to_le_bytes());
            h.update(bytes);
        }
        h.update(&icount.to_le_bytes());
        h.update(&vns.to_le_bytes());
        self.value = *h.finalize().as_bytes();
    }

    /// The Phase-1 final link over a paused slot: canonical vCPU blob,
    /// device sections, FULL guest-RAM walk, position. Call only while the
    /// vCPU is stopped at a boundary.
    pub fn push_final_link(
        &mut self,
        slot: &SlotVm,
        device_sections: &[u8],
        icount: u64,
        vns: u64,
    ) -> Result<(), KvmError> {
        let vcpu_blob = canonical_vcpu_blob(&slot.vcpu, vns)?;
        let mut h = blake3::Hasher::new();
        h.update(&self.value);
        h.update(&(vcpu_blob.len() as u64).to_le_bytes());
        h.update(&vcpu_blob);
        h.update(&(device_sections.len() as u64).to_le_bytes());
        h.update(device_sections);
        let mut page = [0u8; PAGE_SIZE];
        for idx in 0..slot.mem_bytes / PAGE_SIZE as u64 {
            slot.guest_mem
                .read_slice(&mut page, GuestAddress(idx * PAGE_SIZE as u64))
                .map_err(|e| KvmError::Memory(format!("hash page read: {e}")))?;
            h.update(&idx.to_le_bytes());
            h.update(&page);
        }
        h.update(&icount.to_le_bytes());
        h.update(&vns.to_le_bytes());
        self.value = *h.finalize().as_bytes();
        Ok(())
    }
}

fn seg(out: &mut Vec<u8>, s: &kvm_segment) {
    out.extend_from_slice(&s.base.to_le_bytes());
    out.extend_from_slice(&s.limit.to_le_bytes());
    out.extend_from_slice(&s.selector.to_le_bytes());
    out.push(s.type_);
    out.push(s.present);
    out.push(s.dpl);
    out.push(s.db);
    out.push(s.s);
    out.push(s.l);
    out.push(s.g);
    out.push(s.avl);
    out.push(s.unusable);
}

/// The §8.1 non-XSAVE canonical vCPU blob, serialized field-by-field LE
/// (never raw struct memory — padding is not part of machine state).
/// `vns` fills the IA32_TSC slot (normalized form, see module docs).
pub fn canonical_vcpu_blob(vcpu: &VcpuFd, vns: u64) -> Result<Vec<u8>, KvmError> {
    let kvm_err = |what: &str| {
        let what = what.to_string();
        move |e: kvm_ioctls::Error| KvmError::Open(format!("{what}: {e}"))
    };
    let regs = vcpu.get_regs().map_err(kvm_err("KVM_GET_REGS"))?;
    let sregs = vcpu.get_sregs().map_err(kvm_err("KVM_GET_SREGS"))?;
    let fpu = vcpu.get_fpu().map_err(kvm_err("KVM_GET_FPU"))?;
    let events = vcpu
        .get_vcpu_events()
        .map_err(kvm_err("KVM_GET_VCPU_EVENTS"))?;
    let dbg = vcpu
        .get_debug_regs()
        .map_err(kvm_err("KVM_GET_DEBUGREGS"))?;

    let mut out = Vec::with_capacity(1024);

    // KVM_GET_REGS: 16 GPRs + rip + rflags.
    for v in [
        regs.rax,
        regs.rbx,
        regs.rcx,
        regs.rdx,
        regs.rsi,
        regs.rdi,
        regs.rsp,
        regs.rbp,
        regs.r8,
        regs.r9,
        regs.r10,
        regs.r11,
        regs.r12,
        regs.r13,
        regs.r14,
        regs.r15,
        regs.rip,
        regs.rflags,
    ] {
        out.extend_from_slice(&v.to_le_bytes());
    }

    // SREGS: segments, descriptor tables, control registers. (SREGS2's
    // pdptr extension matters only for PAE-without-LMA guests — not this
    // machine; M4's codec owns the SREGS2 upgrade.)
    for s in [
        &sregs.cs, &sregs.ds, &sregs.es, &sregs.fs, &sregs.gs, &sregs.ss, &sregs.tr, &sregs.ldt,
    ] {
        seg(&mut out, s);
    }
    for (base, limit) in [
        (sregs.gdt.base, sregs.gdt.limit),
        (sregs.idt.base, sregs.idt.limit),
    ] {
        out.extend_from_slice(&base.to_le_bytes());
        out.extend_from_slice(&limit.to_le_bytes());
    }
    for v in [
        sregs.cr0,
        sregs.cr2,
        sregs.cr3,
        sregs.cr4,
        sregs.cr8,
        sregs.efer,
        sregs.apic_base,
    ] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&sregs.interrupt_bitmap[0].to_le_bytes());
    out.extend_from_slice(&sregs.interrupt_bitmap[1].to_le_bytes());
    out.extend_from_slice(&sregs.interrupt_bitmap[2].to_le_bytes());
    out.extend_from_slice(&sregs.interrupt_bitmap[3].to_le_bytes());

    // FPU (x87/SSE control + registers; XSAVE proper is M4).
    for fpr in &fpu.fpr {
        out.extend_from_slice(fpr);
    }
    out.extend_from_slice(&fpu.fcw.to_le_bytes());
    out.extend_from_slice(&fpu.fsw.to_le_bytes());
    out.push(fpu.ftwx);
    out.extend_from_slice(&fpu.last_opcode.to_le_bytes());
    out.extend_from_slice(&fpu.last_ip.to_le_bytes());
    out.extend_from_slice(&fpu.last_dp.to_le_bytes());
    for xmm in &fpu.xmm {
        out.extend_from_slice(xmm);
    }
    // MXCSR — sourced from KVM_GET_XSAVE, NOT kvm_fpu.mxcsr: MEASURED
    // (iteration 51, kernel 6.8.0-124): KVM_GET_FPU reports mxcsr=0
    // regardless of the guest's live value, while GET_XSAVE carries the
    // truth (FXSAVE-area byte offset 24 = region[6]). With CR4.OSFXSR
    // on (bead ttk) a guest can change its rounding mode; hashing the
    // lying field would let it escape replay verification.
    let xsave = vcpu.get_xsave().map_err(kvm_err("KVM_GET_XSAVE"))?;
    out.extend_from_slice(&xsave.region[6].to_le_bytes());

    // VCPU_EVENTS: pending exception/interrupt/NMI/SMI state.
    // exception_has_payload/exception_payload are deliberately omitted in
    // Phase 1: hash points are instruction boundaries with nothing in
    // flight (the boundary engine guarantees it); the M4 codec revisits
    // alongside XSAVE.
    out.push(events.exception.injected);
    out.push(events.exception.nr);
    out.push(events.exception.has_error_code);
    out.push(events.exception.pending);
    out.extend_from_slice(&events.exception.error_code.to_le_bytes());
    out.push(events.interrupt.injected);
    out.push(events.interrupt.nr);
    out.push(events.interrupt.soft);
    out.push(events.interrupt.shadow);
    out.push(events.nmi.injected);
    out.push(events.nmi.pending);
    out.push(events.nmi.masked);
    out.extend_from_slice(&events.sipi_vector.to_le_bytes());
    out.extend_from_slice(&events.flags.to_le_bytes());
    out.push(events.smi.smm);
    out.push(events.smi.pending);
    out.push(events.smi.smm_inside_nmi);
    out.push(events.smi.latched_init);
    out.push(events.triple_fault.pending);

    // DEBUGREGS.
    for db in &dbg.db {
        out.extend_from_slice(&db.to_le_bytes());
    }
    out.extend_from_slice(&dbg.dr6.to_le_bytes());
    out.extend_from_slice(&dbg.dr7.to_le_bytes());
    out.extend_from_slice(&dbg.flags.to_le_bytes());

    // Explicit MSR list (§8.1), then the normalized IA32_TSC slot.
    let entries: Vec<kvm_msr_entry> = MSR_CAPTURE_LIST
        .iter()
        .map(|&index| kvm_msr_entry {
            index,
            ..Default::default()
        })
        .collect();
    let mut msrs = Msrs::from_entries(&entries)
        .map_err(|e| KvmError::Open(format!("msr list alloc: {e:?}")))?;
    let n = vcpu.get_msrs(&mut msrs).map_err(kvm_err("KVM_GET_MSRS"))?;
    if n != MSR_CAPTURE_LIST.len() {
        // get_msrs stops at the first unreadable index — name it. A host
        // that cannot read a captured MSR cannot hash faithfully; loud
        // failure is the §2.1 posture.
        return Err(KvmError::Open(format!(
            "KVM_GET_MSRS returned {n}/{} entries (first unreadable MSR {:#x})",
            MSR_CAPTURE_LIST.len(),
            MSR_CAPTURE_LIST[n]
        )));
    }
    for (i, e) in msrs.as_slice().iter().enumerate() {
        if i == TSC_SLOT_AT {
            // IA32_TSC, normalized to vns (§8.1 restore rule; raw TSC is
            // host state until the M2 alignment bead) — at the doc-order
            // position so the M4 codec preimage matches.
            out.extend_from_slice(&0x10u32.to_le_bytes());
            out.extend_from_slice(&vns.to_le_bytes());
        }
        out.extend_from_slice(&e.index.to_le_bytes());
        out.extend_from_slice(&e.data.to_le_bytes());
    }

    Ok(out)
}

/// Frame device sections unambiguously: (device_id, section_version,
/// len, bytes) per device, in bus registration (base-address) order.
pub fn device_sections(bus: &dh_devices::MmioBus) -> Vec<u8> {
    let mut out = Vec::new();
    for (_base, dev) in bus.devices() {
        let mut section = Vec::new();
        dev.snapshot(&mut section);
        out.extend_from_slice(&dev.device_id().to_le_bytes());
        out.extend_from_slice(&dev.section_version().to_le_bytes());
        out.extend_from_slice(&(section.len() as u32).to_le_bytes());
        out.extend_from_slice(&section);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const MC: [u8; 32] = [7u8; 32];
    const BASE: [u8; 32] = [9u8; 32];

    #[test]
    fn mxcsr_is_in_the_blob_from_the_truthful_source() {
        // MEASURED (iteration 51): KVM_GET_FPU reports mxcsr=0 always;
        // the blob must source it from KVM_GET_XSAVE or the guest's
        // rounding mode escapes replay verification.
        if std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/kvm")
            .is_err()
        {
            eprintln!("skipping: /dev/kvm not usable");
            return;
        }
        let sys = crate::kvm::KvmSystem::open().unwrap();
        let slot = sys.create_slot_vm(2 << 20).unwrap();
        let a = canonical_vcpu_blob(&slot.vcpu, 0).unwrap();
        let mut xsave = slot.vcpu.get_xsave().unwrap();
        xsave.region[6] = 0x7F80; // flip a rounding-control-relevant bit set
        xsave.region[128] |= 0b10; // XSTATE_BV.SSE: the area is live, not init
                                   // SAFETY: plain kvm_xsave (no FAM tail in the 0.24 binding); the
                                   // struct came from GET_XSAVE on this same vCPU.
        #[allow(unsafe_code)]
        unsafe {
            slot.vcpu.set_xsave(&xsave).unwrap();
        }
        let b = canonical_vcpu_blob(&slot.vcpu, 0).unwrap();
        assert_ne!(a, b, "an MXCSR change must change the canonical blob");
    }

    #[test]
    fn h0_is_deterministic_and_input_sensitive() {
        let a = StateHashChain::new(&MC, &BASE);
        let b = StateHashChain::new(&MC, &BASE);
        assert_eq!(a.value(), b.value());
        let c = StateHashChain::new(&[8u8; 32], &BASE);
        assert_ne!(a.value(), c.value());
    }

    #[test]
    fn links_chain_and_every_input_matters() {
        let page_a = [1u8; PAGE_SIZE];
        let page_b = [2u8; PAGE_SIZE];
        let base = || {
            let mut c = StateHashChain::new(&MC, &BASE);
            c.push_link(
                b"vcpu",
                b"devs",
                [(0u64, &page_a[..]), (5u64, &page_b[..])].into_iter(),
                100,
                200,
            );
            c
        };
        let reference = base().value();
        assert_eq!(base().value(), reference, "deterministic");

        // Chain position matters: a second identical link changes the value.
        let mut two = base();
        two.push_link(
            b"vcpu",
            b"devs",
            [(0u64, &page_a[..]), (5u64, &page_b[..])].into_iter(),
            100,
            200,
        );
        assert_ne!(two.value(), reference);

        // Every component perturbs the hash.
        let variants: Vec<[u8; 32]> = vec![
            {
                let mut c = StateHashChain::new(&MC, &BASE);
                c.push_link(
                    b"vcpX",
                    b"devs",
                    [(0u64, &page_a[..]), (5u64, &page_b[..])].into_iter(),
                    100,
                    200,
                );
                c.value()
            },
            {
                let mut c = StateHashChain::new(&MC, &BASE);
                c.push_link(
                    b"vcpu",
                    b"devs",
                    [(0u64, &page_a[..]), (6u64, &page_b[..])].into_iter(),
                    100,
                    200,
                );
                c.value()
            },
            {
                let mut c = StateHashChain::new(&MC, &BASE);
                c.push_link(
                    b"vcpu",
                    b"devs",
                    [(0u64, &page_a[..]), (5u64, &page_a[..])].into_iter(),
                    100,
                    200,
                );
                c.value()
            },
            {
                let mut c = StateHashChain::new(&MC, &BASE);
                c.push_link(
                    b"vcpu",
                    b"devs",
                    [(0u64, &page_a[..]), (5u64, &page_b[..])].into_iter(),
                    101,
                    200,
                );
                c.value()
            },
            {
                let mut c = StateHashChain::new(&MC, &BASE);
                c.push_link(
                    b"vcpu",
                    b"devs",
                    [(0u64, &page_a[..]), (5u64, &page_b[..])].into_iter(),
                    100,
                    201,
                );
                c.value()
            },
        ];
        for v in &variants {
            assert_ne!(*v, reference);
        }
    }

    #[test]
    #[should_panic(expected = "strictly ascending")]
    fn out_of_order_pages_panic() {
        let page = [0u8; PAGE_SIZE];
        let mut c = StateHashChain::new(&MC, &BASE);
        c.push_link(
            b"",
            b"",
            [(5u64, &page[..]), (3u64, &page[..])].into_iter(),
            0,
            0,
        );
    }

    #[test]
    fn vcpu_blob_is_stable_across_reads_live() {
        if !crate::kvm::kvm_usable() {
            eprintln!("skipping: /dev/kvm not usable");
            return;
        }
        let sys = crate::kvm::KvmSystem::open().unwrap();
        let slot = sys.create_slot_vm(2 * 1024 * 1024).unwrap();
        let a = canonical_vcpu_blob(&slot.vcpu, 42).unwrap();
        let b = canonical_vcpu_blob(&slot.vcpu, 42).unwrap();
        assert_eq!(a, b, "capture must be read-stable");
        let c = canonical_vcpu_blob(&slot.vcpu, 43).unwrap();
        assert_ne!(a, c, "normalized TSC slot must reflect vns");
    }

    #[test]
    fn final_link_sees_guest_ram_live() {
        use vm_memory::Bytes;
        if !crate::kvm::kvm_usable() {
            eprintln!("skipping: /dev/kvm not usable");
            return;
        }
        let sys = crate::kvm::KvmSystem::open().unwrap();
        let slot = sys.create_slot_vm(2 * 1024 * 1024).unwrap();

        let mut a = StateHashChain::new(&MC, &BASE);
        a.push_final_link(&slot, b"", 1, 1).unwrap();
        let mut b = StateHashChain::new(&MC, &BASE);
        b.push_final_link(&slot, b"", 1, 1).unwrap();
        assert_eq!(a.value(), b.value(), "same state, same hash");

        // One flipped byte anywhere in RAM must change the hash.
        slot.guest_mem
            .write_slice(&[0xA5], vm_memory::GuestAddress(0x1F_F123))
            .unwrap();
        let mut c = StateHashChain::new(&MC, &BASE);
        c.push_final_link(&slot, b"", 1, 1).unwrap();
        assert_ne!(a.value(), c.value(), "RAM byte flip must perturb the hash");
    }
}

#[cfg(test)]
mod device_section_tests {
    use super::*;
    use dh_devices::blk::{BaseIoError, BlockBase, PvBlk};
    use dh_devices::pad::PvPad;
    use dh_devices::MmioBus;

    struct ZeroBase;
    impl BlockBase for ZeroBase {
        fn len_bytes(&self) -> u64 {
            4096
        }
        fn read_at(&self, _offset: u64, buf: &mut [u8]) -> Result<(), BaseIoError> {
            buf.fill(0);
            Ok(())
        }
    }

    #[test]
    fn device_sections_frame_id_version_len_bytes_in_bus_order() {
        let mut bus = MmioBus::new();
        bus.register(0xD000_1000, Box::new(PvPad::new())).unwrap();
        bus.register(0xD000_4000, Box::new(PvBlk::new(Box::new(ZeroBase))))
            .unwrap();
        let bytes = device_sections(&bus);

        // pv-pad first (lower base): id 0x0003, version 1, len 24.
        assert_eq!(u16::from_le_bytes(bytes[0..2].try_into().unwrap()), 0x0003);
        assert_eq!(u16::from_le_bytes(bytes[2..4].try_into().unwrap()), 1);
        let pad_len = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        assert_eq!(pad_len, 24);
        // pv-blk follows immediately after pad's section.
        let at = 8 + pad_len;
        assert_eq!(
            u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()),
            0x0005
        );
        // Deterministic: two harvests are byte-identical.
        assert_eq!(bytes, device_sections(&bus));
    }
}
