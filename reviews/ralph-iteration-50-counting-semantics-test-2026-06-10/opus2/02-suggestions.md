# Suggestions (non-blocking)

## S1 — The margin-8/8 choice in the regression test is robust *today* but brittle to guest edits

**Prompt angle 5 — the flakiness question, answered.** Default margins are 8192/1024.
The regression test deliberately uses `skid_margin: 8, resync_slack: 8` so the
near-window straddles the MMIO write. Threshold = `skid + slack` = 16; target 20 > 16, so
**the FAR approach is taken**, arming a PMI period of `d - skid = 20 - 8 = 12`.

The prompt worried this overshoots because skid was measured up to 39 elsewhere
(iteration-16 doc says 18 with zero variance; the prompt cites 39). I ran the test **30x
and the full file 20x in parallel — 0 failures.** It is **not flaky** here. The reason is
structural and worth understanding:

Between count 0 and count 12 the guest **exits to userspace repeatedly** (the `S` OUT at
count 6, then the CPUID is in-kernel but the MMIO read and write at count 12 force
`KVM_EXIT_MMIO`). Each such exit returns from `KVM_RUN` *before* the armed PMI period of
12 can elapse in a single free-run. So the far-approach run never actually free-runs
12+skid instructions in one go; it is chopped into sub-12 segments by the device exits,
the engine re-reads the counter at each, and crosses into the near (stepping) phase well
before any skid could carry it past 20. The skid budget is simply never spent at these
tiny targets.

**That safety is incidental to the *current* guest layout**, not to the engine. If a
future edit to `counting.asm` removed the MMIO read/write before the target, a margin-8
FAR approach at a tiny target *could* in principle skid past 20. The general rule the
prompt asks for is correct: **for the FAR approach to be safe, the target's distance must
exceed the observed max skid, OR the approach must be anchored from a closer count.**

Recommendation (pick one):
- Land to an earlier anchor first (e.g. `land_at(8)` with default margins, then
  `land_at(20)` with tiny margins so 20-8=12 < 16 forces the *near* path that the test
  actually means to exercise), **or**
- Add a one-line comment in `landing_across_an_mmio_write_does_not_free_run` noting that
  margin 8 forces the FAR path and that device exits before count 12 are what keep it
  from skidding — so a future asm edit that drops those exits must revisit the margins.

This is a suggestion, not a fix: the test passes deterministically as written and the
*engine* is correct regardless of margins. The fragility is in the test's coupling to the
guest's exit layout, which deserves a comment.

## S2 — Document the plateau-landing rule in ARCH §3.2

§3.2 says boundaries are `(icount, RIP)` tuples and "the landed boundary is independent of
[margins]", but is silent on icounts that map to multiple RIPs (zero-retiring exits in a
row). The measured behavior — `land_at` returns the **first** (lowest-RIP-in-stream)
count==target observation, and it is replay-stable — should be a sentence in §3.2 so the
M6 scheduler and future engine work can rely on it. Suggested wording: *"When several
consecutive instructions retire zero (back-to-back VM-exiting instructions), an icount
maps to multiple RIPs; `land_at` deterministically returns the first such (icount, RIP)
observed at a loop-top counter read. M6 should avoid scheduling stops on such plateaus."*

## S3 — Assert exits-per-step ≤ 1 in the attribution region (defense-in-depth)

**Prompt angle 2.** The classifier's `other => panic!` arm is exhaustive for the *current*
trace, and with the re-arm fix a single entry can no longer chain two device exits in the
region slice (the write sentinel short-circuits; the MMIO read is the only non-short-
circuit exit and it is followed by a `#DB`). To make that invariant explicit and catch a
future guest that emits two exits in one entry, add inside the region loop:
`assert!(s.exits.len() <= 1, "step {i} produced {} exits", s.exits.len());`
before the `match`. Cheap, and it turns a silent reclassification into a loud failure.

## S4 — `step_one_entry` re-arm: the doc-comment now describes a multi-instruction entry

The new comment in `step_one_entry` correctly notes "one entry can span the write plus its
successor." Worth adding to the **function-level** doc-comment (lines 193-208) that the
"one ENTRY" contract now explicitly *excludes* the per-instruction guarantee across an
MMIO write — i.e. the returned boundary may be the successor of a write, not the write
itself. The `counting_semantics` harness already compensates with its own sentinel, but a
future caller reading only the doc-comment could assume a write traps at the write.
