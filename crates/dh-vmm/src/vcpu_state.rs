//! Full vCPU capture/restore (bead 55f) — the KVM GET/SET round trip that
//! the snapshot engine (qmp) and CoW fork (9e4) decode DHSNAP `VCPU`
//! sections into.
//!
//! Capture = the §8.1 state set: REGS, SREGS, FPU, XSAVE (CANONICALIZED —
//! crate::xsave, beads ec4/69), XCRS, the explicit MSR list (msr.rs order,
//! with the normalized-TSC slot filled at restore time, never captured),
//! VCPU_EVENTS, DEBUGREGS.
//!
//! Restore order is normative (ARCH §8.3): SREGS before REGS before
//! VCPU_EVENTS; **XCRS then XSAVE** (the reverse is wrong: XSETBV after
//! XRSTOR would re-init components); FPU before XSAVE (XSAVE is
//! authoritative for the overlap); MSRs last; IA32_TSC ← vns via the
//! `KVM_VCPU_TSC_OFFSET` device attribute per docs/decisions/
//! tsc-alignment.md (tsc.rs — decided, do not reopen).
//!
//! SREGS vs SREGS2: kvm-ioctls 0.24 exposes no SREGS2 wrappers; the pdptr
//! extension matters only for PAE-without-LMA guests, which this machine
//! never runs (same call the Phase-1 hash blob made). The section codec
//! reserves the slot — swapping in kvm_sregs2 when the wrapper lands is a
//! sec_version bump, tracked on the bead.
//!
//! XSAVE area size: fixed 4096 (`KVM_GET/SET_XSAVE`). `capture` hard-fails
//! if `KVM_CAP_XSAVE2` reports a larger required size (AMX-class hosts)
//! instead of truncating silently — switching to the XSAVE2 ioctls is the
//! documented follow-up on the bead.

use crate::kvm::{KvmError, SlotVm};
use crate::msr::{
    MSR_CSTAR, MSR_EFER, MSR_FS_BASE, MSR_GS_BASE, MSR_KERNEL_GS_BASE, MSR_LSTAR, MSR_PAT,
    MSR_SFMASK, MSR_SPEC_CTRL, MSR_STAR, MSR_SYSENTER_CS, MSR_SYSENTER_EIP, MSR_SYSENTER_ESP,
    MSR_TSC_AUX,
};
use kvm_bindings::{
    kvm_debugregs, kvm_fpu, kvm_msr_entry, kvm_regs, kvm_sregs, kvm_vcpu_events, kvm_xcrs, Msrs,
};

/// §8.1 MSR capture list for the restore codec — same set and order as
/// hash.rs's, WITHOUT the normalized-TSC slot (TSC is written from vns at
/// restore, never carried as captured data).
pub const RESTORE_MSR_LIST: &[u32] = &[
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
    MSR_SPEC_CTRL,
];

pub const XSAVE_AREA_LEN: usize = 4096;

/// Everything KVM_SET_* needs to reconstruct a vCPU at a boundary.
#[derive(Clone, Debug)]
pub struct VcpuState {
    pub regs: kvm_regs,
    pub sregs: kvm_sregs,
    pub fpu: kvm_fpu,
    /// Canonical form (crate::xsave) — exactly [`XSAVE_AREA_LEN`] bytes.
    pub xsave: Vec<u8>,
    pub xcrs: kvm_xcrs,
    /// [`RESTORE_MSR_LIST`] order.
    pub msrs: Vec<kvm_msr_entry>,
    pub events: kvm_vcpu_events,
    pub dbg: kvm_debugregs,
}

impl PartialEq for VcpuState {
    /// Byte-level equality of the canonical section encoding — exactly the
    /// equality the determinism platform cares about.
    fn eq(&self, other: &Self) -> bool {
        encode_section(self) == encode_section(other)
    }
}

fn kvm_err(what: &'static str) -> impl Fn(kvm_ioctls::Error) -> KvmError {
    move |e| KvmError::Open(format!("{what}: {e}"))
}

