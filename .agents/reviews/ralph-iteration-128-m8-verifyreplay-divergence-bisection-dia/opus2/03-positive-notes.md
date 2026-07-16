## Positive Notes

- The `ReplayError::Divergence` to `VerifyProgress::Divergence` plumbing is cleaner with `rip_actual` carried from the replay engine.

- Adding a shared `RegDiff` type in `dh-verify` gives the postcard payload a real Rust shape instead of ad hoc bytes.

- The new RPC test covers both explicit `bisect_on_divergence` modes and validates that `reg_diff` decodes.
