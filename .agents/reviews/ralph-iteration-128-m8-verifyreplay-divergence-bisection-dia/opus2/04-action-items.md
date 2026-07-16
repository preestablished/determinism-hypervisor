## Action Items

### Critical
- [ ] [crates/dh-worker/src/service.rs:669] Replace the synthetic `at_icount.saturating_sub(1024)` range with real divergence refinement, or keep `bisect_on_divergence=true` unimplemented until that exists.

### Important
- [ ] [crates/dh-verify/src/verify.rs:42] Treat resealed-byte divergences as byte-coordinate diagnostics, not instruction-coordinate diagnostics.
- [ ] [crates/dh-worker/src/service.rs:685] Do not emit RIP `RegDiff` entries when either RIP side is an unknown zero sentinel.
- [ ] [crates/dh-worker/src/service.rs:3551] Add regression coverage that catches a divergence whose true first bad instruction lies more than 1024 instructions before `end_icount`.

### Suggestions
- [ ] [crates/dh-worker/src/service.rs:692] Avoid encoding state-hash words as pseudo-register diffs.