/// Capture the full state set from a vCPU stopped at a boundary.
pub fn capture(slot: &SlotVm) -> Result<VcpuState, KvmError> {
    let vcpu = &slot.vcpu;

    // Fail closed on hosts whose XSAVE area exceeds the fixed-size ioctl
    // (see module doc).
    let xsave2_size = slot.vm.check_extension_int(kvm_ioctls::Cap::Xsave2).max(0) as usize;
    if xsave2_size > XSAVE_AREA_LEN {
        return Err(KvmError::Open(format!(
            "host XSAVE area is {xsave2_size} bytes > {XSAVE_AREA_LEN}: \
             KVM_GET_XSAVE would truncate; the XSAVE2 switch (bead 55f \
             notes) is required on this host"
        )));
    }

    let xs = vcpu.get_xsave().map_err(kvm_err("KVM_GET_XSAVE"))?;
    let mut xsave: Vec<u8> = xs.region.iter().flat_map(|w| w.to_le_bytes()).collect();
    crate::xsave::canonicalize(&mut xsave, &crate::xsave::host_component_layout())
        .map_err(|e| KvmError::Open(format!("XSAVE canonicalization: {e:?}")))?;

    let entries: Vec<kvm_msr_entry> = RESTORE_MSR_LIST
        .iter()
        .map(|&index| kvm_msr_entry {
            index,
            ..Default::default()
        })
        .collect();
    let mut msrs =
        Msrs::from_entries(&entries).map_err(|e| KvmError::Open(format!("msr alloc: {e:?}")))?;
    let n = vcpu.get_msrs(&mut msrs).map_err(kvm_err("KVM_GET_MSRS"))?;
    if n != RESTORE_MSR_LIST.len() {
        return Err(KvmError::Open(format!(
            "KVM_GET_MSRS returned {n}/{} (first unreadable {:#x})",
            RESTORE_MSR_LIST.len(),
            RESTORE_MSR_LIST[n]
        )));
    }

    Ok(VcpuState {
        regs: vcpu.get_regs().map_err(kvm_err("KVM_GET_REGS"))?,
        sregs: vcpu.get_sregs().map_err(kvm_err("KVM_GET_SREGS"))?,
        fpu: vcpu.get_fpu().map_err(kvm_err("KVM_GET_FPU"))?,
        xsave,
        xcrs: vcpu.get_xcrs().map_err(kvm_err("KVM_GET_XCRS"))?,
        msrs: msrs.as_slice().to_vec(),
        events: vcpu
            .get_vcpu_events()
            .map_err(kvm_err("KVM_GET_VCPU_EVENTS"))?,
        dbg: vcpu
            .get_debug_regs()
            .map_err(kvm_err("KVM_GET_DEBUGREGS"))?,
    })
}

/// Restore in the §8.3 order; `vns` becomes the guest TSC via the
/// TSC_OFFSET attribute (`guest_tsc = host_tsc + offset`).
pub fn restore(slot: &SlotVm, st: &VcpuState, vns: u64) -> Result<(), KvmError> {
    let vcpu = &slot.vcpu;
    if st.xsave.len() != XSAVE_AREA_LEN {
        return Err(KvmError::Open(format!(
            "xsave area is {} bytes, want {XSAVE_AREA_LEN}",
            st.xsave.len()
        )));
    }

    // SREGS before REGS before VCPU_EVENTS (§8.3).
    vcpu.set_sregs(&st.sregs)
        .map_err(kvm_err("KVM_SET_SREGS"))?;
    vcpu.set_regs(&st.regs).map_err(kvm_err("KVM_SET_REGS"))?;
    // FPU before XSAVE: XSAVE is authoritative for the x87/SSE overlap.
    vcpu.set_fpu(&st.fpu).map_err(kvm_err("KVM_SET_FPU"))?;
    // XCRS then XSAVE — the reverse is wrong (§8.3): XSETBV after XRSTOR
    // would re-init enabled components.
    vcpu.set_xcrs(&st.xcrs).map_err(kvm_err("KVM_SET_XCRS"))?;
    let mut xs = kvm_bindings::kvm_xsave::default();
    for (i, chunk) in st.xsave.chunks_exact(4).enumerate() {
        xs.region[i] = u32::from_le_bytes(chunk.try_into().expect("chunks_exact(4)"));
    }
    // SAFETY: plain kvm_xsave (no FAM tail in this kvm-bindings version); the area
    // is the canonical form whose clear bits XRSTOR treats as init.
    #[allow(unsafe_code)]
    unsafe { vcpu.set_xsave(&xs) }.map_err(kvm_err("KVM_SET_XSAVE"))?;

    vcpu.set_debug_regs(&st.dbg)
        .map_err(kvm_err("KVM_SET_DEBUGREGS"))?;
    vcpu.set_vcpu_events(&st.events)
        .map_err(kvm_err("KVM_SET_VCPU_EVENTS"))?;

    // MSRs last (§8.3).
    let msrs =
        Msrs::from_entries(&st.msrs).map_err(|e| KvmError::Open(format!("msr alloc: {e:?}")))?;
    let n = vcpu.set_msrs(&msrs).map_err(kvm_err("KVM_SET_MSRS"))?;
    if n != st.msrs.len() {
        return Err(KvmError::Open(format!(
            "KVM_SET_MSRS wrote {n}/{} (first unwritable {:#x})",
            st.msrs.len(),
            st.msrs[n].index
        )));
    }

    // IA32_TSC ← vns via the TSC_OFFSET attribute (tsc-alignment.md).
    crate::tsc::set_tsc_value_with_offset(vcpu, vns)?;
    Ok(())
}

