# Action Items

### Critical

_None._ All "old" quotes match the `d55ecc3` baseline, all applied-amendment "new" texts
match their commits verbatim, and all five proposed-new technical claims verify against the
cited code. Nothing here would ship a wrong spec upstream.

### Important

- [ ] [docs/upstream-divergences.md:274] Fix the #8 provenance: the ledger says "iteration
      73" but the cited authority (`snapshot_engine.rs` module header) says "iteration-70
      review I1, decided here." Reconcile to the code's number, or cite both with their
      relationship. (I1)
- [ ] [docs/upstream-divergences.md:196] Give #5 a full path for its authority: replace bare
      `dirty.rs` with `crates/dh-vmm/src/dirty.rs` (`enable_dirty_logging`) to match the
      file's own convention and let a zero-context operator grep it. (I2)
- [ ] [docs/upstream-divergences.md:16-17] Add an operator failure-mode line: if a quoted
      "old" string is NOT found verbatim upstream, STOP and flag for human reconciliation —
      do not guess the insertion point. Call out that #4/#5/#6 quote multi-line blocks that
      must be matched whole. (I3)
- [ ] [docs/upstream-divergences.md:247-270] Make #6 a clean swappable old→new pair: either
      extend the "Old" quote through "...M3 before freezing the mechanism." or instruct the
      operator to replace item 4 in its entirety (and note the dropped "benchmark both in M3"
      clause is intentional). As quoted, a mechanical replace strands the trailing clause. (I4)

### Suggestions

- [ ] [docs/upstream-divergences.md:1-18] One sentence framing "bead"/"iteration" refs as
      provenance-only, not prerequisites to apply an entry. (S1)
- [ ] [docs/upstream-divergences.md:21] Note that the five applied "new" texts are verbatim
      review-passed local edits, while the five proposed wordings are newly authored (accurate
      but not doc-review-tested). (S2)
- [ ] [docs/upstream-divergences.md:39-40,86,142,163-169] Note that long `| ... |` rows are
      intentionally single-line; paste unwrapped or the Markdown cell breaks. (S3)
- [ ] [docs/upstream-divergences.md:188-189] Note that #4's lifecycle chain is split into two
      inline-code spans on purpose — there is no `Running → Frozen` edge — so it is not
      "tidied" back together. (S4)
- [ ] [docs/upstream-divergences.md, per entry] Add a "consequences" footer per applied entry:
      after upstream applies + re-sync, the local amendment is subsumed and bead `veu` can
      close. (S5)
