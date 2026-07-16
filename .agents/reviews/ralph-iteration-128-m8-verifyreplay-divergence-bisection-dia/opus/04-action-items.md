## Action Items

### Critical
- [ ] [crates/dh-worker/src/service.rs:669] Implement actual replay bisection for `bisect_on_divergence=true`; only report `icount_lo/icount_hi` after proving the first divergent interval.

### Important
- [ ] [crates/dh-worker/src/service.rs:784] Capture expected/live memory at the diagnostic boundary and populate the first `<=64` differing page indices.
- [ ] [crates/dh-worker/src/verify_replay.rs:91] Capture expected/live RIP and real register differences at the narrowed boundary.
- [ ] [crates/dh-worker/src/service.rs:3551] Strengthen tests with a known early divergence and assertions that the reported range contains that point and page diffs are populated when pages differ.

### Suggestions
- [ ] [proto/hypervisor.proto:338] Clarify the intended default for `bisect_on_divergence`.
