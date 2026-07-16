# Critical And Important Findings

## Critical

No critical findings.

## Important

1. Missing workload-observed decision assertion.

   References:
   - `crates/dh-worker/tests/gs7_inject_replay.rs:209`
   - `crates/dh-worker/tests/gs7_inject_replay.rs:219`
   - `crates/dh-worker/tests/gs7_inject_replay.rs:248`

   The bead asks for "assert the workload-observed decisions match recorded `PIO_ANSWER` values." The new test currently checks:

   - `StreamGuestEvents` contains enough `InjectQuery` events.
   - The sealed DHILOG contains the same number of `PORT_INJECT` `PIO_ANSWER` records.
   - At least two recorded answers are distinct non-`Proceed` values.
   - `VerifyReplay` from the READY snapshot reproduces the live end hash and total icount.

   It never reads or parses an observation from the Linux workload showing what `detguest_sdk::inject_point()` returned to guest code. As a result, the gate can pass in cases where the host logs non-trivial answers and replay consumes those same answers, but the workload receives, decodes, ignores, or reports different values. The current implementation likely returns and logs the same value internally, but the acceptance gate is meant to prove the cross-repo SDK/workload contract, not just the hypervisor's internal log/replay path.

   Recommended fix: have the fixture publish the workload-observed packed decisions, keyed by inject sequence or in strict call order, through a stable SDK event/log/region. Then assert that observed packed values exactly equal the `RecordedInjectAnswer.value` sequence from the DHILOG before accepting the gate.

2. The query collection is not scoped to the post-READY segment, and `iseq` is assumed to start at zero.

   References:
   - `crates/dh-worker/tests/gs7_inject_replay.rs:110`
   - `crates/dh-worker/tests/gs7_inject_replay.rs:181`
   - `crates/dh-worker/tests/common/mod.rs:481`
   - `crates/dh-worker/tests/common/mod.rs:507`
   - `crates/dh-worker/src/service.rs:2236`

   `m9_linux_ready_snapshot()` runs until the READY SDK event and takes the READY snapshot, but it does not drain the retained SDK event backlog. `StreamGuestEvents` with an empty stream filter then selects all retained events. The GS-7 test calls it only after the one-frame post-READY run, so `queries` can include any `InjectQuery` emitted before READY, while `post_snapshot.input_log_id` describes the sealed post-READY segment.

   This makes the acceptance gate dependent on the future fixture never calling `inject_point` before READY. A valid fixture with setup-time inject points could fail the count comparison against the post-READY DHILOG. The separate `assert_contiguous_iseq()` check also requires the first selected query to have `iseq == 0`, which is stronger than the post-READY segment contract if the SDK's guest-local inject counter has already advanced.

   Recommended fix: drain or filter the event backlog at the READY boundary before starting the GS-7 segment, and assert only segment-local query evidence. If absolute `iseq` values are retained across READY, check adjacent `iseq` increments rather than requiring a zero baseline.
