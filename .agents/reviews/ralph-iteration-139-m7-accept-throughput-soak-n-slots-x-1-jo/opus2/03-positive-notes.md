# Positive Notes

- `ci/m7-throughput-soak.sh:125` builds the M7 test binary before starting `stress-ng`, so compile time is excluded from the measured throughput window.
- `ci/m7-throughput-soak.sh:43` through `ci/m7-throughput-soak.sh:108` validates positive integer inputs and rejects malformed or duplicate core-list components before running the long soak.
- `ci/m7-throughput-soak.sh:153` through `ci/m7-throughput-soak.sh:156` sets `DH_M7_ACCEPT_ALLOW_SKIP=0`, which correctly makes missing KVM or unavailable slot cores fail on the intended x86_64 test path instead of becoming a local skip.
- `ci/m7-throughput-soak.sh:133` through `ci/m7-throughput-soak.sh:139` installs cleanup for normal and interrupted exits, avoiding orphaned `stress-ng` load in the common failure paths.
- `docs/ops/github-runner.md:97` through `docs/ops/github-runner.md:103` accurately positions `stress-ng` as operator-exercised by this soak rather than implying scheduled workflow coverage.
- `docs/ops/test-partitioning.md:59` adds the M7 throughput soak to the hardware-gated runbook table, which is the right place for an operator-run acceptance command.
