# Suggestions (non-blocking)

### S1 — §6.2 "run control subtracts its segment base internally" is slightly imprecise about *where* the subtraction happens

**File:** `.agents/docs/determinism-hypervisor/ARCHITECTURE.md` lines 441–444

The new annotation on `TIMER_DEADLINE` is **substantively correct**: the device register holds
**absolute guest vns** (verified in `dh-devices/src/clock.rs` — `timer_deadline_vns` is the same
axis as `VNS_LO/HI`, base-relative continuity restored on restore, lines 44–50, 88–95). The
"absolute, never segment-relative" claim is right, and `gate.rs`/`timer_determinism.rs` both feed
`TIMER_AT = 1_234_567` straight into `TimerArm.deadline_vns` because base is 0 today, which is
consistent.

The imprecision is in the phrase "**run control** subtracts its segment base **internally**." In the
actual code layering:

- The device's `PvClock::armed()` doc (clock.rs lines 88–95) says run control converts the
  absolute deadline via `icount_for_vns_target(deadline - vns_base_of_segment)`.
- But `runctl::TimerArm` (runctl.rs lines 106–113) documents that *the caller* subtracts the base
  **before** constructing `TimerArm` — `deadline_vns` is already counter-space (origin-0). And
  `timer_to_injection` (lines 124–137) calls `icount_for_vns_target(timer.deadline_vns)` with **no**
  base subtraction inside it.

So today the subtraction is a **caller-boundary** step, not something `run_segment`/`timer_to_injection`
does internally; it's also a no-op because `vns_base` is always 0 ("Today that origin never moves",
runctl line 109) until M4 restore lands. At the spec altitude — describing the *device register
contract* — "run control subtracts the segment base" is a fair black-box description and I would not
block on it. But since the parenthetical invites the reader into the conversion mechanism, consider
softening "internally" so it doesn't imply a specific call site:

> …never segment-relative — the run-control layer subtracts the segment's vns base when it converts
> the deadline to a segment-relative icount target (today the base is 0; see `dh-vmm`'s
> `runctl::TimerArm` / `PvClock::armed`).

This also gives the reader the two code anchors, matching how the §6.4 cross-reference cites API.md.

### S2 — Consider citing the measurement count for "bit-stable across cold boots" in §3.1

The §3.1 sentence says "bit-stable across cold boots/cores/processes/load." The supporting code
comment (`nanokernel::COUNTING_DELTA_AT_OUT_EXITS`, lib.rs lines 116–120) and bead 0sc both quantify
it: "15+ cold boots bit-identical." A spec that says "bit-stable across N cold boots" is more
auditable than an unquantified "bit-stable." Minor; the constant already carries the number.

### S3 — Editorial-history parenthetical in §3.1 (style judgment, suggestion only)

The new §3.1 paragraph carries an inline historiography note:

> (An earlier revision of this section claimed "retire exactly once, on the completing resume"; the
> empirics refuted that.)

For a living spec this is defensible — it warns a reader who has the old claim in muscle memory.
But editorializing the document's own revision history inside normative prose is the kind of thing
that accretes: three iterations from now §3.1 may carry two or three such parentheticals. The
cleaner home is a `CHANGELOG`/`HISTORY` note or a one-line footnote, leaving the normative paragraph
to state the *current* rule cleanly. Purely a style call — no correctness impact, and the
information itself is worth keeping somewhere. If kept inline, fine; if the project has a changelog
convention, prefer moving it there.
