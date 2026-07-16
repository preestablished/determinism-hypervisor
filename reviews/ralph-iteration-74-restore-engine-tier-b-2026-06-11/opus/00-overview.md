# Review Overview — restore engine, tier B (bead 9wa)

- **Branch:** `ralph/iteration-74-restore-engine-tier-b`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Stats:** 6 files, +1035 / -0, 1 commit
- **Verdict:** **APPROVE**

## Summary

This iteration adds `restore_snapshot` (the §8.3 tier-B RESTORE engine) as a clean
mirror of the existing `take_snapshot` capture path, plus a minimal `as_any_mut`
downcast seam on `DetDevice` (defaulted, overridden only by `PvClock`) and a
`MmioBus::devices_mut` pass. The engine fetches the manifest and the
server-flattened full page set from the real snapshot-store, materializes guest
RAM through the slot's live mapping, then restores in the load-bearing order
RAM → devices → vCPU, re-seeding the segment clocks (PvClock `vns_base` ← TIME.vns,
hash chain via `from_value`, optional counter `IOC_RESET`, dirty-set clear). The
shape-strictness contract is enforced fail-closed in both directions (every bus
device must find its section; every container section must find a consumer),
the ENTR version-domain split and the empty-v1 LAPC placeholder are handled
exactly as the capture side and ARCH §8.3 specify, and the headline transparency
property (take → restore into a fresh slot → take == byte-identical ref) is
verified live against the in-process store for both FULL and DELTA-chain
snapshots. The code is correct, the ordering is right, every mismatch path is a
loud typed error, and the five joint tests are substantive (not tautological)
with a strong negative-case suite. I found no Critical or Important issues; a
handful of non-blocking suggestions are recorded in `02-suggestions.md`.
