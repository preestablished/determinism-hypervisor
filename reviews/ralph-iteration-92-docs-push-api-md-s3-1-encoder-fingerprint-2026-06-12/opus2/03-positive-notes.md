# Positive Notes

### P1 — Quote-on-text, not line numbers — and it actually matches

`docs/upstream-divergences.md:16-17` explicitly tells the operator to match on quoted text
because line numbers drift, then delivers: every "old" quote I checked is byte-for-byte
identical to the `d55ecc3` baseline (#1 API:520, #2 API:441-442, #3 API:617, #4 ARCH:740-741,
#5 ARCH:118-121 + 662-664, #6 ARCH:367-372, #7 API:614, #8 ARCH:720, #9 ARCH:656 +
IMPL-PLAN:79, #10 API:619). This is the single most important property of a divergence ledger
and it is done correctly. It is the textbook fix for the "vague references instead of exact
old→new text" pitfall.

### P2 — Placed in `docs/`, not `.agents/docs/`, with the reason stated inline

Lines 13-14 call out that the file lives in `docs/` "precisely so a sync cannot overwrite it."
This directly defends against the "divergence docs placed where a sync will clobber them"
pitfall — and, critically, the file *explains why* it is there, so a future maintainer does
not "helpfully" move it into `.agents/docs/` next to the things it documents.

### P3 — Authority named per entry, with a pointer to verifiable evidence

Every entry names who wins when code and spec disagree and points at the proof: applied
entries cite the local-amendment commit (`c7e2b1a`, `8a22a56`, `efa286f`, `d94c605`,
`84e99cc` — all verified to contain the claimed "new" text verbatim); proposed entries cite
the code or decision doc (`detchannel.rs` `EVTC_LEN`, `dirty.rs`, `tsc-alignment.md`,
`snapshot_engine.rs`). The proposed-new claims are genuinely accurate — e.g. #3's EVTC offset
map (0/4/8/12/13/17/18/22/23/31/35, total 39) reproduces `detchannel.rs::restore()` exactly,
and #5's "flag needed on the ring path too" is literally what the `enable_dirty_logging` doc
comment says ("Without the flag the ring stays empty").

### P4 — Applied vs proposed split, and the durable-ledger move off the bead notes field

The two-section structure (applied-local-amendment vs upstream-only) cleanly separates "this
will be silently reverted by sync" from "no local edit exists." The provenance note (299-307)
candidly records that the bead's notes field was overwritten across iterations and the ledger
was reconstructed from `dolt_history_issues` — and the bead `veu` notes now point AT this file
as the durable home ("do not re-accumulate entries in this notes field"). Moving the ledger
out of a mutable, overwritable field into a versioned file is exactly the right response to
the "ledgers in mutable overwritable fields" pitfall, and it is documented honestly.

### P5 — Wire/behavior-compatibility caveats are preserved, not lost in the rewrite

Where a spec change could be misread as a breaking change, the entries say it is not: #2 notes
"Wire-compatible (numbers unchanged)"; #7 keeps v1 as "the spec-exact 56-byte form" while
adding v2; #9 notes the landed acceptance compares snapshot REFS (BLAKE3 over the manifest),
"equal-or-stronger" than the originally-sketched hash. These keep the upstream reader from
over-correcting.
