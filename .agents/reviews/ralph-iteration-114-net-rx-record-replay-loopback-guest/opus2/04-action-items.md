# Action Items

Required before approval: none.

Before the iteration is merged:

- Stage and commit `crates/dh-worker/tests/m5_net_loopback.rs`; it is currently untracked.

Optional follow-ups:

- Revisit exact epoch-count pinning if the count is stable across the intended KVM hosts.
- Consider adding a replay-serial assertion if `replay_segment` eventually exposes the replay rail or serial output.