// ---- DHSNAP `VCPU` section codec (API.md §4 table order) ---------------------

pub const VCPU_SECTION_VERSION: u16 = 1;

/// Raw little-endian struct bytes: every kvm-bindings struct here is
/// `repr(C)` plain integers/arrays (any bit pattern valid), which is what
/// API.md §4 specifies for the section ("raw kvm-bindings structs,
/// byte-copied — they are repr(C), stable ABI").
fn struct_bytes<T>(t: &T) -> &[u8] {
    // SAFETY: T is a repr(C) kvm-bindings struct of plain old data; the
    // slice borrows t for its lifetime.
    #[allow(unsafe_code)]
    unsafe {
        std::slice::from_raw_parts(std::ptr::from_ref(t).cast::<u8>(), std::mem::size_of::<T>())
    }
}

fn read_struct<T: Copy + Default>(bytes: &[u8], at: &mut usize) -> Result<T, KvmError> {
    let size = std::mem::size_of::<T>();
    let end = at
        .checked_add(size)
        .filter(|&e| e <= bytes.len())
        .ok_or_else(|| KvmError::Open("VCPU section truncated".into()))?;
    let mut out = T::default();
    // SAFETY: T is repr(C) plain old data (any bit pattern valid); the
    // source range is bounds-checked; copy is by value into a local.
    #[allow(unsafe_code)]
    unsafe {
        std::ptr::copy_nonoverlapping(
            bytes[*at..end].as_ptr(),
            std::ptr::from_mut(&mut out).cast::<u8>(),
            size,
        );
    }
    *at = end;
    Ok(out)
}

