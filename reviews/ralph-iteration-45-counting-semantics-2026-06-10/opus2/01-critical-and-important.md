# Critical and Important findings

## Critical

None. The diff is sound; the deterministic invariant (bit-identical 997)
held under every perturbation I ran, and no landing/timer code path silently
assumes the contradicted "+1 on completing resume" semantics.

## Important

### I1 — bead `gfb` (P0, the M2 acceptance) still encodes the wrong count; `0sc` does not cover it

- Location: bead `gfb` description vs `tests/nanokernel/src/lib.rs:115-145`
  (`COUNTING_DELTA_AT_OUT_EXITS`) and bead `0sc` scope.
- This diff's whole premise is that VM-exiting instructions retire **zero**,
  so the marker window is 997, not 1,000. The reconciliation bead `0sc`
  correctly targets the ARCH §3.1 prose and the boundary-engine "not yet
  retired" wording. But the **P0 acceptance bead `gfb`** — the one a future
  implementer will actually read to build the single-step test — still says
  verbatim: *"counter delta exactly 1,000; … CPUID/HLT/MMIO-exiting
  instructions retire exactly once on the completing resume."* That is the
  exact statement `d34` just measured to be false on this class.
- Why it matters: an implementer of `gfb` who codes to `gfb`'s own
  description will assert delta == 1,000 and `+1` per exiting instruction,
  watch it fail at 997 / 0, and either "fix" the test to mask the empiric or
  re-derive the contradiction `d34` already resolved — wasting the very work
  this iteration did. The well-written `COUNTING_DELTA_AT_OUT_EXITS` doc
  comment is the right knowledge, but it lives in a constant the `gfb`
  implementer may not find first.
- Note also the `gfb` description bundles **HLT** into "retire exactly once":
  the smoke does not measure HLT (it is the terminal park, outside the
  window), so the HLT half of that claim is still *unvalidated* on this class,
  not just CPUID/MMIO. The decomposition is `s=6 → e=1003` (preamble 6 +
  region 997); HLT's retirement is never observed.
- Fix: widen `0sc`'s scope (or file a child bead) to also update `gfb`'s
  acceptance criteria to "delta == COUNTING_DELTA_AT_OUT_EXITS (997 on the
  kvm-intel class); exiting instructions retire zero," and explicitly mark
  HLT retirement as a separate, still-to-be-measured case rather than
  asserting it retires once. This is a tracking/wording fix, not a code change
  in this diff — hence Important, not Critical, and not a merge blocker.
