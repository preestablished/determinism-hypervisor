//! The VerifyReplay reporting model (bead 1py; INTEGRATION §3/§4 and
//! proto §2.7). dh-verify owns the EVENT SHAPES — the execution lives in
//! dh-worker's `verify_replay` (ARCH §1: nothing depends on dh-worker,
//! so the executor imports this model, never the reverse).
//!
//! Phase-1 scope: epoch-grained verdicts only. `Divergence` reports the
//! FIRST bad epoch and the hash pair; the icount-range bisection that
//! narrows it to ≤1024 instructions is M8 (proto fields exist, the
//! refinement does not).

/// One verification event — mirrors proto `VerifyReplayProgress`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerifyProgress {
    /// An EPOCH_HASH record matched the live chain (proto `EpochOk`).
    EpochOk { epoch_index: u64, icount: u64 },
    /// Full match through END (proto `VerifyDone`).
    Done {
        total_icount: u64,
        end_state_hash: [u8; 32],
    },
    /// Terminal mismatch (proto `Divergence`, P0 by convention). The
    /// bisection fields (icount_lo/hi, rip pair) arrive with M8.
    Divergence {
        first_bad_epoch: u64,
        at_icount: u64,
        what: &'static str,
        expected: [u8; 32],
        got: [u8; 32],
    },
}

/// A collected verification run (the gate.rs-style library harness).
#[derive(Clone, Debug, Default)]
pub struct VerifyReport {
    pub events: Vec<VerifyProgress>,
}

impl VerifyReport {
    pub fn push(&mut self, e: VerifyProgress) {
        self.events.push(e);
    }

    /// Verified end-to-end: ends with `Done` and carries no divergence.
    pub fn verified(&self) -> bool {
        matches!(self.events.last(), Some(VerifyProgress::Done { .. }))
            && self.divergence().is_none()
    }

    pub fn done(&self) -> Option<(u64, [u8; 32])> {
        self.events.iter().find_map(|e| match e {
            VerifyProgress::Done {
                total_icount,
                end_state_hash,
            } => Some((*total_icount, *end_state_hash)),
            _ => None,
        })
    }

    pub fn divergence(&self) -> Option<&VerifyProgress> {
        self.events
            .iter()
            .find(|e| matches!(e, VerifyProgress::Divergence { .. }))
    }

    pub fn epochs_ok(&self) -> usize {
        self.events
            .iter()
            .filter(|e| matches!(e, VerifyProgress::EpochOk { .. }))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_semantics() {
        let mut r = VerifyReport::default();
        r.push(VerifyProgress::EpochOk {
            epoch_index: 1,
            icount: 30_000,
        });
        assert!(!r.verified(), "no Done yet");
        r.push(VerifyProgress::Done {
            total_icount: 100_000,
            end_state_hash: [7; 32],
        });
        assert!(r.verified());
        assert_eq!(r.epochs_ok(), 1);
        assert_eq!(r.done(), Some((100_000, [7; 32])));

        let mut bad = VerifyReport::default();
        bad.push(VerifyProgress::Divergence {
            first_bad_epoch: 2,
            at_icount: 60_000,
            what: "EPOCH_HASH chain value",
            expected: [1; 32],
            got: [2; 32],
        });
        assert!(!bad.verified());
        assert!(bad.divergence().is_some());
    }
}
