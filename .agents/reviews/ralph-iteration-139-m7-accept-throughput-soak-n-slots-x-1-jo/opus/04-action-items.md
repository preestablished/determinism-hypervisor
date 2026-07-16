# Action Items

## Critical

- [ ] In `ci/m7-throughput-soak.sh`, verify `stress-ng` starts successfully before `start_ns`, fail if it exits before the measured deadline, and stop ignoring its `wait` status. Without this, the soak can pass without the required housekeeping load.

## Important

- [ ] In `ci/m7-throughput-soak.sh`, validate the configured topology before launching the soak: slot cores and housekeeping cores must be disjoint, and the housekeeping core mask must be valid in the current process cpuset.
- [ ] In `ci/m7-throughput-soak.sh`, split the `EXIT` trap from `INT`/`TERM` traps so external interruption both cleans up `stress-ng` and terminates the script with a signal-like nonzero status.
- [ ] In `docs/ops/test-partitioning.md`, update the introduction so it no longer says every listed gate is part of `cargo test --workspace` or self-skips; the M7 throughput soak is a standalone operator-run script with skip disabled.

## Suggestions

- [ ] In `ci/m7-throughput-soak.sh`, move final rate math to Python or another non-overflowing calculation path before this becomes a template for longer/larger soaks.
- [ ] In `ci/m7-throughput-soak.sh`, either include `required_jobs_at_elapsed` in the failure message or remove the logged value to avoid unused-threshold confusion.
- [ ] In `ci/m7-throughput-soak.sh`, print child concurrency and explicitly note that one configured slot is reserved for the reusable root parent.
- [ ] In `ci/m7-throughput-soak.sh`, print `stress-ng`, `taskset`, and `cargo` versions in the acceptance transcript.
