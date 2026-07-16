# Review Overview

- Branch: `ralph/iteration-153-persist-lapic-state-in-dhsnap-state-hash`
- Date: 2026-06-18
- Reviewer: Claude Opus

This branch adds a concrete DHSNAP `LAPC` v2 section, validates it into `LocalApic`, threads LAPIC state through snapshot/restore/fork/service/replay hashing, and re-baselines the affected golden fixtures. The codec/layout work is mostly disciplined: explicit little-endian fields, reserved-byte rejection, compatibility for empty v1, and golden hash updates all line up. I found one important propagation gap in VerifyReplay bisection capture: the replay probe snapshot still calls the wrapper that supplies a reset LAPIC, so bisection diagnostics can compare an expected checkpoint against an actual probe with stale LAPIC state whenever replay's live LAPIC is non-reset.

Overall verdict: REQUEST_CHANGES

Stats:

- Files changed: 27
- Lines added: 1294
- Lines removed: 219
- Commits: 1 (`dae068e ralph: iteration 153 checkpoint - persist lapic state`)