/// Encode the DHSNAP `VCPU` section contents (framing is dh-snapshot's):
/// regs ‖ sregs ‖ fpu ‖ len-prefixed canonical XSAVE ‖ xcrs ‖
/// msr count u32 + (index u32, _pad u32, value u64)* ‖ events ‖ debugregs.
pub fn encode_section(st: &VcpuState) -> Vec<u8> {
    let mut out = Vec::with_capacity(8192);
    out.extend_from_slice(struct_bytes(&st.regs));
    out.extend_from_slice(struct_bytes(&st.sregs));
    out.extend_from_slice(struct_bytes(&st.fpu));
    out.extend_from_slice(&(st.xsave.len() as u32).to_le_bytes());
    out.extend_from_slice(&st.xsave);
    out.extend_from_slice(struct_bytes(&st.xcrs));
    out.extend_from_slice(&(st.msrs.len() as u32).to_le_bytes());
    for e in &st.msrs {
        out.extend_from_slice(&e.index.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // _pad
        out.extend_from_slice(&e.data.to_le_bytes());
    }
    out.extend_from_slice(struct_bytes(&st.events));
    out.extend_from_slice(struct_bytes(&st.dbg));
    out
}

/// Decode a DHSNAP `VCPU` section. Total: rejects truncation, trailing
/// bytes, wrong XSAVE length, and an MSR list diverging from
/// [`RESTORE_MSR_LIST`] (the list is versioned in code, §8.1).
pub fn decode_section(bytes: &[u8], sec_version: u16) -> Result<VcpuState, KvmError> {
    if sec_version != VCPU_SECTION_VERSION {
        return Err(KvmError::Open(format!(
            "VCPU section version {sec_version}, want {VCPU_SECTION_VERSION}"
        )));
    }
    let mut at = 0usize;
    let regs: kvm_regs = read_struct(bytes, &mut at)?;
    let sregs: kvm_sregs = read_struct(bytes, &mut at)?;
    let fpu: kvm_fpu = read_struct(bytes, &mut at)?;

    let xlen = u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or_else(|| KvmError::Open("VCPU section truncated at xsave len".into()))?
            .try_into()
            .expect("4 bytes"),
    ) as usize;
    at += 4;
    if xlen != XSAVE_AREA_LEN {
        return Err(KvmError::Open(format!(
            "xsave len {xlen}, want {XSAVE_AREA_LEN}"
        )));
    }
    let xsave = bytes
        .get(at..at + xlen)
        .ok_or_else(|| KvmError::Open("VCPU section truncated in xsave".into()))?
        .to_vec();
    at += xlen;

    let xcrs: kvm_xcrs = read_struct(bytes, &mut at)?;

    let count = u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or_else(|| KvmError::Open("VCPU section truncated at msr count".into()))?
            .try_into()
            .expect("4 bytes"),
    ) as usize;
    at += 4;
    if count != RESTORE_MSR_LIST.len() {
        return Err(KvmError::Open(format!(
            "msr count {count}, want {} (list is code-versioned)",
            RESTORE_MSR_LIST.len()
        )));
    }
    let mut msrs = Vec::with_capacity(count);
    for (i, &expected) in RESTORE_MSR_LIST.iter().enumerate() {
        let rec = bytes
            .get(at..at + 16)
            .ok_or_else(|| KvmError::Open("VCPU section truncated in msrs".into()))?;
        let index = u32::from_le_bytes(rec[0..4].try_into().expect("4"));
        if index != expected {
            return Err(KvmError::Open(format!(
                "msr[{i}] is {index:#x}, want {expected:#x}"
            )));
        }
        if rec[4..8] != [0u8; 4] {
            return Err(KvmError::Open(format!("msr[{i}] pad nonzero")));
        }
        let data = u64::from_le_bytes(rec[8..16].try_into().expect("8"));
        msrs.push(kvm_msr_entry {
            index,
            data,
            ..Default::default()
        });
        at += 16;
    }

    let events: kvm_vcpu_events = read_struct(bytes, &mut at)?;
    let dbg: kvm_debugregs = read_struct(bytes, &mut at)?;
    if at != bytes.len() {
        return Err(KvmError::Open(format!(
            "VCPU section has {} trailing bytes",
            bytes.len() - at
        )));
    }
    Ok(VcpuState {
        regs,
        sregs,
        fpu,
        xsave,
        xcrs,
        msrs,
        events,
        dbg,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_state() -> VcpuState {
        let mut xsave = vec![0u8; XSAVE_AREA_LEN];
        xsave[24..28].copy_from_slice(&0x1F80u32.to_le_bytes()); // MXCSR
        VcpuState {
            regs: kvm_regs {
                rip: 0x1000,
                rax: 0xDEAD_BEEF,
                rflags: 2,
                ..Default::default()
            },
            sregs: kvm_sregs {
                cr0: 0x11,
                cr4: 0x200,
                efer: 0x500,
                ..Default::default()
            },
            fpu: kvm_fpu {
                fcw: 0x037F,
                ..Default::default()
            },
            xsave,
            xcrs: Default::default(),
            msrs: RESTORE_MSR_LIST
                .iter()
                .enumerate()
                .map(|(i, &index)| kvm_msr_entry {
                    index,
                    data: 0x1000 + i as u64,
                    ..Default::default()
                })
                .collect(),
            events: Default::default(),
            dbg: kvm_debugregs {
                dr7: 0x400,
                ..Default::default()
            },
        }
    }

    /// Pin the struct ABI sizes the raw-byte section layout depends on: a
    /// kvm-bindings upgrade that changes any size breaks decode of
    /// existing sections and MUST be a sec_version bump, caught here.
    #[test]
    fn struct_abi_sizes_are_pinned() {
        assert_eq!(std::mem::size_of::<kvm_regs>(), 144);
        assert_eq!(std::mem::size_of::<kvm_sregs>(), 312);
        assert_eq!(std::mem::size_of::<kvm_fpu>(), 416);
        assert_eq!(std::mem::size_of::<kvm_xcrs>(), 392);
        assert_eq!(std::mem::size_of::<kvm_vcpu_events>(), 64);
        assert_eq!(std::mem::size_of::<kvm_debugregs>(), 128);
    }

    #[test]
    fn section_roundtrips_and_is_deterministic() {
        let st = synthetic_state();
        let enc = encode_section(&st);
        assert_eq!(enc, encode_section(&st), "encoding is pure");
        let back = decode_section(&enc, VCPU_SECTION_VERSION).unwrap();
        assert_eq!(st, back);
        assert_eq!(enc, encode_section(&back), "re-encode byte-identical");
    }

    #[test]
    fn decode_rejects_malformed_sections() {
        let st = synthetic_state();
        let enc = encode_section(&st);

        assert!(decode_section(&enc, 2).is_err(), "wrong version");
        assert!(
            decode_section(&enc[..enc.len() - 1], 1).is_err(),
            "truncated"
        );
        let mut trailing = enc.clone();
        trailing.push(0);
        assert!(decode_section(&trailing, 1).is_err(), "trailing");

        // Wrong MSR index where the list is code-versioned.
        let mut wrong = enc.clone();
        let regs_len = std::mem::size_of::<kvm_regs>()
            + std::mem::size_of::<kvm_sregs>()
            + std::mem::size_of::<kvm_fpu>();
        let msr0_at = regs_len + 4 + XSAVE_AREA_LEN + std::mem::size_of::<kvm_xcrs>() + 4;
        wrong[msr0_at..msr0_at + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        assert!(decode_section(&wrong, 1).is_err(), "msr list mismatch");
    }

    #[test]
    fn live_get_set_get_roundtrip() {
        if !crate::kvm::kvm_usable() {
            eprintln!("skipping: /dev/kvm not usable");
            return;
        }
        let sys = crate::kvm::KvmSystem::open().unwrap();
        let slot = sys.create_slot_vm(2 * 1024 * 1024).unwrap();

        // Perturb a few restorable fields so the round trip is not
        // trivially comparing defaults.
        let mut regs = slot.vcpu.get_regs().unwrap();
        regs.rax = 0x1234_5678_9ABC_DEF0;
        regs.rip = 0x8000;
        slot.vcpu.set_regs(&regs).unwrap();

        let a = capture(&slot).expect("capture A");
        restore(&slot, &a, 1_000_000).expect("restore A");
        let b = capture(&slot).expect("capture B");
        assert_eq!(a, b, "GET→SET→GET must be a fixed point");

        // The TSC offset attribute reflects the vns write.
        let off = crate::tsc::get_tsc_offset(&slot.vcpu).unwrap();
        assert_ne!(off, 0, "restore must have programmed the TSC offset");

        // And the canonical section encoding is stable across the trip.
        assert_eq!(encode_section(&a), encode_section(&b));
    }

    #[test]
    fn live_restore_overwrites_dirty_state() {
        if !crate::kvm::kvm_usable() {
            eprintln!("skipping: /dev/kvm not usable");
            return;
        }
        let sys = crate::kvm::KvmSystem::open().unwrap();
        let slot = sys.create_slot_vm(2 * 1024 * 1024).unwrap();
        let baseline = capture(&slot).unwrap();

        // Dirty the vCPU: different regs + MSR.
        let mut regs = slot.vcpu.get_regs().unwrap();
        regs.rbx = 0xFFFF;
        regs.rip = 0x9000;
        slot.vcpu.set_regs(&regs).unwrap();

        restore(&slot, &baseline, 5).expect("restore baseline");
        let after = capture(&slot).unwrap();
        assert_eq!(baseline, after, "restore must overwrite dirty state");
    }
}
