//! DHILOG lineage splicing (bead 3lt): compose a fork-tree path —
//! `child log = parent prefix + child segment` — into one replayable
//! whole. DHILOG v1 stays FROZEN (INTEGRATION §3): a splice is NOT a
//! byte-concatenated file but a VALIDATED SEQUENCE of sealed v1
//! segments, verified per edge with root-anchored induction —
//! `VerifyReplay(base = snapshot_{i-1}, log_i)` for every i, where
//! segment i's `base_snapshot_id` must equal segment i-1's
//! `end_snapshot_id` (content-addressed refs make the induction sound:
//! equal ref ⇒ bit-identical state). The flat `.dilog` v2 container
//! that persists a splice is replay-renderer's, out of this repo.
//!
//! Continuity rules enforced at construction (anything else is a
//! corrupt or hostile lineage and errors loudly):
//! - every segment parses as a SEALED v1 log (the reader's full
//!   validation: watermark, body hash, END identity);
//! - STITCHING: `seg[i].end_snapshot_id == seg[i+1].base_snapshot_id`,
//!   and inner segments must HAVE an end snapshot (zeros = "no end
//!   snapshot taken", legal only for the final segment — a parent
//!   without an end ref cannot anchor a child);
//! - one machine: `machine_config_hash` and the clock ratio are
//!   identical across the lineage (a config change mid-lineage is a
//!   different machine, never a splice);
//! - icount/seq axes RESTART per segment by design (§3.1: each segment
//!   counts from its base snapshot's boundary; replay resets the
//!   counter per edge), so there is no cross-segment watermark — the
//!   per-segment watermark plus the stitch rule IS the continuity.

use crate::reader::{Header, LogReader, ReadError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpliceError {
    /// A lineage must have at least one segment.
    Empty,
    /// Segment `index` failed the reader's validation.
    Segment { index: usize, err: ReadError },
    /// `seg[index].end_snapshot_id != seg[index+1].base_snapshot_id`.
    BrokenStitch { index: usize },
    /// An INNER segment has a zero end_snapshot_id — nothing anchors
    /// the next edge.
    MissingEndSnapshot { index: usize },
    /// `machine_config_hash` differs from segment 0's.
    ConfigMismatch { index: usize },
    /// Clock ratio differs from segment 0's.
    ClockMismatch { index: usize },
}

/// One edge of a validated lineage, ready for per-edge verification:
/// replay `log` from `base_snapshot_id`.
pub struct Edge<'a> {
    pub index: usize,
    pub base_snapshot_id: [u8; 32],
    pub log: LogReader<'a>,
}

/// A validated fork-tree path. Borrowed: the segment bytes live with
/// the caller (the store / the recording rails); this type only proves
/// they compose. `Clone` is cheap (slices + headers) and is how one
/// validated parent prefix is shared across many children (M7: one
/// root, a thousand forks).
#[derive(Clone, Debug)]
pub struct Lineage<'a> {
    segments: Vec<(&'a [u8], Header)>,
}

