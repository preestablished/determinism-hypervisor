# Iteration 49 — landing-precision acceptance (bead 8g1): 2nd-reviewer overview

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** ralph/iteration-49-landing-precision
- **Diff:** `git diff main...HEAD` (4 files, +247)
- **Verdict:** **APPROVE**

## Scope reviewed

- `tests/determinism/tests/landing_precision.rs` (new) — 10k landing-loop
  targets + 1k REP-loop targets, two boots, tuple-equality + per-landing
  `icount == target` + REP `rcx ∈ {0,64}`.
- `tests/nanokernel/asm/rep_loop.asm` (new) — 6-instruction REP-MOVSB
  torture guest with RCX as a mid-REP detector.
- `tests/nanokernel/build.rs`, `tests/nanokernel/src/lib.rs` — wiring +
  exported constants.

## What I did beyond reading

I ran an **independent residue→rip analysis** (a temporary scratch test,
reverted; tree is clean) to attack the one thing the committed test does
*not* by itself prove: that the cross-boot-equal RIP is actually the
*start of an instruction* and not "the same wrong RIP in both boots"
(e.g. systematically one instruction late). See
`01-critical-and-important.md` §A — the result is conclusive and
**confirms instruction-start landing** on both guests.

I also ran the committed tests in full (both pass, ~95 s), and clippy
`--workspace --all-targets` on **both** x86_64 and aarch64 (clean on
both). Tree verified clean after instrumentation was removed.

## Bottom line

This is a high-quality acceptance test. The contract it claims (exact
landing, zero overshoot, margin-independence, no mid-REP boundary) is
real and now independently corroborated by the residue analysis. My
findings are all **non-blocking**: two Important maintainability items
(missing elf_shape entry + an unused exported const that breaks an
established sibling pattern) and a handful of suggestions. Nothing here
is a correctness defect in the code under test.

## Severity counts

- Critical: 0
- Important: 2
- Suggestions: 6
