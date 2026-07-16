# Iteration 74 — Restore Engine (tier B) — Second Review

- **Branch:** `ralph/iteration-74-restore-engine-tier-b`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus (2nd reviewer)
- **Stats:** 6 files, +1035 / -0, 1 commit

## Summary

This iteration adds `restore_snapshot` (bead 9wa, ARCH §8.3): it fetches the
manifest and the server-flattened page set from the real snapshot-store,
materializes guest RAM through the slot's live mapping, restores devices,
then the vCPU last, and re-seeds the segment clocks (PvClock `vns_base`,
hash chain `from_value`, optional perf-counter reset). The capture/restore
symmetry is sound for every device that is actually a `DetDevice` on the bus
(PvClock, PvPad, PvEntropy, DebugSerial, PvBlk): each device restore strictly
checks both `sec_version` and byte length, the container codec rejects
duplicate tags so the `5 + consumed` shape arithmetic is robust, the ENTR v2
device-version split is fed `device.restore(regs, 1)` correctly, and the
server-side flatten guarantees full page coverage (verified against the
snapstore server handler and ARCH §8.3). The fixed-point property
(take → restore → take ≡ identical ref) is exercised live for both FULL and
DELTA chains. The implementation is careful, well-documented, and fail-closed.

The findings are not blocking. The most substantive issue is **documentation
drift**: the module doc repeatedly asserts that the RAM-first ordering is
load-bearing *because* `DetChannelHost`'s (EVTC) restore re-attaches against
live guest RAM — but `DetChannelHost` does not implement `DetDevice` (its
`restore` takes an extra generic `plan: P` argument and the type is generic
over `M`/`P`), so it cannot currently be a `Box<dyn DetDevice>` on the
`MmioBus` and this engine cannot drive it. The RAM-first ordering is still
correct and worth keeping, but its stated justification references a device
the engine cannot reach, which is a maintainability trap for a future reader.
Two smaller latent issues (slot-reuse dirty-ring staleness; a two-entropy-device
shape-check gap) are noted as suggestions.

## Verdict

**APPROVE** — with documentation corrections recommended (see 01) and minor
hardening suggestions (see 02). No correctness defect blocks merge.
