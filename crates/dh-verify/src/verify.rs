//! The VerifyReplay reporting model (bead 1py; INTEGRATION §3/§4 and
//! proto §2.7). dh-verify owns the EVENT SHAPES — the execution lives in
//! dh-worker's `verify_replay` (ARCH §1: nothing depends on dh-worker,
//! so the executor imports this model, never the reverse).
//!
//! Phase-1 scope: epoch-grained verdicts only. `Divergence` reports the
//! FIRST bad epoch and the hash pair. M8 bisection is represented as an
//! explicit, evidence-carrying event so callers cannot launder coarse
//! epoch mismatches into fake `icount_lo..hi` diagnostics.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BisectionMode {
    /// Fresh replays of the same segment disagree; no recorded ground truth
    /// is needed to prove replay instability.
    ReplayVsReplay,
    /// Replay is stable but disagrees with a recorded checkpoint artifact.
    ReplayVsRecorded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BisectionEvidence {
    pub mode: BisectionMode,
    /// Recorded checkpoint snapshot ref used as expected state. Required
    /// for replay-vs-recorded divergence; absent for replay-vs-replay.
    pub expected_checkpoint_ref: Option<[u8; 32]>,
    /// Snapshot ref captured from the replay probe, when one was taken.
    pub actual_probe_ref: Option<[u8; 32]>,
    /// The artifact coverage that justifies `icount_lo..icount_hi`.
    pub coverage_icount_lo: u64,
    pub coverage_icount_hi: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BisectionDivergence {
    pub first_bad_epoch: Option<u64>,
    pub icount_lo: u64,
    pub icount_hi: u64,
    pub rip_expected: u64,
    pub rip_actual: u64,
    /// Postcard-encoded `Vec<RegDiff>` per proto/API. The schema lives in
    /// API.md until the runtime encoder lands.
    pub reg_diff: Vec<u8>,
    /// First differing flattened logical guest page indices.
    pub diff_page_idx: Vec<u64>,
    pub suspected_cause: String,
    pub evidence: BisectionEvidence,
}

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
    /// Terminal mismatch refined by recorded checkpoint/probe evidence.
    BisectionDivergence(BisectionDivergence),
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
        self.events.iter().find(|e| {
            matches!(
                e,
                VerifyProgress::Divergence { .. } | VerifyProgress::BisectionDivergence(_)
            )
        })
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

        let refined = VerifyProgress::BisectionDivergence(BisectionDivergence {
            first_bad_epoch: Some(2),
            icount_lo: 59_392,
            icount_hi: 60_000,
            rip_expected: 0x1000,
            rip_actual: 0x1004,
            reg_diff: vec![1, 2, 3],
            diff_page_idx: vec![7],
            suspected_cause: "recorded checkpoint mismatch".into(),
            evidence: BisectionEvidence {
                mode: BisectionMode::ReplayVsRecorded,
                expected_checkpoint_ref: Some([0xAA; 32]),
                actual_probe_ref: Some([0xBB; 32]),
                coverage_icount_lo: 59_392,
                coverage_icount_hi: 60_000,
            },
        });
        let mut report = VerifyReport::default();
        report.push(refined);
        assert!(!report.verified());
        assert!(report.divergence().is_some());
    }
}
