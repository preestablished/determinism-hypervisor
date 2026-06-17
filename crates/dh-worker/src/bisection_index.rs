//! VerifyReplay-side BISECTION_CHECKPOINT indexing.
//!
//! The DHILOG reader validates each checkpoint payload in isolation. This
//! module validates the cross-record facts needed for honest bisection:
//! checkpoint records are ordered by `(icount, seq)`, each follows the epoch
//! hash whose boundary it captures, and each advertised coverage gap is at
//! least as wide as the spacing since the previous checkpoint.

use dh_inputlog::dhilog::{BISECTION_CHECKPOINT_FLAGS, BISECTION_CHECKPOINT_FORMAT_VERSION};
use dh_inputlog::reader::{LogReader, RecordBody};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RecordPosition {
    pub icount: u64,
    pub seq: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndexedEpochHash {
    pub epoch_index: u64,
    pub position: RecordPosition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndexedBisectionCheckpoint {
    pub position: RecordPosition,
    pub checkpoint_icount: u64,
    pub checkpoint_vns: u64,
    pub checkpoint_snapshot_ref: [u8; 32],
    pub max_covered_gap: u32,
    pub preceding_epoch_hash: IndexedEpochHash,
}

impl IndexedBisectionCheckpoint {
    pub fn coverage_icount_lo(&self) -> u64 {
        self.checkpoint_icount
            .saturating_sub(u64::from(self.max_covered_gap))
    }

    pub fn coverage_icount_hi(&self) -> u64 {
        self.checkpoint_icount
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BisectionDivergenceSite {
    EpochHash {
        epoch_index: u64,
        position: RecordPosition,
    },
    Terminal {
        position: RecordPosition,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BisectionSelectionTarget {
    EpochHash { epoch_index: u64, at_icount: u64 },
    TerminalEndState { end_icount: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectedBisectionCheckpoint {
    pub checkpoint: IndexedBisectionCheckpoint,
    pub divergence: BisectionDivergenceSite,
    pub coverage_icount_lo: u64,
    pub coverage_icount_hi: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BisectionCheckpointIndex {
    checkpoints: Vec<IndexedBisectionCheckpoint>,
    epoch_hashes: Vec<IndexedEpochHash>,
    end_position: Option<RecordPosition>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BisectionCheckpointIndexError {
    UnsupportedFormatVersion {
        seq: u32,
        found: u16,
    },
    InvalidFlags {
        seq: u32,
        flags: u16,
    },
    IcountMismatch {
        seq: u32,
        record_icount: u64,
        checkpoint_icount: u64,
    },
    MissingPrecedingEpochHash {
        checkpoint: RecordPosition,
    },
    CheckpointDoesNotFollowEpochHash {
        checkpoint: RecordPosition,
        epoch_hash: RecordPosition,
    },
    CheckpointSeparatedFromEpochHashByCanonicalRecord {
        checkpoint: RecordPosition,
        epoch_hash: RecordPosition,
        canonical: RecordPosition,
    },
    InconsistentSequenceOrdering {
        previous: RecordPosition,
        current: RecordPosition,
    },
    GapTooNarrow {
        seq: u32,
        checkpoint_icount: u64,
        previous_checkpoint_icount: Option<u64>,
        max_covered_gap: u32,
        required_gap: u64,
    },
    UnusableSnapshotRef {
        seq: u32,
        checkpoint_snapshot_ref: [u8; 32],
        reason: String,
    },
}

impl fmt::Display for BisectionCheckpointIndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFormatVersion { seq, found } => {
                write!(
                    f,
                    "BISECTION_CHECKPOINT seq {seq} has unsupported format version {found}"
                )
            }
            Self::InvalidFlags { seq, flags } => {
                write!(
                    f,
                    "BISECTION_CHECKPOINT seq {seq} has invalid flags 0x{flags:04x}"
                )
            }
            Self::IcountMismatch {
                seq,
                record_icount,
                checkpoint_icount,
            } => {
                write!(
                    f,
                    "BISECTION_CHECKPOINT seq {seq} icount mismatch: record {record_icount}, payload {checkpoint_icount}"
                )
            }
            Self::MissingPrecedingEpochHash { checkpoint } => {
                write!(
                    f,
                    "BISECTION_CHECKPOINT seq {} at icount {} does not follow an EPOCH_HASH at the same icount",
                    checkpoint.seq, checkpoint.icount
                )
            }
            Self::CheckpointDoesNotFollowEpochHash {
                checkpoint,
                epoch_hash,
            } => {
                write!(
                    f,
                    "BISECTION_CHECKPOINT seq {} at icount {} must follow EPOCH_HASH seq {} at the same icount",
                    checkpoint.seq, checkpoint.icount, epoch_hash.seq
                )
            }
            Self::CheckpointSeparatedFromEpochHashByCanonicalRecord {
                checkpoint,
                epoch_hash,
                canonical,
            } => {
                write!(
                    f,
                    "BISECTION_CHECKPOINT seq {} at icount {} is separated from EPOCH_HASH seq {} by canonical record seq {}",
                    checkpoint.seq, checkpoint.icount, epoch_hash.seq, canonical.seq
                )
            }
            Self::InconsistentSequenceOrdering { previous, current } => {
                write!(
                    f,
                    "BISECTION_CHECKPOINT position regressed from (icount {}, seq {}) to (icount {}, seq {})",
                    previous.icount, previous.seq, current.icount, current.seq
                )
            }
            Self::GapTooNarrow {
                seq,
                checkpoint_icount,
                previous_checkpoint_icount,
                max_covered_gap,
                required_gap,
            } => {
                let previous = previous_checkpoint_icount
                    .map(|icount| icount.to_string())
                    .unwrap_or_else(|| "segment-start".into());
                write!(
                    f,
                    "BISECTION_CHECKPOINT seq {seq} at icount {checkpoint_icount} advertises max_covered_gap {max_covered_gap}, but spacing from {previous} requires {required_gap}"
                )
            }
            Self::UnusableSnapshotRef {
                seq,
                checkpoint_snapshot_ref,
                reason,
            } => {
                write!(
                    f,
                    "BISECTION_CHECKPOINT seq {seq} snapshot ref {} is unusable: {reason}",
                    hex32(checkpoint_snapshot_ref)
                )
            }
        }
    }
}

impl std::error::Error for BisectionCheckpointIndexError {}

impl BisectionCheckpointIndex {
    pub fn from_reader(reader: &LogReader<'_>) -> Result<Self, BisectionCheckpointIndexError> {
        let mut epoch_hash_by_icount: BTreeMap<u64, EpochHashContext> = BTreeMap::new();
        let mut epoch_hashes = Vec::new();
        let mut candidates = Vec::new();
        let mut end_position = None;

        for record in reader.records() {
            let position = RecordPosition {
                icount: record.icount(),
                seq: record.seq(),
            };
            match record.body() {
                RecordBody::EpochHash { epoch_index, .. } => {
                    let epoch_hash = IndexedEpochHash {
                        epoch_index,
                        position,
                    };
                    epoch_hash_by_icount.insert(
                        position.icount,
                        EpochHashContext {
                            epoch_hash,
                            intervening_canonical: None,
                        },
                    );
                    epoch_hashes.push(epoch_hash);
                }
                RecordBody::BisectionCheckpoint {
                    format_version,
                    flags,
                    max_covered_gap,
                    checkpoint_snapshot_ref,
                    checkpoint_icount,
                    checkpoint_vns,
                } => {
                    let context = epoch_hash_by_icount.get(&position.icount).copied();
                    candidates.push(BisectionCheckpointCandidate {
                        position,
                        format_version,
                        flags,
                        max_covered_gap,
                        checkpoint_snapshot_ref,
                        checkpoint_icount,
                        checkpoint_vns,
                        preceding_epoch_hash: context.map(|context| context.epoch_hash),
                        intervening_canonical: context
                            .and_then(|context| context.intervening_canonical),
                    })
                }
                RecordBody::End { .. } => end_position = Some(position),
                _ if !record.is_aux() => {
                    if let Some(context) = epoch_hash_by_icount.get_mut(&position.icount) {
                        if context.epoch_hash.position < position
                            && context.intervening_canonical.is_none()
                        {
                            context.intervening_canonical = Some(position);
                        }
                    }
                }
                _ => {}
            }
        }

        Self::from_candidates(epoch_hashes, end_position, candidates)
    }

    pub fn checkpoints(&self) -> &[IndexedBisectionCheckpoint] {
        &self.checkpoints
    }

    pub fn epoch_hashes(&self) -> &[IndexedEpochHash] {
        &self.epoch_hashes
    }

    pub fn is_empty(&self) -> bool {
        self.checkpoints.is_empty()
    }

    pub fn validate_snapshot_refs<F, E>(
        &self,
        mut validate: F,
    ) -> Result<(), BisectionCheckpointIndexError>
    where
        F: FnMut([u8; 32]) -> Result<(), E>,
        E: fmt::Display,
    {
        for checkpoint in &self.checkpoints {
            validate(checkpoint.checkpoint_snapshot_ref).map_err(|e| {
                BisectionCheckpointIndexError::UnusableSnapshotRef {
                    seq: checkpoint.position.seq,
                    checkpoint_snapshot_ref: checkpoint.checkpoint_snapshot_ref,
                    reason: e.to_string(),
                }
            })?;
        }
        Ok(())
    }

    pub fn select_for_divergence(
        &self,
        target: BisectionSelectionTarget,
    ) -> Option<SelectedBisectionCheckpoint> {
        match target {
            BisectionSelectionTarget::EpochHash {
                epoch_index,
                at_icount,
            } => self.select_epoch_hash_divergence(epoch_index, at_icount),
            BisectionSelectionTarget::TerminalEndState { end_icount } => {
                self.select_terminal_divergence(end_icount)
            }
        }
    }

    pub fn select_epoch_hash_divergence(
        &self,
        epoch_index: u64,
        at_icount: u64,
    ) -> Option<SelectedBisectionCheckpoint> {
        let epoch_hash = self
            .epoch_hashes
            .iter()
            .find(|epoch| epoch.epoch_index == epoch_index && epoch.position.icount == at_icount)?;

        self.checkpoints
            .iter()
            .copied()
            .find(|checkpoint| checkpoint.covers_epoch_position(epoch_hash.position))
            .map(|checkpoint| SelectedBisectionCheckpoint {
                checkpoint,
                divergence: BisectionDivergenceSite::EpochHash {
                    epoch_index,
                    position: epoch_hash.position,
                },
                coverage_icount_lo: checkpoint.coverage_icount_lo(),
                coverage_icount_hi: checkpoint.coverage_icount_hi(),
            })
    }

    pub fn select_terminal_divergence(
        &self,
        end_icount: u64,
    ) -> Option<SelectedBisectionCheckpoint> {
        let end_position = self.end_position?;
        if end_position.icount != end_icount {
            return None;
        }

        self.checkpoints
            .iter()
            .rev()
            .copied()
            .find(|checkpoint| {
                checkpoint.position < end_position && checkpoint.checkpoint_icount <= end_icount
            })
            .map(|checkpoint| SelectedBisectionCheckpoint {
                checkpoint,
                divergence: BisectionDivergenceSite::Terminal {
                    position: end_position,
                },
                coverage_icount_lo: checkpoint.coverage_icount_lo(),
                coverage_icount_hi: end_icount,
            })
    }

    fn from_candidates(
        epoch_hashes: Vec<IndexedEpochHash>,
        end_position: Option<RecordPosition>,
        candidates: Vec<BisectionCheckpointCandidate>,
    ) -> Result<Self, BisectionCheckpointIndexError> {
        let mut checkpoints = Vec::with_capacity(candidates.len());
        let mut previous_position = None;
        let mut previous_checkpoint_icount = None;

        for candidate in candidates {
            if candidate.format_version != BISECTION_CHECKPOINT_FORMAT_VERSION {
                return Err(BisectionCheckpointIndexError::UnsupportedFormatVersion {
                    seq: candidate.position.seq,
                    found: candidate.format_version,
                });
            }
            if candidate.flags != BISECTION_CHECKPOINT_FLAGS {
                return Err(BisectionCheckpointIndexError::InvalidFlags {
                    seq: candidate.position.seq,
                    flags: candidate.flags,
                });
            }
            if candidate.position.icount != candidate.checkpoint_icount {
                return Err(BisectionCheckpointIndexError::IcountMismatch {
                    seq: candidate.position.seq,
                    record_icount: candidate.position.icount,
                    checkpoint_icount: candidate.checkpoint_icount,
                });
            }
            if let Some(previous) = previous_position {
                if candidate.position <= previous {
                    return Err(
                        BisectionCheckpointIndexError::InconsistentSequenceOrdering {
                            previous,
                            current: candidate.position,
                        },
                    );
                }
            }
            let preceding_epoch_hash = candidate.preceding_epoch_hash.ok_or(
                BisectionCheckpointIndexError::MissingPrecedingEpochHash {
                    checkpoint: candidate.position,
                },
            )?;
            if preceding_epoch_hash.position.icount != candidate.position.icount
                || preceding_epoch_hash.position >= candidate.position
            {
                return Err(
                    BisectionCheckpointIndexError::CheckpointDoesNotFollowEpochHash {
                        checkpoint: candidate.position,
                        epoch_hash: preceding_epoch_hash.position,
                    },
                );
            }
            if let Some(canonical) = candidate.intervening_canonical {
                return Err(
                    BisectionCheckpointIndexError::CheckpointSeparatedFromEpochHashByCanonicalRecord {
                        checkpoint: candidate.position,
                        epoch_hash: preceding_epoch_hash.position,
                        canonical,
                    },
                );
            }
            let required_gap = if let Some(previous_icount) = previous_checkpoint_icount {
                candidate
                    .checkpoint_icount
                    .checked_sub(previous_icount)
                    .ok_or(
                        BisectionCheckpointIndexError::InconsistentSequenceOrdering {
                            previous: previous_position.expect("previous icount implies position"),
                            current: candidate.position,
                        },
                    )?
            } else {
                candidate.checkpoint_icount
            };
            if u64::from(candidate.max_covered_gap) < required_gap {
                return Err(BisectionCheckpointIndexError::GapTooNarrow {
                    seq: candidate.position.seq,
                    checkpoint_icount: candidate.checkpoint_icount,
                    previous_checkpoint_icount,
                    max_covered_gap: candidate.max_covered_gap,
                    required_gap,
                });
            }

            checkpoints.push(IndexedBisectionCheckpoint {
                position: candidate.position,
                checkpoint_icount: candidate.checkpoint_icount,
                checkpoint_vns: candidate.checkpoint_vns,
                checkpoint_snapshot_ref: candidate.checkpoint_snapshot_ref,
                max_covered_gap: candidate.max_covered_gap,
                preceding_epoch_hash,
            });
            previous_position = Some(candidate.position);
            previous_checkpoint_icount = Some(candidate.checkpoint_icount);
        }

        Ok(Self {
            checkpoints,
            epoch_hashes,
            end_position,
        })
    }
}

impl IndexedBisectionCheckpoint {
    fn covers_epoch_position(&self, epoch_position: RecordPosition) -> bool {
        self.coverage_icount_lo() <= epoch_position.icount
            && (epoch_position.icount < self.checkpoint_icount
                || (epoch_position.icount == self.checkpoint_icount
                    && epoch_position < self.position))
    }
}

#[derive(Clone, Copy, Debug)]
struct EpochHashContext {
    epoch_hash: IndexedEpochHash,
    intervening_canonical: Option<RecordPosition>,
}

#[derive(Clone, Copy, Debug)]
struct BisectionCheckpointCandidate {
    position: RecordPosition,
    format_version: u16,
    flags: u16,
    max_covered_gap: u32,
    checkpoint_snapshot_ref: [u8; 32],
    checkpoint_icount: u64,
    checkpoint_vns: u64,
    preceding_epoch_hash: Option<IndexedEpochHash>,
    intervening_canonical: Option<RecordPosition>,
}

fn hex32(bytes: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use dh_inputlog::dhilog::{LogWriter, SealParams, SegmentHeader, FRAME_HINT_NONE};

    fn pos(icount: u64, seq: u32) -> RecordPosition {
        RecordPosition { icount, seq }
    }

    fn epoch(epoch_index: u64, icount: u64, seq: u32) -> IndexedEpochHash {
        IndexedEpochHash {
            epoch_index,
            position: pos(icount, seq),
        }
    }

    fn candidate(
        seq: u32,
        icount: u64,
        max_covered_gap: u32,
        checkpoint_snapshot_ref: [u8; 32],
        checkpoint_vns: u64,
        preceding_epoch_hash: IndexedEpochHash,
    ) -> BisectionCheckpointCandidate {
        BisectionCheckpointCandidate {
            position: pos(icount, seq),
            format_version: BISECTION_CHECKPOINT_FORMAT_VERSION,
            flags: BISECTION_CHECKPOINT_FLAGS,
            max_covered_gap,
            checkpoint_snapshot_ref,
            checkpoint_icount: icount,
            checkpoint_vns,
            preceding_epoch_hash: Some(preceding_epoch_hash),
            intervening_canonical: None,
        }
    }

    fn header() -> SegmentHeader {
        SegmentHeader {
            base_snapshot_id: [0x11; 32],
            entropy_seed: [0x22; 32],
            machine_config_hash: [0x33; 32],
            clock_num: 1,
            clock_den: 1,
            encoder_fingerprint: 0,
        }
    }

    fn seal_params(end_icount: u64) -> SealParams {
        SealParams {
            end_snapshot_id: [0; 32],
            end_icount,
            end_vns: end_icount,
            end_state_hash: [0x99; 32],
            stop_reason: 0,
        }
    }

    #[test]
    fn indexes_reader_checkpoints_and_preserves_positions() {
        let mut writer = LogWriter::new(header());
        writer.epoch_hash(20, 0x2000, 1, [0x01; 32]).unwrap();
        writer
            .bisection_checkpoint(20, 0x2000, 20, [0xA1; 32], 2_000)
            .unwrap();
        writer.epoch_hash(40, 0x4000, 2, [0x02; 32]).unwrap();
        writer
            .bisection_checkpoint(40, 0x4000, 20, [0xA2; 32], 4_000)
            .unwrap();
        let log = writer.seal(seal_params(45)).unwrap();
        let reader = LogReader::parse(&log).unwrap();

        let index = BisectionCheckpointIndex::from_reader(&reader).unwrap();

        assert_eq!(index.epoch_hashes().len(), 2);
        assert_eq!(index.checkpoints().len(), 2);
        assert_eq!(
            index.checkpoints()[0],
            IndexedBisectionCheckpoint {
                position: pos(20, 1),
                checkpoint_icount: 20,
                checkpoint_vns: 2_000,
                checkpoint_snapshot_ref: [0xA1; 32],
                max_covered_gap: 20,
                preceding_epoch_hash: epoch(1, 20, 0),
            }
        );
        assert_eq!(index.checkpoints()[1].position, pos(40, 3));
    }

    #[test]
    fn selects_epoch_and_terminal_evidence_windows() {
        let epochs = vec![epoch(1, 20, 0), epoch(2, 30, 2), epoch(3, 40, 4)];
        let index = BisectionCheckpointIndex::from_candidates(
            epochs.clone(),
            Some(pos(45, 6)),
            vec![
                candidate(1, 20, 20, [0xA1; 32], 2_000, epochs[0]),
                candidate(5, 40, 20, [0xA2; 32], 4_000, epochs[2]),
            ],
        )
        .unwrap();

        let epoch_selection = index.select_epoch_hash_divergence(2, 30).unwrap();
        assert_eq!(epoch_selection.checkpoint.position, pos(40, 5));
        assert_eq!(epoch_selection.coverage_icount_lo, 20);
        assert_eq!(epoch_selection.coverage_icount_hi, 40);
        assert_eq!(
            epoch_selection.divergence,
            BisectionDivergenceSite::EpochHash {
                epoch_index: 2,
                position: pos(30, 2),
            }
        );
        assert_eq!(
            index
                .select_for_divergence(BisectionSelectionTarget::EpochHash {
                    epoch_index: 2,
                    at_icount: 30,
                })
                .unwrap(),
            epoch_selection
        );

        let terminal_selection = index.select_terminal_divergence(45).unwrap();
        assert_eq!(terminal_selection.checkpoint.position, pos(40, 5));
        assert_eq!(terminal_selection.coverage_icount_lo, 20);
        assert_eq!(terminal_selection.coverage_icount_hi, 45);
        assert_eq!(
            terminal_selection.divergence,
            BisectionDivergenceSite::Terminal {
                position: pos(45, 6),
            }
        );
        assert_eq!(
            index
                .select_for_divergence(BisectionSelectionTarget::TerminalEndState {
                    end_icount: 45,
                })
                .unwrap(),
            terminal_selection
        );
        assert!(
            index.select_terminal_divergence(44).is_none(),
            "non-END icounts, including reseal-byte offsets, must not select terminal evidence"
        );
        assert!(index
            .select_for_divergence(BisectionSelectionTarget::TerminalEndState { end_icount: 44 })
            .is_none());
    }

    #[test]
    fn selection_falls_back_without_usable_checkpoint_evidence() {
        let index = BisectionCheckpointIndex::from_candidates(
            vec![epoch(1, 20, 0)],
            Some(pos(30, 1)),
            vec![],
        )
        .unwrap();
        assert!(index.is_empty());
        assert!(index.select_epoch_hash_divergence(1, 20).is_none());
        assert!(index.select_terminal_divergence(30).is_none());

        let epochs = vec![epoch(1, 20, 0)];
        let index = BisectionCheckpointIndex::from_candidates(
            epochs.clone(),
            Some(pos(30, 2)),
            vec![candidate(1, 20, 20, [0xA1; 32], 2_000, epochs[0])],
        )
        .unwrap();
        assert!(index.select_epoch_hash_divergence(2, 30).is_none());
    }

    #[test]
    fn validation_rejects_bad_checkpoint_metadata() {
        let e20 = epoch(1, 20, 0);
        let mut bad = candidate(1, 20, 20, [0xA1; 32], 2_000, e20);
        bad.format_version = BISECTION_CHECKPOINT_FORMAT_VERSION + 1;
        assert!(matches!(
            BisectionCheckpointIndex::from_candidates(vec![e20], None, vec![bad]).unwrap_err(),
            BisectionCheckpointIndexError::UnsupportedFormatVersion { seq: 1, .. }
        ));

        let mut bad = candidate(1, 20, 20, [0xA1; 32], 2_000, e20);
        bad.flags = 1;
        assert!(matches!(
            BisectionCheckpointIndex::from_candidates(vec![e20], None, vec![bad]).unwrap_err(),
            BisectionCheckpointIndexError::InvalidFlags { seq: 1, flags: 1 }
        ));

        let mut bad = candidate(1, 20, 20, [0xA1; 32], 2_000, e20);
        bad.checkpoint_icount = 19;
        assert!(matches!(
            BisectionCheckpointIndex::from_candidates(vec![e20], None, vec![bad]).unwrap_err(),
            BisectionCheckpointIndexError::IcountMismatch {
                seq: 1,
                record_icount: 20,
                checkpoint_icount: 19
            }
        ));

        let mut bad = candidate(1, 20, 20, [0xA1; 32], 2_000, e20);
        bad.preceding_epoch_hash = None;
        assert!(matches!(
            BisectionCheckpointIndex::from_candidates(vec![e20], None, vec![bad]).unwrap_err(),
            BisectionCheckpointIndexError::MissingPrecedingEpochHash { .. }
        ));

        let mut bad = candidate(1, 20, 20, [0xA1; 32], 2_000, e20);
        bad.preceding_epoch_hash = Some(epoch(1, 20, 2));
        assert!(matches!(
            BisectionCheckpointIndex::from_candidates(vec![e20], None, vec![bad]).unwrap_err(),
            BisectionCheckpointIndexError::CheckpointDoesNotFollowEpochHash { .. }
        ));

        let e40 = epoch(2, 40, 2);
        assert!(matches!(
            BisectionCheckpointIndex::from_candidates(
                vec![e20, e40],
                None,
                vec![
                    candidate(3, 40, 40, [0xA2; 32], 4_000, e40),
                    candidate(1, 20, 20, [0xA1; 32], 2_000, e20),
                ],
            )
            .unwrap_err(),
            BisectionCheckpointIndexError::InconsistentSequenceOrdering { .. }
        ));

        assert!(matches!(
            BisectionCheckpointIndex::from_candidates(
                vec![e20],
                None,
                vec![candidate(1, 20, 19, [0xA1; 32], 2_000, e20)],
            )
            .unwrap_err(),
            BisectionCheckpointIndexError::GapTooNarrow {
                seq: 1,
                required_gap: 20,
                ..
            }
        ));

        assert!(matches!(
            BisectionCheckpointIndex::from_candidates(
                vec![e20, e40],
                None,
                vec![
                    candidate(1, 20, 20, [0xA1; 32], 2_000, e20),
                    candidate(3, 40, 19, [0xA2; 32], 4_000, e40),
                ],
            )
            .unwrap_err(),
            BisectionCheckpointIndexError::GapTooNarrow {
                seq: 3,
                required_gap: 20,
                ..
            }
        ));
    }

    #[test]
    fn reader_index_rejects_checkpoint_before_same_icount_epoch_hash() {
        let mut writer = LogWriter::new(header());
        writer
            .bisection_checkpoint(20, 0x2000, 20, [0xA1; 32], 2_000)
            .unwrap();
        writer.epoch_hash(20, 0x2000, 1, [0x01; 32]).unwrap();
        let log = writer.seal(seal_params(25)).unwrap();
        let reader = LogReader::parse(&log).unwrap();

        assert!(matches!(
            BisectionCheckpointIndex::from_reader(&reader).unwrap_err(),
            BisectionCheckpointIndexError::MissingPrecedingEpochHash { .. }
        ));
    }

    #[test]
    fn reader_index_rejects_canonical_record_between_epoch_hash_and_checkpoint() {
        let mut writer = LogWriter::new(header());
        writer.epoch_hash(20, 0x2000, 1, [0x01; 32]).unwrap();
        writer
            .pad_set(20, 0x2004, 0, 0x0000_0001, FRAME_HINT_NONE)
            .unwrap();
        writer
            .bisection_checkpoint(20, 0x2008, 20, [0xA1; 32], 2_000)
            .unwrap();
        let log = writer.seal(seal_params(25)).unwrap();
        let reader = LogReader::parse(&log).unwrap();

        assert!(matches!(
            BisectionCheckpointIndex::from_reader(&reader).unwrap_err(),
            BisectionCheckpointIndexError::CheckpointSeparatedFromEpochHashByCanonicalRecord {
                checkpoint: RecordPosition { icount: 20, seq: 2 },
                epoch_hash: RecordPosition { icount: 20, seq: 0 },
                canonical: RecordPosition { icount: 20, seq: 1 },
            }
        ));
    }

    #[test]
    fn snapshot_ref_validation_rejects_unusable_refs() {
        let e20 = epoch(1, 20, 0);
        let e40 = epoch(2, 40, 2);
        let index = BisectionCheckpointIndex::from_candidates(
            vec![e20, e40],
            Some(pos(45, 4)),
            vec![
                candidate(1, 20, 20, [0xA1; 32], 2_000, e20),
                candidate(3, 40, 20, [0xA2; 32], 4_000, e40),
            ],
        )
        .unwrap();

        let err = index
            .validate_snapshot_refs(|snapshot_ref| {
                if snapshot_ref == [0xA2; 32] {
                    Err("missing snapshot")
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
        match err {
            BisectionCheckpointIndexError::UnusableSnapshotRef {
                seq,
                checkpoint_snapshot_ref,
                ..
            } => {
                assert_eq!(seq, 3);
                assert_eq!(checkpoint_snapshot_ref, [0xA2; 32]);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
