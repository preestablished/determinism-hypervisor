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
    /// XCOMP_BV bit 63 set: a COMPACTED-format area. The fixed offsets
    /// here are standard-format only; KVM returns standard form, so a
    /// compacted area means the caller fed something else (55f guard).
    CompactedFormat,
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

/// The §8.1 canonicalization, in place — ALLOWLIST semantics: the §8.1
/// goal is "equality of blobs ⇔ equality of logical state", which the
/// literal clear-components-only rule does NOT deliver, because bytes
/// covered by NO component (legacy reserved/software-available
/// `[416, 512)`, header reserved `[528, 576)`, inter-component gaps, the
/// tail past the host's XSAVE size) carry kernel buffer garbage that can
/// vary run-to-run (MEASURED: iteration 68 shipped the subtractive rule
/// and the phase-1 gate flaked DIVERGED under parallel-suite load within
/// the hour). Canonical form = everything zero EXCEPT:
///
/// - MXCSR/MXCSR_MASK `[24, 32)` — always-valid live state, not governed
///   by an XSTATE_BV bit. RESTORE-SAFETY INVARIANT (55f): keeping these
///   bytes unconditionally is what makes the canonical form exact under
///   XRSTOR — MXCSR loads from the memory image whenever RFBM covers
///   SSE/AVX, INDEPENDENT of XSTATE_BV[1], so clearing an init SSE bit
///   never resets a live MXCSR. Do not gate these bytes on a bit;
/// - the x87 area `[0, 24) ∪ [32, 160)` iff NORMALIZED bit 0 is set;
/// - XMM0..15 `[160, 416)` iff normalized bit 1 is set;
/// - each extended component's area iff its normalized bit is set;
/// - the header words `[512, 528)`, with XSTATE_BV rewritten NORMALIZED:
///   a set bit whose area carries the architectural init pattern is
///   canonicalized to clear (KVM reports init state either way depending
///   on host preemption timing — both encodings are the same logical
///   state, measured as the iteration-68/69 gate flake).
///
/// `extended` lists the bit-≥2 component areas (CPUID-enumerated on the
/// capture host; empty when guest CPUID masks everything past SSE, as
/// Phase 1 does).
pub fn canonicalize(area: &mut [u8], extended: &[XsaveComponent]) -> Result<(), XsaveError> {
    let bv = xstate_bv(area)?;
    let xcomp_bv = u64::from_le_bytes(area[520..528].try_into().expect("checked length"));
    if xcomp_bv >> 63 != 0 {
        return Err(XsaveError::CompactedFormat);
    }

    // INIT-STATE NORMALIZATION: for logically-init component state the
    // kernel ABI permits either encoding — bit CLEAR (init optimization,
    // area undefined) or bit SET with the architectural init-pattern
    // bytes — and which one arrives may vary across kernels and fpstate
    // paths (this box's 6.8 consistently reports x87-init as bit-set;
    // the measured iteration-68 flake driver was the OTHER hole, gap
    // garbage — e.g. the real 128-byte non-component gap [832,960) on
    // this host. Both holes are closed here). Both encodings are the
    // SAME logical state; canonical form is bit clear + zero area.
    let mut canon_bv = bv;
    if bv & 1 != 0 && is_x87_init(area) {
        canon_bv &= !1;
    }
    if bv & 2 != 0 && area[160..416].iter().all(|&b| b == 0) {
        canon_bv &= !2;
    }

    // Collect the keep-ranges (bounds-checked), then rebuild the area as
    // zeros + kept bytes. This allowlist shape cannot miss a garbage
    // region by omission (legacy reserved, header reserved, gaps, tail).
    let mut keep: Vec<(usize, usize)> = vec![(24, 32)];
    if canon_bv & 1 != 0 {
        keep.push((0, 24));
        keep.push((32, 160));
    }
    if canon_bv & 2 != 0 {
        keep.push((160, 416));
    }
    for c in extended {
        debug_assert!(c.bit >= 2, "legacy bits have fixed areas");
        let end = c
            .offset
            .checked_add(c.size)
            .filter(|&e| e <= area.len())
            .ok_or(XsaveError::ComponentOutOfBounds { bit: c.bit })?;
        if bv & (1u64 << c.bit) != 0 {
            // all-zero ⇒ init holds for the SSE-like register components
            // (AVX YMM_Hi, AVX-512 ZMM halves/opmask) but NOT for every
            // architectural component — restrict normalization to the
            // bits where it is known-true; others keep their encoding.
            // Phase 1 masks all extended state, so this list is ahead of
            // need; 55f extends it deliberately, never by default.
            const ZERO_INIT_BITS: u64 = (1 << 2) | (1 << 5) | (1 << 6) | (1 << 7);
            if ZERO_INIT_BITS & (1u64 << c.bit) != 0 && area[c.offset..end].iter().all(|&b| b == 0)
            {
                canon_bv &= !(1u64 << c.bit);
            } else {
                keep.push((c.offset, end));
            }
        }
    }

    let mut canon = vec![0u8; area.len()];
    for (a, b) in keep {
        canon[a..b].copy_from_slice(&area[a..b]);
    }
    canon[512..520].copy_from_slice(&canon_bv.to_le_bytes());
    // XCOMP_BV: standard form is 0 (bit63 already rejected above); write
    // the canonical zero rather than passing through whatever arrived.
    canon[520..528].copy_from_slice(&0u64.to_le_bytes());
    area.copy_from_slice(&canon);
    Ok(())
}

