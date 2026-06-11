//! XSAVE canonicalization (ARCH §8.1, risk R7).
//!
//! `KVM_GET_XSAVE` output varies byte-wise for logically-equal state: a
//! component in its init state has its XSTATE_BV bit CLEAR and its area
//! bytes UNDEFINED (the init optimization skips writing them). The rule:
//! **for each XSAVE component whose XSTATE_BV bit is clear, zero the
//! component area** — then blob equality ⇔ logical-state equality, in both
//! the DHSNAP vCPU section and the state-hash preimage.
//!
//! This module is a PURE byte transform (host-runnable, ungated — it
//! builds and unit-tests on aarch64); the CPUID-derived component layout
//! and the live `KVM_GET_XSAVE` wiring are x86-gated.
//!
//! Standard-format XSAVE area layout (the form KVM returns; XCOMP_BV = 0):
//! - legacy region `[0, 512)`: x87 state at `[0, 24) ∪ [32, 160)`
//!   (component bit 0), MXCSR/MXCSR_MASK at `[24, 32)` (NOT governed by an
//!   XSTATE_BV bit — always written when SSE/AVX are in the request mask,
//!   never zeroed here), XMM0..15 at `[160, 416)` (component bit 1),
//!   reserved/software-available `[416, 512)` (not component state; left
//!   untouched per the §8.1 rule — see the doc note on `canonicalize`).
//! - XSAVE header `[512, 576)`: `XSTATE_BV` u64 at 512, `XCOMP_BV` at 520.
//! - extended components (bits ≥ 2) at CPUID(0xD, bit)-enumerated offsets,
//!   passed in as a table so the transform stays pure.

/// One extended component (bit ≥ 2) from CPUID leaf 0xD enumeration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct XsaveComponent {
    pub bit: u32,
    pub offset: usize,
    pub size: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XsaveError {
    /// Area shorter than legacy region + header (576 bytes).
    TooShort { len: usize },
    /// An extended component's area falls outside the buffer.
    ComponentOutOfBounds { bit: u32 },
}

pub const XSAVE_HEADER_OFFSET: usize = 512;
pub const XSAVE_MIN_LEN: usize = 576;

/// Read XSTATE_BV (LE u64 at offset 512).
pub fn xstate_bv(area: &[u8]) -> Result<u64, XsaveError> {
    if area.len() < XSAVE_MIN_LEN {
        return Err(XsaveError::TooShort { len: area.len() });
    }
    Ok(u64::from_le_bytes(
        area[512..520].try_into().expect("checked length"),
    ))
}

/// The §8.1 canonicalization, in place. `extended` lists the bit-≥2
/// component areas (CPUID-enumerated on the capture host; empty when the
/// guest CPUID masks everything past SSE, as Phase 1 does).
///
/// Deliberately NOT zeroed (the rule is scoped to component areas):
/// MXCSR/MXCSR_MASK `[24, 32)` — real state, always valid in KVM output —
/// and the legacy reserved/software-available bytes `[416, 512)`, which
/// KVM zero-fills; if a kernel ever leaks variance there, the R7
/// fault-injection test shape below is how it would be caught and the rule
/// extended.
pub fn canonicalize(area: &mut [u8], extended: &[XsaveComponent]) -> Result<(), XsaveError> {
    let bv = xstate_bv(area)?;
    if bv & 1 == 0 {
        area[0..24].fill(0); // FCW/FSW/FTW/FOP/FIP/FDP
        area[32..160].fill(0); // ST0..7
    }
    if bv & 2 == 0 {
        area[160..416].fill(0); // XMM0..15
    }
    for c in extended {
        debug_assert!(c.bit >= 2, "legacy bits have fixed areas");
        if bv & (1u64 << c.bit) == 0 {
            let end = c
                .offset
                .checked_add(c.size)
                .filter(|&e| e <= area.len())
                .ok_or(XsaveError::ComponentOutOfBounds { bit: c.bit })?;
            area[c.offset..end].fill(0);
        }
    }
    Ok(())
}

/// The capture host's extended-component table from CPUID leaf 0xD:
/// for every XCR0-supported bit ≥ 2, subleaf `bit` gives (size EAX,
/// offset EBX) in the standard format.
#[cfg(target_arch = "x86_64")]
pub fn host_component_layout() -> Vec<XsaveComponent> {
    let mut out = Vec::new();
    let d0 = unsafe_cpuid(0xD, 0);
    let supported = u64::from(d0.eax) | (u64::from(d0.edx) << 32);
    for bit in 2..64u32 {
        if supported & (1u64 << bit) == 0 {
            continue;
        }
        let sub = unsafe_cpuid(0xD, bit);
        if sub.eax == 0 {
            continue;
        }
        out.push(XsaveComponent {
            bit,
            offset: sub.ebx as usize,
            size: sub.eax as usize,
        });
    }
    out
}

