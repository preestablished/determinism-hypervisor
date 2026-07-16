# Action Items

## Critical

- [x] No critical issues identified.

## Important

- [ ] Treat `stress-ng` as acceptance-critical: after spawning it, verify startup, check liveness before and after each cargo batch, and fail the soak if it exits before the measured run is complete.
- [ ] Reject overlapping slot and housekeeping core lists for operator acceptance runs, with an explicit override only for constrained local smoke runs.
- [ ] Add a fail-closed host/test discovery guard so the wrapper cannot count `batch_jobs` when the ignored M7 test is not present or the host is not the intended x86_64 `kvm-intel` class.

## Suggestions

- [ ] Use monotonic time for `deadline_ns`, `elapsed_ns`, and rate math instead of wall-clock `date +%s%N`.
- [ ] Print decimal jobs/second alongside millijobs/second to make operator logs easier to audit.
- [ ] Expand the docs table command to show the default slot cores, housekeeping cores, and `slot_count x 1 job/s` target.
- [ ] Mention that `DH_M7_SOAK_SECONDS` is a minimum wall-clock window and the command can run past it until the in-flight batch completes.
