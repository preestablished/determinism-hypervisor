# XSAVE Allowlist Init-Encoding Hotfix — Review Overview

- **Branch:** `ralph/iteration-69-xsave-allowlist-hotfix` vs `main`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Scope:** Hotfix for a determinism flake introduced in iteration 68 (`crates/dh-vmm/src/xsave.rs`, +158/-25, single file).

## Summary

This hotfix rewrites `canonicalize()` from a *subtractive* rule (zero the areas of clear-bit
components) into an *allowlist* rule (rebuild the area as all-zeros plus a kept set of ranges)
and adds **init-state normalization** (a set XSTATE_BV bit whose component area is the
architectural init pattern is rewritten to bit-clear + zero area, with XSTATE_BV written back
normalized). Both changes are correct and well-targeted. The allowlist closes the iteration-68
garbage-leak vector (legacy reserved `[416,512)`, header reserved `[528,576)`, inter-component
gaps, and the buffer tail were never zeroed before and carried run-to-run kernel garbage into
the hash preimage). The init normalization closes the deeper KVM nondeterminism (INIT-state
component reported either bit-clear or bit-set-with-init-pattern depending on host preemption at
`GET_XSAVE`). The `is_x87_init` pattern (FCW=0x037F, FSW/FTW-abridged/FOP/FIP/FDP=0, ST0-7=0,
MXCSR deliberately excluded) is **exactly** the FXSAVE-layout architectural init state per SDM.
Tests pin both encodings → identical canonical form, non-init bits survive, garbage is always
zeroed, and OOB is loud whether the bit is clear or set. All 94 `dh-vmm` lib tests pass (incl.
the KVM live test), and `skid_gate` passed 4/4 consecutive runs. The commit message records the
process lesson.

The one item that genuinely needs to be nailed down before — or at least loudly *at* — the 55f
restore reuse is the SET_XSAVE/XRSTOR safety of the **extended bits** and the **MXCSR-vs-SSE-bit**
interaction. For the **current hash-only** consumer (`hash.rs:278`) everything is sound. For the
**future 55f restore** path the function doc promises reuse of, the generic extended-bit
normalization is only safe for components whose init state is genuinely all-zeros — true for
AVX/SSE but NOT guaranteed for every XCR0 component the generic loop will accept (PKRU init can be
nonzero; the same byte-allzero test would wrongly clear a non-init component on a future CPU).
Phase 1 masks extended components so this is latent, not live — hence Important, not Critical.

## Verdict

**APPROVE** (with two Important follow-ups to file as beads before 55f reuses `canonicalize()`
on the restore path; neither blocks this hash-only hotfix).

## Stats

- Files changed: 1 (`crates/dh-vmm/src/xsave.rs`)
- Lines: +158 / -25
- Tests: 94/94 `dh-vmm` lib pass (incl. live KVM test); `skid_gate` 2/2 × 4 runs
- Findings: 0 Critical, 2 Important, 4 Suggestions
