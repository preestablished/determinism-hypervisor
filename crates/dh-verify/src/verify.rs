//! The VerifyReplay reporting model (bead 1py; INTEGRATION §3/§4 and
//! proto §2.7). dh-verify owns the EVENT SHAPES — the execution lives in
//! dh-worker's `verify_replay` (ARCH §1: nothing depends on dh-worker,
//! so the executor imports this model, never the reverse).
//!
//! Phase-1 scope: epoch-grained verdicts only. `Divergence` reports the
//! FIRST bad epoch and the hash pair; the icount-range bisection that
//! narrows it to ≤1024 instructions is M8 (proto fields exist, the
//! refinement does not).

/// One verification event. `EpochOk`/`Done` mirror proto §2.7's
/// `EpochOk`/`VerifyDone` field-for-field. `Divergence` mirrors the
/// 1py bead's `Divergence{first_divergent_epoch, hashes}` — the PROTO
/// Divergence instead carries the M8 bisection fields (icount range,
/// rip pair) that do not exist yet; the M6 RPC (rfv) owns that mapping.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerifyProgress {
    /// An EPOCH_HASH record matched the live chain (proto `EpochOk`).
    EpochOk { epoch_index: u64, icount: u64 },
    /// Full match through END (proto `VerifyDone`).
    Done {
        total_icount: u64,
        end_state_hash: [u8; 32],
    },
    /// Terminal mismatch (P0 by convention).
    Divergence {
        /// `Some(n)` only when an EPOCH link itself diverged — the
        /// first bad epoch. `None` means every epoch matched and the
        /// divergence is in the END identity (end_state_hash/end_vns)
        /// or the resealed bytes — naming an epoch there would blame
        /// one that VERIFIED (iteration-89 review I1).
        first_bad_epoch: Option<u64>,
        /// For epoch/END-hash kinds: the diverging position in icount
        /// space. For "resealed log bytes": the first differing BYTE
        /// OFFSET (the engine's report shape, documented there).
        at_icount: u64,
        what: &'static str,
        /// Hash pair for hash kinds; for "end_vns" these carry the two
        /// u64s LE-packed in the first 8 bytes (the engine's shape).
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

    /// Verified end-to-end: a `Done` was reported and no divergence —
    /// order-independent (iteration-89 review I3: last-event semantics
    /// would flip on any post-Done event a future caller appends).
    pub fn verified(&self) -> bool {
        self.done().is_some() && self.divergence().is_none()
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
            first_bad_epoch: Some(2),
            at_icount: 60_000,
            what: "EPOCH_HASH chain value",
            expected: [1; 32],
            got: [2; 32],
        });
        assert!(!bad.verified());
        assert!(bad.divergence().is_some());
    }
}
