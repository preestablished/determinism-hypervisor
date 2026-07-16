# Positive Notes

- `.github/workflows/nightly-drift.yaml:118-149` adds the M7 canary as a KVM-gated job behind `determinism-class`, uses the documented `kvm-intel` runner, sets `DH_M7_ACCEPT_JOBS` and `DH_M7_ACCEPT_SLOT_CORES` with scheduled-run-safe fallbacks, and invokes the ignored M7 acceptance test in release mode.
- `.github/workflows/nightly-drift.yaml:219-232` correctly includes `m7-fork-verify-100` in the existing alert fan-in and updates the issue title/body so an M7 failure is not silent or mislabeled.
- `crates/dh-worker/tests/m5_net_loopback.rs:150-159` uses `run_segment_with_epoch_options` only for the manual pre-`NET_RX` landing and disables `hash_final_stop` there, matching replay's treatment of intermediate canonical-record landings.
- `crates/dh-worker/tests/m5_net_loopback.rs:236-262` keeps the real sealed tail on `run_segment_with_epochs`, preserving normal final-stop hashing for the segment that contributes `END.end_state_hash`.
- `crates/dh-worker/tests/m5_net_loopback.rs:289-314` pins the intended DHILOG partitioning: one canonical `NET_RX`, one AUX `NET_TX`, and the `NET_RX` icount one instruction after the TX doorbell.
- `crates/dh-worker/tests/m5_net_loopback.rs:395-402` keeps the high-value replay checks: applied canonical record count, epoch hash verification, end identity, and byte-identical reseal.
- `docs/ops/github-runner.md:109-114` and `docs/ops/test-partitioning.md:57` document the scheduled 100-child M7 canary and the `2-5` slot-core assumption in the two operator-facing places that matter.
