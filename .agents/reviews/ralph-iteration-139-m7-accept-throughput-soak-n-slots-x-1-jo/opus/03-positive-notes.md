# Positive Notes

- `ci/m7-throughput-soak.sh:80` validates the duration, target, batch size, and core-list syntax before doing any long-running work, which is the right shape for an operator-run acceptance.
- `ci/m7-throughput-soak.sh:100` derives the default target from the configured slot count, so changing `DH_M7_SOAK_SLOT_CORES` does not silently keep a stale fixed threshold.
- `ci/m7-throughput-soak.sh:125` runs `cargo test --no-run` before the timer starts, keeping compile time out of the throughput measurement.
- `ci/m7-throughput-soak.sh:153` sets `DH_M7_ACCEPT_ALLOW_SKIP=0` for each measured batch, which prevents a `/dev/kvm` or affinity prerequisite failure from becoming a skipped-test false green.
- `ci/m7-throughput-soak.sh:146` runs full batches until the deadline has been reached, then computes throughput from actual elapsed time. That avoids pretending the run ended exactly at `DH_M7_SOAK_SECONDS` when the last batch naturally overruns.
- `docs/ops/github-runner.md:97` correctly frames `stress-ng` as operator-exercised by this soak rather than as scheduled nightly coverage.
- `docs/ops/test-partitioning.md:59` adds the soak to the lab-box run table, making the M7 phase-exit command discoverable next to the other hardware-gated gates.