#[cfg(target_arch = "x86_64")]
fn unsafe_cpuid(leaf: u32, subleaf: u32) -> core::arch::x86_64::CpuidResult {
    // Safe on x86_64: CPUID is unprivileged and universally present.
    core::arch::x86_64::__cpuid_count(leaf, subleaf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area_with(bv: u64, fill: u8) -> Vec<u8> {
        let mut a = vec![fill; 1024];
        a[512..520].copy_from_slice(&bv.to_le_bytes());
        a[520..528].copy_from_slice(&0u64.to_le_bytes()); // XCOMP_BV
        a
    }

    #[test]
    fn clear_bits_zero_their_areas_set_bits_survive() {
        // x87 clear, SSE set: x87 areas zeroed, XMM + MXCSR survive.
        let mut a = area_with(0b10, 0xAB);
        canonicalize(&mut a, &[]).unwrap();
        assert!(a[0..24].iter().all(|&b| b == 0));
        assert!(a[32..160].iter().all(|&b| b == 0));
        assert!(a[24..32].iter().all(|&b| b == 0xAB)); // MXCSR untouched
        assert!(a[160..416].iter().all(|&b| b == 0xAB)); // XMM survive

        // SSE clear, x87 set: the reverse.
        let mut a = area_with(0b01, 0xCD);
        canonicalize(&mut a, &[]).unwrap();
        assert!(a[0..24].iter().all(|&b| b == 0xCD));
        assert!(a[160..416].iter().all(|&b| b == 0));
    }

    #[test]
    fn extended_components_follow_the_table() {
        let avx = XsaveComponent {
            bit: 2,
            offset: 576,
            size: 256,
        };
        // AVX clear: its area zeroes; everything else untouched.
        let mut a = area_with(0b011, 0xEE);
        canonicalize(&mut a, &[avx]).unwrap();
        assert!(a[576..832].iter().all(|&b| b == 0));
        assert!(a[832..].iter().all(|&b| b == 0xEE));

        // AVX set: area survives.
        let mut a = area_with(0b111, 0xEE);
        canonicalize(&mut a, &[avx]).unwrap();
        assert!(a[576..832].iter().all(|&b| b == 0xEE));
    }

    #[test]
    fn bounds_are_loud() {
        assert_eq!(
            canonicalize(&mut vec![0; 575], &[]),
            Err(XsaveError::TooShort { len: 575 })
        );
        let oob = XsaveComponent {
            bit: 9,
            offset: 1020,
            size: 8,
        };
        assert_eq!(
            canonicalize(&mut area_with(0, 0), &[oob]),
            Err(XsaveError::ComponentOutOfBounds { bit: 9 })
        );
    }

    /// The R7 fault-injection shape: two areas with IDENTICAL logical state
    /// (same set-bit content) but different garbage in a clear component.
    /// Un-canonicalized they hash differently (the fault the risk register
    /// warns about); canonicalized they are byte-identical.
    #[test]
    fn r7_uncanonicalized_garbage_changes_the_hash_canonical_does_not() {
        let mut a = area_with(0b10, 0x00);
        let mut b = area_with(0b10, 0x00);
        // Same live SSE state in both.
        a[160..416].fill(0x42);
        b[160..416].fill(0x42);
        a[24..32].copy_from_slice(&[0x80, 0x1F, 0, 0, 0xFF, 0xFF, 0, 0]);
        b[24..32].copy_from_slice(&[0x80, 0x1F, 0, 0, 0xFF, 0xFF, 0, 0]);
        // Different init-optimization garbage in the CLEAR x87 component.
        a[40..48].fill(0xDE);
        b[40..48].fill(0xAD);

        // UN-canonicalized: the hashes differ — the R7 fault.
        assert_ne!(blake3::hash(&a), blake3::hash(&b));

        canonicalize(&mut a, &[]).unwrap();
        canonicalize(&mut b, &[]).unwrap();
        assert_eq!(a, b);
        assert_eq!(blake3::hash(&a), blake3::hash(&b));
    }

    #[test]
    fn canonicalization_is_idempotent() {
        let mut a = area_with(0b01, 0x77);
        canonicalize(&mut a, &[]).unwrap();
        let once = a.clone();
        canonicalize(&mut a, &[]).unwrap();
        assert_eq!(a, once);
    }
}

#[cfg(all(test, target_arch = "x86_64"))]
mod live_tests {
    use super::*;

    #[test]
    fn host_layout_is_sane() {
        // No KVM needed — pure CPUID. On any x86_64 host the table must be
        // internally consistent: offsets ≥ 576, nonzero sizes, areas may
        // legitimately overlap nothing below the header.
        for c in host_component_layout() {
            assert!(c.bit >= 2);
            assert!(c.offset >= XSAVE_MIN_LEN, "component {c:?}");
            assert!(c.size > 0);
        }
    }

    #[test]
    fn live_xsave_canonicalizes_and_is_stable() {
        if !crate::kvm::kvm_usable() {
            eprintln!("skipping: /dev/kvm not usable");
            return;
        }
        let sys = crate::kvm::KvmSystem::open().unwrap();
        let slot = sys.create_slot_vm(2 * 1024 * 1024).unwrap();
        let layout = host_component_layout();

        let xs = slot.vcpu.get_xsave().unwrap();
        let mut a: Vec<u8> = xs.region.iter().flat_map(|w| w.to_le_bytes()).collect();
        let bv = xstate_bv(&a).unwrap();
        canonicalize(&mut a, &layout).unwrap();

        // Every clear legacy component is zero post-canonicalization.
        if bv & 1 == 0 {
            assert!(a[0..24].iter().all(|&b| b == 0));
            assert!(a[32..160].iter().all(|&b| b == 0));
        }
        if bv & 2 == 0 {
            assert!(a[160..416].iter().all(|&b| b == 0));
        }

        // Stability: a second GET_XSAVE canonicalizes to identical bytes
        // (no guest ran in between).
        let xs2 = slot.vcpu.get_xsave().unwrap();
        let mut b: Vec<u8> = xs2.region.iter().flat_map(|w| w.to_le_bytes()).collect();
        canonicalize(&mut b, &layout).unwrap();
        assert_eq!(a, b, "canonical XSAVE must be stable across reads");
    }
}
