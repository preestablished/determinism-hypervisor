# Critical and Important Issues

## Critical

**None.** I verified the content that would actually ship upstream and found no misquote and
no wrong technical claim:

- All ten "old" quotes match the `d55ecc3` baseline exactly (`git show d55ecc3:.agents/docs/
  determinism-hypervisor/{API,ARCHITECTURE}.md`): #1 line 520, #2 lines 441-442, #3 line 617,
  #4 lines 740-741, #5 lines 118-121 and 662-664, #6 lines 367-372, #7 line 614, #8 line 720,
  #9 ARCHITECTURE line 656 + IMPLEMENTATION-PLAN line 79, #10 line 619.
- All five applied-amendment "new" texts (#1, #2, #7, #9, #10) match their cited commits
  (`c7e2b1a`, `8a22a56`, `efa286f`, `d94c605`, `84e99cc`) verbatim.
- All five proposed-new technical claims check out against code: #3 EVTC layout matches
  `detchannel.rs` (`EVTC_LEN = 4+4+4+5+5+1+16 = 39`, offsets 0/4/8/12/13/17/18/22/23/31/35);
  #4 matches §8.4 ("a **paused parent** ... is `Frozen{children:n}`"); #5 matches `dh-vmm/src/
  dirty.rs::enable_dirty_logging` ("Without the flag the ring stays empty"); #6 matches
  `docs/decisions/tsc-alignment.md` (offset-once, `guest_tsc = host_tsc + offset`, ~3k
  exits/s, 2026-06-10); #8 matches `dh-worker/src/snapshot_engine.rs` module header
  (option (b), STAY SEPARATE, field-selective `canonical_vcpu_blob`, iteration-69 hazard).

## Important

### I1 — Provenance discrepancy: #8 "iteration 73" vs the code's "iteration-70"

- **Severity:** Important
- **Location:** `docs/upstream-divergences.md:274`
- **Description:** The ledger says #8 was "Found: iteration 73 (the qmp reconciliation
  decided it)." But the cited authority itself — the `snapshot_engine.rs` module header —
  states "HASH vs SECTION reconciliation (**iteration-70 review I1**, decided here)." Both
  point at the qmp bead's reconciliation, but the iteration number in the ledger (73)
  contradicts the iteration number in the authoritative source (70). For a ledger whose whole
  value is a trustworthy provenance trail, a citation that disagrees with the artifact it
  cites undermines confidence. This does not affect the upstream-applied text (the "old"/
  "proposed new" wording is correct), so it is Important, not Critical.
- **Suggested fix:** Reconcile to the code's own number, or cite both with the relationship:
  ```
  - **Found:** iteration 70 review I1 (the qmp reconciliation decided it; the divergence
    was logged against the bead at iteration 73). **Authority:** ...
  ```

### I2 — `dirty.rs` authority for #5 is under-specified (no crate path)

- **Severity:** Important
- **Location:** `docs/upstream-divergences.md:196`
- **Description:** #5 cites "**Authority:** `dirty.rs` `enable_dirty_logging`." There is one
  `dirty.rs` in the tree (`crates/dh-vmm/src/dirty.rs`), so it is currently locatable, but
  every other code authority in the file uses a full crate-relative path (`crates/dh-devices/
  src/detchannel.rs`, `crates/dh-worker/src/snapshot_engine.rs`, `docs/decisions/
  tsc-alignment.md`). A bare `dirty.rs` is a vague reference by the file's own standard, and a
  human operator with zero context cannot `grep` a unique path. The function exists and the
  claim is correct — `enable_dirty_logging` sets `KVM_MEM_LOG_DIRTY_PAGES` and the doc comment
  literally says "Without the flag the ring stays empty" — only the pointer is weak.
- **Suggested fix:**
  ```
  **Authority:** `crates/dh-vmm/src/dirty.rs` (`enable_dirty_logging` — sets
  `KVM_MEM_LOG_DIRTY_PAGES`; its doc comment: "Without the flag the ring stays empty").
  ```

### I3 — Operator has no stated way to confirm "upstream still matches `d55ecc3`"

- **Severity:** Important
- **Location:** `docs/upstream-divergences.md:16-17` (and the closing note, 299-307)
- **Description:** The file's load-bearing assumption is that the upstream tree the operator
  edits still contains the quoted "old" text. The header correctly says "line numbers may have
  drifted upstream — match on the quoted text," and the closing note repeats "quote-match
  against upstream before applying." That is good. What is missing is the failure-mode
  instruction: what should the operator do if a quoted "old" string is NOT found upstream (it
  was already changed, or reworded)? Several "old" quotes are multi-line list/prose passages
  (#4, #5, #6), which are more fragile to upstream drift than the single-cell table rows. A
  zero-context operator could silently skip an entry, or worse, paste the "new" text in the
  wrong place. This is the difference between "applicable" and "mechanically safe to apply."
- **Suggested fix:** Add one operator-protocol line near the header, e.g.:
  ```
  If a quoted "old" string is not found verbatim upstream, STOP and treat that entry as
  needing human reconciliation — do not guess the insertion point. Entries #4/#5/#6 quote
  multi-line passages; match the whole block, not just the first line.
  ```

### I4 — #6 "old" quote is truncated mid-sentence; the replacement scope is ambiguous

- **Severity:** Important
- **Location:** `docs/upstream-divergences.md:247-256` (the #6 "Old" block) and 258-270
  ("Proposed new")
- **Description:** The #6 "Old" block ends mid-sentence at "...prefer adjusting the **TSC
  offset**" — but the baseline sentence continues for two more lines upstream:
  "`(KVM_VCPU_TSC_CTRL offset attribute) over MSR value writes; benchmark both in / M3 before
  freezing the mechanism.`" The "Proposed new" is a full rewrite of item 4 whose last clause
  ("benchmark both in M3 before freezing") is dropped. Because the quoted "old" text does not
  include that trailing clause, a mechanical operator doing find-and-replace on the quoted old
  string will leave the orphaned "`(KVM_VCPU_TSC_CTRL offset attribute) over MSR value writes;
  benchmark both in M3 before freezing the mechanism.`" stranded after the replacement,
  producing a garbled item 4. The header's note says to "rewrite item 4's opening," which
  signals intent, but the old/new blocks as quoted are not a clean swappable pair.
- **Suggested fix:** Either (a) extend the #6 "Old" quote to the full item 4 (through "M3
  before freezing the mechanism.") so old→new is a complete, replaceable unit, or (b) state
  explicitly that the operator should replace item 4 *in its entirety* (not just the quoted
  fragment) and that the "benchmark both in M3" clause is intentionally removed because the
  mechanism is now decided. Right now neither the old quote nor the instruction is
  unambiguous for a zero-context operator.
