## Critical Issues

No critical issues found.

## Important Issues

### Important: Fork entropy wording overstates the implemented API contract

Path: `docs/phase-2-exit-gate.md:44`

The fork architecture notes say "Each child gets an explicit entropy seed and a fresh DHILOG segment." That is accurate for the M7 harness, but it is not the general fork contract. `crates/dh-worker/src/fork_engine.rs` documents and implements optional child entropy: when no nonzero seed is provided, the child continues the fork-point ENTR stream; an explicit seed starts a fresh deterministic PRNG stream. The close-out record should not make optional API behavior sound mandatory.

Suggested fix:

```md
- **Fork:** tier-A fork starts from a frozen parent and creates children
  through CoW memory plus in-memory DHSNAP restore. A parent cannot run
  while children live; a child is not a new fork parent. The service opens
  a fresh DHILOG segment for each child. Child entropy continues from the
  fork-point ENTR state unless the caller supplies an explicit nonzero
  segment seed; the M7 harness supplies explicit per-child seeds.
```
