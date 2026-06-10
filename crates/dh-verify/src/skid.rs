//! PMI skid histogram (bead 19l; ARCH §9 "PMI skid histogram").
//!
//! Collection lives here (named home per ARCH §1); the measurement driver
//! is dh-cli's `skid` subcommand (it owns the VM machinery). The exit-gate
//! rule (risk R1 alert threshold): the measured MAX skid on the box must
//! stay under skid_margin / 2 — otherwise raise the margin and
//! re-baseline before trusting any landing.

use std::collections::BTreeMap;

/// Skid-sample distribution: skid (instructions past the armed point) →
/// occurrence count. BTreeMap keeps every export deterministic.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SkidHistogram {
    buckets: BTreeMap<u64, u64>,
    samples: u64,
    sum: u128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarginViolation {
    pub max_skid: u64,
    pub skid_margin: u64,
}

impl std::fmt::Display for MarginViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "R1 ALERT: measured max skid {} >= skid_margin/2 ({} / 2 = {}) — \
             raise the margin and re-baseline",
            self.max_skid,
            self.skid_margin,
            self.skid_margin / 2
        )
    }
}

impl SkidHistogram {
    pub fn record(&mut self, skid: u64) {
        *self.buckets.entry(skid).or_insert(0) += 1;
        self.samples += 1;
        self.sum += u128::from(skid);
    }

    pub fn samples(&self) -> u64 {
        self.samples
    }

    pub fn max(&self) -> Option<u64> {
        self.buckets.keys().next_back().copied()
    }

    pub fn min(&self) -> Option<u64> {
        self.buckets.keys().next().copied()
    }

    /// The exit-gate assertion: max skid strictly under skid_margin / 2.
    /// An EMPTY histogram fails (no data is not a pass).
    pub fn assert_margin(&self, skid_margin: u64) -> Result<(), MarginViolation> {
        match self.max() {
            Some(max) if max < skid_margin / 2 => Ok(()),
            Some(max) => Err(MarginViolation {
                max_skid: max,
                skid_margin,
            }),
            None => Err(MarginViolation {
                max_skid: u64::MAX,
                skid_margin,
            }),
        }
    }

    /// Plain-text artifact: one `skid count` line per bucket, then a
    /// summary line. Deterministic ordering.
    pub fn artifact(&self) -> String {
        let mut out = String::new();
        for (skid, count) in &self.buckets {
            out.push_str(&format!("{skid} {count}\n"));
        }
        out.push_str(&format!(
            "# samples={} min={} max={} sum={}\n",
            self.samples,
            self.min().unwrap_or(0),
            self.max().unwrap_or(0),
            self.sum
        ));
        out
    }

    /// Prometheus-ready series (ARCH §9): cumulative histogram buckets
    /// plus _sum and _count, exposition text format.
    pub fn prometheus(&self, name: &str) -> String {
        let mut out = String::new();
        out.push_str(&format!("# TYPE {name} histogram\n"));
        let mut cumulative = 0u64;
        for (skid, count) in &self.buckets {
            cumulative += count;
            out.push_str(&format!("{name}_bucket{{le=\"{skid}\"}} {cumulative}\n"));
        }
        out.push_str(&format!("{name}_bucket{{le=\"+Inf\"}} {}\n", self.samples));
        out.push_str(&format!("{name}_sum {}\n", self.sum));
        out.push_str(&format!("{name}_count {}\n", self.samples));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_summarizes() {
        let mut h = SkidHistogram::default();
        for s in [18, 18, 18, 20, 17] {
            h.record(s);
        }
        assert_eq!(h.samples(), 5);
        assert_eq!(h.min(), Some(17));
        assert_eq!(h.max(), Some(20));
    }

    #[test]
    fn margin_gate_is_strict_and_empty_fails() {
        let mut h = SkidHistogram::default();
        h.record(4095);
        assert!(h.assert_margin(8192).is_ok());
        h.record(4096); // == margin/2: NOT under it
        assert!(h.assert_margin(8192).is_err());
        assert!(SkidHistogram::default().assert_margin(8192).is_err());
    }

    #[test]
    fn exports_are_deterministic_and_cumulative() {
        let mut h = SkidHistogram::default();
        for s in [20, 18, 18] {
            h.record(s);
        }
        assert_eq!(
            h.artifact(),
            "18 2\n20 1\n# samples=3 min=18 max=20 sum=56\n"
        );
        let p = h.prometheus("dh_pmi_skid_instructions");
        assert!(p.contains("dh_pmi_skid_instructions_bucket{le=\"18\"} 2\n"));
        assert!(p.contains("dh_pmi_skid_instructions_bucket{le=\"20\"} 3\n"));
        assert!(p.contains("dh_pmi_skid_instructions_bucket{le=\"+Inf\"} 3\n"));
        assert!(p.contains("dh_pmi_skid_instructions_sum 56\n"));
        assert!(p.contains("dh_pmi_skid_instructions_count 3\n"));
    }
}