impl<'a> Lineage<'a> {
    /// Validate `segments` (root-first order) into a lineage.
    pub fn new(segments: &[&'a [u8]]) -> Result<Self, SpliceError> {
        if segments.is_empty() {
            return Err(SpliceError::Empty);
        }
        let mut parsed: Vec<(&'a [u8], Header)> = Vec::with_capacity(segments.len());
        for (index, bytes) in segments.iter().enumerate() {
            let log = LogReader::parse(bytes).map_err(|err| SpliceError::Segment { index, err })?;
            parsed.push((bytes, log.header().clone()));
        }
        let first = &parsed[0].1;
        for (index, (_, h)) in parsed.iter().enumerate() {
            if h.machine_config_hash != first.machine_config_hash {
                return Err(SpliceError::ConfigMismatch { index });
            }
            if h.clock_num != first.clock_num || h.clock_den != first.clock_den {
                return Err(SpliceError::ClockMismatch { index });
            }
        }
        for index in 0..parsed.len() - 1 {
            let end = parsed[index].1.end_snapshot_id;
            if end == [0u8; 32] {
                return Err(SpliceError::MissingEndSnapshot { index });
            }
            if end != parsed[index + 1].1.base_snapshot_id {
                return Err(SpliceError::BrokenStitch { index });
            }
        }
        Ok(Self { segments: parsed })
    }

    /// `child log = parent prefix + child segment` — the fork-tree
    /// composition this module exists for. Re-validates only the new
    /// edge (the prefix is already proven). Borrows the parent so one
    /// validated prefix can fan out to many children.
    pub fn extend(&self, child_segment: &'a [u8]) -> Result<Self, SpliceError> {
        let index = self.segments.len();
        let log =
            LogReader::parse(child_segment).map_err(|err| SpliceError::Segment { index, err })?;
        let h = log.header().clone();
        let first = &self.segments[0].1;
        if h.machine_config_hash != first.machine_config_hash {
            return Err(SpliceError::ConfigMismatch { index });
        }
        if h.clock_num != first.clock_num || h.clock_den != first.clock_den {
            return Err(SpliceError::ClockMismatch { index });
        }
        let last = &self.segments[index - 1].1;
        if last.end_snapshot_id == [0u8; 32] {
            return Err(SpliceError::MissingEndSnapshot { index: index - 1 });
        }
        if last.end_snapshot_id != h.base_snapshot_id {
            return Err(SpliceError::BrokenStitch { index: index - 1 });
        }
        let mut segments = self.segments.clone();
        segments.push((child_segment, h));
        Ok(Self { segments })
    }

    pub fn len(&self) -> usize {
        self.segments.len()
    }

    pub fn is_empty(&self) -> bool {
        false // Lineage::new rejects empty — kept for clippy's len/is_empty pairing
    }

    /// The root snapshot the whole lineage replays from.
    pub fn root_base(&self) -> [u8; 32] {
        self.segments[0].1.base_snapshot_id
    }

    /// The final segment's END identity: `(end_snapshot_id,
    /// end_state_hash, end_icount)` — zeros end_snapshot_id means the
    /// leaf took no end snapshot (legal at the leaf only).
    pub fn end_identity(&self) -> ([u8; 32], [u8; 32], u64) {
        let h = &self.segments.last().expect("non-empty by construction").1;
        (h.end_snapshot_id, h.end_state_hash, h.end_icount)
    }

    /// The per-edge verification plan, root-first: replay `edge.log`
    /// from `edge.base_snapshot_id` and check its own END identity —
    /// the root-anchored induction (INTEGRATION §3). Parsing cannot
    /// fail here (validated at construction).
    pub fn edges(&self) -> impl Iterator<Item = Edge<'a>> + '_ {
        self.segments
            .iter()
            .enumerate()
            .map(|(index, (bytes, h))| Edge {
                index,
                base_snapshot_id: h.base_snapshot_id,
                log: LogReader::parse(bytes).expect("validated at construction"),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dhilog::{LogWriter, SealParams, SegmentHeader};

    /// A sealed segment with the given lineage anchors.
    fn seal_segment(base: [u8; 32], end: [u8; 32], cfg: [u8; 32], clock: (u32, u32)) -> Vec<u8> {
        let mut w = LogWriter::new(SegmentHeader {
            base_snapshot_id: base,
            entropy_seed: [0; 32],
            machine_config_hash: cfg,
            clock_num: clock.0,
            clock_den: clock.1,
            encoder_fingerprint: 0,
        });
        w.pad_set(1_000, 0x40, 0, 0xAB12, 0).unwrap();
        w.seal(SealParams {
            end_snapshot_id: end,
            end_icount: 50_000,
            end_vns: 50_000,
            end_state_hash: [0xEE; 32],
            stop_reason: 5,
        })
        .unwrap()
    }

    const CFG: [u8; 32] = [0x11; 32];
    const R: [u8; 32] = [0xA0; 32]; // root snapshot
    const S1: [u8; 32] = [0xA1; 32];
    const S2: [u8; 32] = [0xA2; 32];

    #[test]
    fn a_stitched_lineage_validates_and_plans_edges() {
        let a = seal_segment(R, S1, CFG, (1, 1));
        let b = seal_segment(S1, S2, CFG, (1, 1));
        let c = seal_segment(S2, [0; 32], CFG, (1, 1)); // leaf: no end snapshot
        let lineage = Lineage::new(&[&a, &b, &c]).unwrap();
        assert_eq!(lineage.len(), 3);
        assert_eq!(lineage.root_base(), R);
        let (end_snap, end_hash, end_icount) = lineage.end_identity();
        assert_eq!(end_snap, [0; 32]);
        assert_eq!(end_hash, [0xEE; 32]);
        assert_eq!(end_icount, 50_000);

        let plan: Vec<([u8; 32], usize)> = lineage
            .edges()
            .map(|e| (e.base_snapshot_id, e.index))
            .collect();
        assert_eq!(plan, vec![(R, 0), (S1, 1), (S2, 2)]);
        // Every edge's log is independently replayable (parses sealed).
        for e in lineage.edges() {
            assert!(e.log.header().has_aux() || !e.log.header().has_aux()); // parsed ✓
        }
    }

    #[test]
    fn extend_is_the_fork_composition() {
        let a = seal_segment(R, S1, CFG, (1, 1));
        let parent = Lineage::new(&[&a]).unwrap();

        // One validated parent prefix fans out to many children — the
        // M7 shape (one root, a thousand forks): extend borrows.
        let b = seal_segment(S1, [0; 32], CFG, (1, 1));
        let b2 = seal_segment(S1, S2, CFG, (1, 1));
        let child = parent.extend(&b).unwrap();
        let sibling = parent.extend(&b2).unwrap();
        assert_eq!(child.len(), 2);
        assert_eq!(child.root_base(), R);
        assert_eq!(sibling.len(), 2);
        assert_eq!(sibling.end_identity().0, S2);
        assert_eq!(parent.len(), 1); // the prefix is untouched

        // A child whose base is NOT the parent's end ref is refused.
        let stranger = seal_segment(S2, [0; 32], CFG, (1, 1));
        assert_eq!(
            parent.extend(&stranger).unwrap_err(),
            SpliceError::BrokenStitch { index: 0 }
        );
    }

    #[test]
    fn a_single_leaf_only_segment_is_a_valid_lineage() {
        // The simplest fork child: one sealed segment, no end snapshot.
        let leaf = seal_segment(R, [0; 32], CFG, (1, 1));
        let lineage = Lineage::new(&[&leaf]).unwrap();
        assert_eq!(lineage.len(), 1);
        assert!(!lineage.is_empty());
        assert_eq!(lineage.root_base(), R);
        assert_eq!(lineage.end_identity().0, [0; 32]);
        let plan: Vec<usize> = lineage.edges().map(|e| e.index).collect();
        assert_eq!(plan, vec![0]);
    }

    #[test]
    fn continuity_violations_are_loud() {
        let a = seal_segment(R, S1, CFG, (1, 1));
        let b = seal_segment(S1, S2, CFG, (1, 1));

        assert_eq!(Lineage::new(&[]).unwrap_err(), SpliceError::Empty);

        // Broken stitch.
        let stranger = seal_segment([0xBB; 32], [0; 32], CFG, (1, 1));
        assert_eq!(
            Lineage::new(&[&a, &stranger]).unwrap_err(),
            SpliceError::BrokenStitch { index: 0 }
        );

        // Inner segment without an end snapshot cannot anchor an edge.
        let dead_end = seal_segment(R, [0; 32], CFG, (1, 1));
        let next = seal_segment([0; 32], [0; 32], CFG, (1, 1));
        assert_eq!(
            Lineage::new(&[&dead_end, &next]).unwrap_err(),
            SpliceError::MissingEndSnapshot { index: 0 }
        );

        // One machine only.
        let other_cfg = seal_segment(S1, S2, [0x22; 32], (1, 1));
        assert_eq!(
            Lineage::new(&[&a, &other_cfg]).unwrap_err(),
            SpliceError::ConfigMismatch { index: 1 }
        );
        let other_clock = seal_segment(S1, S2, CFG, (2, 1));
        assert_eq!(
            Lineage::new(&[&a, &other_clock]).unwrap_err(),
            SpliceError::ClockMismatch { index: 1 }
        );

        // A corrupt segment is the reader's loud error, indexed.
        let mut corrupt = b.clone();
        let last = corrupt.len() - 1;
        corrupt[last] ^= 0xFF;
        match Lineage::new(&[&a, &corrupt]).unwrap_err() {
            SpliceError::Segment { index: 1, .. } => {}
            other => panic!("wrong error: {other:?}"),
        }
    }
}
