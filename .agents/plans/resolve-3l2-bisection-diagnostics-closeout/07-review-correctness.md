# Correctness Review

Reviewer: Mendel

## Findings

1. The original icount-range proof was worded too loosely. Saying the range
   must be "no narrower than" checkpoint evidence could allow a range wider
   than the evidence. The plan now requires exact selected coverage for epoch
   divergence and terminal upper bound equal to `end_icount`.

2. Hardware-gated validation could falsely pass if tests self-skip. The plan
   now runs KVM-backed service tests with `DH_REQUIRE_KVM_TESTS=1` and
   `-- --nocapture`, and treats skip output as a closeout failure.

3. The plan omitted the positive checkpointed VerifyReplay test. It now
   includes `verify_replay_rpc_streams_done_for_bisection_checkpoint_log` to
   prove checkpoint evidence does not break matching replay.

4. Field-population validation needed lower-level snapshot comparison tests.
   The plan now includes tests for RIP mismatch postcard `reg_diff`, page hash
   mismatch reporting, and the first-64 page-diff limit.

5. `bisect=false` coarse behavior needed explicit expectations. The plan now
   requires equal `icount_lo`/`icount_hi`, zero RIP fields, empty evidence-only
   fields, and `coarse:` provenance.

## Assessment

With the accepted edits, the plan can prove the parent acceptance instead of
only confirming the child beads are closed.
