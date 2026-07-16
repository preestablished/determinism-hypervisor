## Critical

If no critical issues are found beyond the important merge blockers below, none.

## Important

- [crates/dh-worker/src/service.rs:669] `bisect_on_divergence` fabricates a bisected range. It does not replay or compare any midpoint, so for END identity divergences the true first divergent instruction can be far before `icount_lo`.

- [crates/dh-verify/src/verify.rs:42] Byte offsets are treated as instruction counts. `VerifyProgress::Divergence.at_icount` can be a byte offset for `resealed log bytes`, but `recorded_rip_hint` and `divergence_icount_range` treat it as icount.

- [crates/dh-worker/src/service.rs:685] Unknown RIP values become false register diffs. Zero is documented as unknown, but the service emits `RegDiff { name: "rip", expected: 0, actual: ... }` when actual RIP is nonzero.
