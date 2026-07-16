# Action Items

### Critical

None.

### Important

None. The diff is correct and mergeable as-is. Bead 6eb's deliverables are met
and the runner is live and online.

### Suggestions

1. **(S1) Soften the "second runner" guarantee in `docs/ops/github-runner.md`.**
   The caveat implies ci.yaml's concurrency group upholds one-job-at-a-time if a
   second `kvm-intel` runner is added. It does not — ci.yaml's per-ref group only
   serializes the *same* ref; two refs could run concurrently on two runners.
   Single-runner serialization is the real guarantee today. Reword to note that a
   future second runner would require switching the KVM CI lane to a static group
   (like nightly's). Doc-precision only; no behavior change.

2. **(S2) Decide on manual-cancellation visibility for `nightly-drift.yaml`.**
   `alert-on-failure` is `if: failure()`, so a manually-cancelled nightly fires
   no alert. With `cancel-in-progress: false` the group never auto-cancels, so
   the gap is limited to human-initiated cancellation. Optional: add
   `|| cancelled()` to the alert gate, or document that manual cancellation is
   silent by design.

3. **(S3) Housekeeping (unrelated to this diff):** `chmod 700 .beads` to clear
   the `bd` permissions warning in a future pass.
