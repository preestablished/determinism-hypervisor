# Positive Notes

This file is a textbook example of spec-divergence documentation done right. Specific
patterns worth preserving:

### Exact, quote-matchable old → new text (the headline strength)

`docs/upstream-divergences.md` — every entry ships the verbatim upstream "old" string
and the verbatim "new"/"proposed" string in fenced blocks. I diffed nine of these
against the actual source-of-truth (five amendment commits + the `d55ecc3` baseline +
current docs) and they matched byte-for-byte. This directly answers the "line numbers
will drift upstream — match on text" pitfall; the file even says so explicitly
(lines 16-17). This is the single most important property for an apply-verbatim
artifact and it is met.

### Authority is named per entry, with a pointer to evidence

Every divergence states *who wins* when code and spec disagree and *why*:
- #3 → `detchannel.rs` (`EVTC_LEN`/`EVTC_VERSION`, "ships and round-trips, pinned by
  tests")
- #5 → `dirty.rs` `enable_dirty_logging`, with the empirical A/B result (0 vs ≥3 ring
  entries on a 6.8 kernel) as the trigger
- #6 → `docs/decisions/tsc-alignment.md` (measured, dated)
- #8 → `snapshot_engine.rs` module-top decision doc

I confirmed each named authority actually contains the cited claim. Naming the
authority *and* the measurement/test that establishes it (not just the conclusion) is
exactly the ADR discipline that keeps a record valuable.

### Provenance trail that survives a mutable-field overwrite

`docs/upstream-divergences.md:299-307` — the provenance note candidly explains that
the bead's notes field was repeatedly overwritten and that #1–#7 had to be recovered
from `dolt_history_issues`. Recording *how* the ledger was reconstructed (and from
where) is the kind of meta-provenance that prevents the next maintainer from
distrusting the older entries. It also implicitly documents the anti-pattern (ledger
in a mutable overwritable field) that motivated moving the canonical copy into a
git-tracked file.

### Placed where a sync cannot clobber it

`docs/upstream-divergences.md:13-14` — the file lives in `docs/`, explicitly NOT
`.agents/docs/`, "precisely so a sync cannot overwrite it." This is the correct fix
for the "divergence doc placed where a sync will clobber it" pitfall, and it is
called out in-line so a future reader doesn't "helpfully" move it.

### Clear separation of applied edits vs upstream-only proposals

The two top-level sections cleanly distinguish the five divergences already amended
locally (which a sync WILL revert — apply upstream first) from the five upstream-only
wording fixes (no local edit; code/decision doc is the authority). The intro states
the 5/5 split and the section headers restate the consequence. This distinction is
exactly what an upstream maintainer needs to avoid double-applying or missing a
silent revert.

### Internally consistent provenance metadata

All five cited amendment commits exist and touch the docs they claim; every
iteration↔commit pair is correct; all referenced beads (veu, 4ld, bcb, 28i, mmv)
resolve. The numbers, byte arithmetic, and offsets all check out. Nothing was
hand-waved.
