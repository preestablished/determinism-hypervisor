## Action Items

### Critical

None.

### Important

- [ ] [.github/workflows/nightly-drift.yaml:125] Force `DH_M7_ACCEPT_ALLOW_SKIP=0` in the nightly M7 job so runner-level local-smoke configuration cannot silently skip the canary.

### Suggestions

- [ ] [crates/dh-worker/tests/m5_net_loopback.rs:156] Add a comment explaining why the pre-`NET_RX` quantum disables only the non-epoch final stop hash.
- [ ] [.github/workflows/nightly-drift.yaml:147] Echo the resolved M7 job count and slot-core settings before running the canary.
- [ ] [docs/ops/test-partitioning.md:58] Show the lab-box `DH_M7_ACCEPT_SLOT_CORES=2-5` override in the full 1000-child M7 acceptance command.
