//! Virtual time (ARCH §4): vns is a pure function of icount through a fixed
//! rational clock. Frames are not time — nothing here (or anywhere) advances
//! vns by a frame period.

/// The per-VM clock rational, fixed for the VM's lifetime and recorded in
/// every DHILOG header. Default 1:1 — one virtual nanosecond per retired
/// instruction ("deterministic 1 GHz").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockRatio {
    num: u64,
    den: u64,
}

impl Default for ClockRatio {
    fn default() -> Self {
        Self { num: 1, den: 1 }
    }
}

impl ClockRatio {
    /// Both terms must be nonzero; the ratio is otherwise unconstrained.
    pub fn new(num: u64, den: u64) -> Option<Self> {
        (num != 0 && den != 0).then_some(Self { num, den })
    }

    pub fn num(self) -> u64 {
        self.num
    }

    pub fn den(self) -> u64 {
        self.den
    }

    /// `vns = icount * num / den`, u128 intermediate, truncating division
    /// (ARCH §4). The u128 product cannot overflow (u64×u64 < u128); `None`
    /// means the *result* exceeds u64 — a fault for the caller to surface
    /// loudly, never to saturate silently.
    pub fn vns_from_icount(self, icount: u64) -> Option<u64> {
        let vns = u128::from(icount) * u128::from(self.num) / u128::from(self.den);
        vns.try_into().ok()
    }

    /// Timer conversion (ARCH §4): `T → target icount = ceil(T * den / num)`.
    /// The result is the smallest icount whose vns reaches `target_vns`,
    /// which is what makes guest-armed timers deterministic agenda points.
    pub fn icount_for_vns_target(self, target_vns: u64) -> Option<u64> {
        let num = u128::from(self.num);
        let icount = (u128::from(target_vns) * u128::from(self.den)).div_ceil(num);
        icount.try_into().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Property tests run over a seeded xorshift stream: deterministic, no
    // dev-dependency, and the failing case prints in the assert message.
    struct XorShift(u64);
    impl XorShift {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
    }

    const CASES: usize = 20_000;

    #[test]
    fn default_is_one_to_one() {
        let r = ClockRatio::default();
        assert_eq!((r.num(), r.den()), (1, 1));
        assert_eq!(r.vns_from_icount(u64::MAX), Some(u64::MAX));
        assert_eq!(r.icount_for_vns_target(u64::MAX), Some(u64::MAX));
    }

    #[test]
    fn zero_terms_rejected() {
        assert_eq!(ClockRatio::new(0, 1), None);
        assert_eq!(ClockRatio::new(1, 0), None);
        assert_eq!(ClockRatio::new(0, 0), None);
    }

    #[test]
    fn truncation_matches_spec_examples() {
        let third = ClockRatio::new(1, 3).unwrap();
        assert_eq!(third.vns_from_icount(8), Some(2)); // 8/3 truncates
        assert_eq!(third.icount_for_vns_target(2), Some(6)); // exact
        assert_eq!(third.icount_for_vns_target(3), Some(9)); // exact
        let two_thirds = ClockRatio::new(2, 3).unwrap();
        assert_eq!(two_thirds.icount_for_vns_target(1), Some(2)); // ceil(3/2)
    }

    #[test]
    fn vns_overflow_is_reported_not_saturated() {
        let doubling = ClockRatio::new(2, 1).unwrap();
        assert_eq!(doubling.vns_from_icount(u64::MAX), None);
        assert_eq!(doubling.vns_from_icount(u64::MAX / 2), Some(u64::MAX - 1));
        let halving = ClockRatio::new(1, 2).unwrap();
        assert_eq!(halving.icount_for_vns_target(u64::MAX), None);
    }

    #[test]
    fn prop_monotone_in_icount() {
        let mut rng = XorShift(0x9E3779B97F4A7C15);
        for _ in 0..CASES {
            let r = ClockRatio::new(1 + rng.next() % 1000, 1 + rng.next() % 1000).unwrap();
            let i = rng.next();
            let (Some(a), Some(b)) = (r.vns_from_icount(i), r.vns_from_icount(i.saturating_add(1)))
            else {
                continue; // result overflow: covered by the overflow test
            };
            assert!(a <= b, "vns not monotone: ratio={r:?} icount={i}");
        }
    }

    #[test]
    fn prop_timer_target_is_minimal_reaching_icount() {
        let mut rng = XorShift(0xD1B54A32D192ED03);
        for _ in 0..CASES {
            let r = ClockRatio::new(1 + rng.next() % 1000, 1 + rng.next() % 1000).unwrap();
            let t = rng.next() % (1 << 48);
            let Some(j) = r.icount_for_vns_target(t) else {
                continue;
            };
            let reached = r
                .vns_from_icount(j)
                .expect("vns(j) overflows only if t did");
            assert!(
                reached >= t,
                "vns(target icount) < T: ratio={r:?} T={t} j={j}"
            );
            if j > 0 {
                let before = r.vns_from_icount(j - 1).unwrap();
                assert!(
                    before < t,
                    "target icount not minimal: ratio={r:?} T={t} j={j}"
                );
            }
        }
    }

    #[test]
    fn prop_roundtrip_identity_on_exact_boundaries() {
        // For any icount i, converting its vns back yields an icount <= i
        // (the timer conversion lands on or before the instruction that
        // produced that vns), and exactly i when the division was exact.
        let mut rng = XorShift(0xA0761D6478BD642F);
        for _ in 0..CASES {
            let r = ClockRatio::new(1 + rng.next() % 1000, 1 + rng.next() % 1000).unwrap();
            let i = rng.next() % (1 << 48);
            let vns = r.vns_from_icount(i).unwrap();
            let Some(back) = r.icount_for_vns_target(vns) else {
                continue;
            };
            assert!(
                back <= i,
                "roundtrip overshot: ratio={r:?} i={i} back={back}"
            );
            if (u128::from(i) * u128::from(r.num())) % u128::from(r.den()) == 0 {
                assert_eq!(back, i, "exact boundary must roundtrip: ratio={r:?} i={i}");
            }
        }
    }
}
