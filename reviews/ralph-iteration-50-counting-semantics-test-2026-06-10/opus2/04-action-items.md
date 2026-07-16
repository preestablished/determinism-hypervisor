# Action items

### Critical
(none)

### Important
(none — the two hazards I was asked to break were disproven by live experiment; see
`01-critical-and-important.md`.)

### Suggestions

- **S1 — Make the margin-8 regression test's safety explicit.** With `skid_margin: 8,
  resync_slack: 8` and target 20, `land_at` takes the FAR approach (20 > 16) and arms a
  PMI period of 12. It does **not** overshoot only because the guest emits device exits
  (S OUT at count 6, MMIO read + write at count 12) that chop the far-run before any skid
  is spent — incidental to the *current* guest layout, not the engine. Either land from a
  closer anchor so the near path is actually exercised, or add a comment in
  `landing_across_an_mmio_write_does_not_free_run`
  (`tests/determinism/tests/counting_semantics.rs`) noting that margin 8 forces the FAR
  path and that a future asm edit removing those pre-target exits must revisit the
  margins. Verified non-flaky as written (30x + 20x parallel, 0 failures).

- **S2 — Document the plateau-landing rule in ARCH §3.2.** Add one sentence stating that
  when consecutive instructions retire zero (back-to-back VM-exiting instructions) an
  icount maps to multiple RIPs, that `land_at` deterministically returns the *first*
  such (icount, RIP) at a loop-top counter read (measured: `land_at(12)` → rip 0x100047
  over 240 cold boots), and that M6 should avoid scheduling stops on such plateaus.
  File: `.agents/docs/determinism-hypervisor/ARCHITECTURE.md` §3.2.

- **S3 — Defense-in-depth assert in the attribution region.** Add
  `assert!(s.exits.len() <= 1, ...)` at the top of the region loop in
  `single_step_attribution_of_every_retirement_case`
  (`tests/determinism/tests/counting_semantics.rs`, ~line 299) so a future guest that
  chains two device exits in one entry fails loudly instead of being silently
  reclassified by the `match`.

- **S4 — Extend the `step_one_entry` function-level doc-comment** (boundary.rs lines
  193-208) to state that an entry spanning an MMIO write traps at the write's *successor*,
  so the returned boundary is not necessarily the write instruction itself. The harness
  compensates with its sentinel; a future caller reading only the doc could be surprised.
