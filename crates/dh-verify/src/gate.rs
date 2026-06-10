//! The Phase-1 determinism gate harness (bead ksx; phase doc gate 1):
//! N consecutive runs, ZERO divergence. Generic over the run's outcome
//! fingerprint — dh-cli's `gate` subcommand and the tests/determinism
//! suite drive it with VM machinery; this crate stays pure.
//!
//! HONESTY NOTE: N runs in one process sample within-boot variation
//! (PMI timing, scheduler interference, cache state) — cross-host-boot
//! and cross-kernel divergence is the dedicated runner's long-baseline
//! job, not this gate's.

/// One gate execution: every run's fingerprint, and the first divergence
/// if any. The REPORT (artifact) lists all fingerprints so a failure is
/// diagnosable without a re-run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GateReport {
    pub name: String,
    pub fingerprints: Vec<String>,
    /// Index of the first run that diverged from run 0.
    pub first_divergence: Option<usize>,
}

impl GateReport {
    pub fn passed(&self) -> bool {
        self.first_divergence.is_none() && !self.fingerprints.is_empty()
    }

    /// Plain-text artifact: name, verdict, one fingerprint per run.
    pub fn artifact(&self) -> String {
        let mut out = format!(
            "gate {} runs={} verdict={}\n",
            self.name,
            self.fingerprints.len(),
            match self.first_divergence {
                None if !self.fingerprints.is_empty() => "PASS".to_string(),
                None => "EMPTY (FAIL)".to_string(),
                Some(i) => format!("DIVERGED at run {i}"),
            }
        );
        for (i, f) in self.fingerprints.iter().enumerate() {
            out.push_str(&format!("run {i}: {f}\n"));
        }
        out
    }
}

/// Run `run` N times; every fingerprint must equal run 0's. Stops at the
/// FIRST divergence (the report still carries everything collected). A
/// run error is a gate failure, not a panic.
pub fn zero_divergence(
    name: &str,
    runs: usize,
    mut run: impl FnMut(usize) -> Result<String, String>,
) -> Result<GateReport, String> {
    let mut fingerprints = Vec::with_capacity(runs);
    let mut first_divergence = None;
    for i in 0..runs {
        let f = run(i).map_err(|e| format!("gate {name}: run {i} failed: {e}"))?;
        if i > 0 && f != fingerprints[0] {
            fingerprints.push(f);
            first_divergence = Some(i);
            break;
        }
        fingerprints.push(f);
    }
    Ok(GateReport {
        name: name.to_string(),
        fingerprints,
        first_divergence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_on_identical_fingerprints() {
        let r = zero_divergence("t", 5, |_| Ok("abc".into())).unwrap();
        assert!(r.passed());
        assert_eq!(r.fingerprints.len(), 5);
        assert!(r.artifact().contains("verdict=PASS"));
    }

    #[test]
    fn stops_and_reports_first_divergence() {
        let r = zero_divergence("t", 10, |i| Ok(if i == 3 { "x" } else { "abc" }.into())).unwrap();
        assert!(!r.passed());
        assert_eq!(r.first_divergence, Some(3));
        assert_eq!(r.fingerprints.len(), 4, "stops at the divergence");
        assert!(r.artifact().contains("DIVERGED at run 3"));
    }

    #[test]
    fn run_errors_fail_the_gate_and_empty_fails() {
        assert!(zero_divergence("t", 3, |i| {
            if i == 1 {
                Err("boom".into())
            } else {
                Ok("a".into())
            }
        })
        .is_err());
        let r = zero_divergence("t", 0, |_| Ok("a".into())).unwrap();
        assert!(!r.passed(), "zero runs is not a pass");
    }
}
