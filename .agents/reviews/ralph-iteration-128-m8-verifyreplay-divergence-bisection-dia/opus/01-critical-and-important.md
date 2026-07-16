## Critical

- [crates/dh-worker/src/service.rs:669] True bisection is not implemented. `divergence_icount_range` only returns `at_icount.saturating_sub(1024)..at_icount`; for `end_state_hash`, `at_icount` is only the segment end, so the first divergent instruction may be far outside the reported range.

## Important

- [crates/dh-worker/src/service.rs:784] `diff_page_idx` is always `Vec::new()`, while the bead and proto require page diagnostics.

- [crates/dh-worker/src/verify_replay.rs:91] RIP and `reg_diff` are not true bisection diagnostics: expected RIP is a nearest-prior hint, live RIP is captured at coarse failure time, and hash words are encoded as pseudo-registers.

- [crates/dh-worker/src/service.rs:3551] Tests would pass the fake implementation because they only assert range width and `diff_page_idx.len() <= 64`, which accepts empty diagnostics.
