# Phase 1 Exit Gate Sign-off — Adversarial Evidence Review (bead dk1)

- **Reviewer:** Claude Opus
- **Date:** 2026-06-10
- **Branch:** ralph/iteration-55-exit-gate vs main
- **Diff under review:** `docs/phase-1-exit-gate.md` (NEW), `docs/ops/skid-histogram-2026-06-10.txt` (NEW)
- **Host:** lab box with rw `/dev/kvm` (Intel vmx present), kernel 6.8.0-124-generic
- **Scope:** This is an EVIDENCE review, not a style review. Every row of the
  sign-off table was audited by re-execution where feasible, and every code
  claim was grep-verified against the actual source.

## Verdict: APPROVE

Every one of the 7 evidence rows survived audit. Each was either re-run live on
this box and reproduced the claimed result, or its underlying artifact/code was
inspected and found to support the table verbatim. The two new files are
internally consistent, the archived histogram is committed in THIS diff (closing
the "histogram archived" checklist item), branch protection is live with all
three required checks, and the latest main CI run (27284395335) is SUCCESS. The
full workspace suite is entirely green (zero failures, zero `#[ignore]`, zero
TODO/FIXME/XXX in source). The sequencing-guard framing is a faithful quote of
the phase doc.

There are no Critical or Important findings. Two minor precision/transparency
suggestions are noted (non-blocking) so a fresh M4 implementer is not subtly
misled by the table's representative-hash shorthand.

## What I re-ran live (all reproduced)

| Audit action | Result |
|---|---|
| `dh-cli gate --runs 10` (spot check; sign-off ran 100) | PASS, both sub-gates; plain hash `482edfed…`, timer hash `7e09ac13…`, icount 2,000,000, timer delivered 1,234,567 — matches table |
| `dh-cli skid --samples 50000` | max 71 < 4096; 49,999 ≤ 31; distribution matches archive shape (~16.6k at 27/30/31) |
| Archived histogram internal consistency | count=50000, sum=1466744, min=26, max=79, all cumulative buckets exact (49,997 ≤ 31) |
| `cargo test … counting_semantics` | 2/2 ok |
| `cargo test … landing_precision` | 2/2 ok (10k targets + 1k REP) |
| `cargo test … regression` | 2/2 ok (10M + 1B, 3.77–3.92 s) |
| `cargo test … m1_acceptance` | 1/1 ok |
| `cargo test … timer_determinism` | 1/1 ok (100 runs × 10 fires, 91–95 s) |
| `cargo test … if0_deferral` | 1/1 ok (100 runs) |
| `cargo test --workspace` (item 7) | all green; tree clean after |
| `gh api …/branches/main/protection` | live; required checks kvm-intel + both host legs |
| CI run 27284395335 | on main, conclusion=success, latest main run |

## Honest-sign-off test

A fresh M4 implementer reading this doc would not be misled on any load-bearing
point. The §8 "what this unblocks" preimages all exist in code as described
(`StateHashChain::from_value`, doc-order MSR blob + normalized-TSC slot, EVTC v1,
SegmentHeader encoder fingerprint, empty-serial rule). The two minor suggestions
below are about cosmetic precision, not substance.