/// The architectural x87 init pattern in FXSAVE layout: FCW = 0x037F,
/// FSW = 0, FTW (abridged) = 0 = all-empty, FOP/FIP/FDP = 0, ST0..7 = 0.
fn is_x87_init(area: &[u8]) -> bool {
    area[0..2] == [0x7F, 0x03]
        && area[2..24].iter().all(|&b| b == 0)
        && area[32..160].iter().all(|&b| b == 0)
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
        assert!(a[24..32].iter().all(|&b| b == 0xAB)); // MXCSR kept
        assert!(a[160..416].iter().all(|&b| b == 0xAB)); // XMM survive

        // SSE clear, x87 set: the reverse.
        let mut a = area_with(0b01, 0xCD);
        canonicalize(&mut a, &[]).unwrap();
        assert!(a[0..24].iter().all(|&b| b == 0xCD));
        assert!(a[160..416].iter().all(|&b| b == 0));
    }

    /// The init-encoding flake, pinned: a component in INIT state arrives
    /// from KVM either as bit-clear (area garbage) or bit-set (area =
    /// architectural init pattern) depending on preemption timing. Both
    /// must canonicalize identically — bit clear, area zero.
    #[test]
    fn init_state_normalizes_to_clear_bit_regardless_of_encoding() {
        // Encoding A: x87 bit clear, garbage in the area.
        let mut a = area_with(0b00, 0x00);
        a[0..24].fill(0x99);
        a[32..160].fill(0x99);
        // Encoding B: x87 bit set, architectural init pattern.
        let mut b = area_with(0b01, 0x00);
        b[0..2].copy_from_slice(&[0x7F, 0x03]); // FCW = 0x037F
                                                // Same MXCSR in both (always live).
        a[24..32].copy_from_slice(&0x1F80_0000_FFFFu64.to_le_bytes()[..8]);
        b[24..32].copy_from_slice(&0x1F80_0000_FFFFu64.to_le_bytes()[..8]);

        canonicalize(&mut a, &[]).unwrap();
        canonicalize(&mut b, &[]).unwrap();
        assert_eq!(a, b, "both init encodings must canonicalize identically");
        assert_eq!(xstate_bv(&a).unwrap(), 0);

        // Same for SSE: bit set + all-zero XMMs == bit clear.
        let mut c = area_with(0b10, 0x00);
        let mut d = area_with(0b00, 0x00);
        d[160..416].fill(0x55); // garbage under a clear bit
        canonicalize(&mut c, &[]).unwrap();
        canonicalize(&mut d, &[]).unwrap();
        assert_eq!(c, d);

        // And extended (all-zero area under a set bit ⇒ init ⇒ clear).
        let avx = XsaveComponent {
            bit: 2,
            offset: 576,
            size: 256,
        };
        let mut e = area_with(0b100, 0x00);
        let mut f = area_with(0b000, 0x00);
        canonicalize(&mut e, &[avx]).unwrap();
        canonicalize(&mut f, &[avx]).unwrap();
        assert_eq!(e, f);

        // NON-init x87 state keeps its bit: FCW differs from 0x037F.
        let mut g = area_with(0b01, 0x00);
        g[0..2].copy_from_slice(&[0x7E, 0x03]);
        canonicalize(&mut g, &[]).unwrap();
        assert_eq!(xstate_bv(&g).unwrap(), 1);
        assert_eq!(&g[0..2], &[0x7E, 0x03]);
    }

    /// The iteration-68 flake, pinned: bytes covered by NO component
    /// (legacy reserved [416,512), header reserved [528,576), the tail)
    /// are kernel-buffer garbage and MUST be zero in canonical form —
    /// regardless of which XSTATE_BV bits are set.
    #[test]
    fn non_component_garbage_is_always_zeroed() {
        for bv in [0u64, 0b01, 0b10, 0b11] {
            let mut a = area_with(bv, 0xFF);
            canonicalize(&mut a, &[]).unwrap();
            assert!(
                a[416..512].iter().all(|&b| b == 0),
                "legacy reserved leaked (bv={bv:#b})"
            );
            assert!(
                a[528..576].iter().all(|&b| b == 0),
                "header reserved leaked (bv={bv:#b})"
            );
            assert!(a[576..].iter().all(|&b| b == 0), "tail leaked (bv={bv:#b})");
            // XSTATE_BV itself survives.
            assert_eq!(xstate_bv(&a).unwrap(), bv);
        }
    }

    #[test]
    fn extended_components_follow_the_table() {
        let avx = XsaveComponent {
            bit: 2,
            offset: 576,
            size: 256,
        };
        // AVX clear: its area zeroes (everything non-allowlisted does).
        let mut a = area_with(0b011, 0xEE);
        canonicalize(&mut a, &[avx]).unwrap();
        assert!(a[576..832].iter().all(|&b| b == 0));
        assert!(a[832..].iter().all(|&b| b == 0)); // tail: no component, zero

        // AVX set: area survives; the gap past it is still zeroed.
        let mut a = area_with(0b111, 0xEE);
        canonicalize(&mut a, &[avx]).unwrap();
        assert!(a[576..832].iter().all(|&b| b == 0xEE));
        assert!(a[832..].iter().all(|&b| b == 0));
    }

    #[test]
    fn compacted_format_is_rejected() {
        let mut a = area_with(0, 0);
        a[520..528].copy_from_slice(&(1u64 << 63).to_le_bytes());
        assert_eq!(canonicalize(&mut a, &[]), Err(XsaveError::CompactedFormat));
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
        // OOB components are loud whether their bit is clear or SET.
        assert_eq!(
            canonicalize(&mut area_with(0, 0), &[oob]),
            Err(XsaveError::ComponentOutOfBounds { bit: 9 })
        );
        assert_eq!(
            canonicalize(&mut area_with(1 << 9, 0), &[oob]),
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
