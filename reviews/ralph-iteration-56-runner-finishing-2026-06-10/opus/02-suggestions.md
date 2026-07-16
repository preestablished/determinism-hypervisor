# Suggestions (non-blocking)

## S1 — Doc's "second runner" guarantee is weaker than implied (accuracy)

`docs/ops/github-runner.md` says the concurrency groups "keep the guarantee if a
second `kvm-intel` runner is ever added." This is true for **nightly-drift**
(static group `kvm-intel-nightly-drift` → at most one nightly run anywhere, even
across two runners). It is **not** true for **ci.yaml**, whose per-ref group
(`ci-<ref>`) only serializes runs *for the same ref*. Two different refs (e.g.
two open PRs, or a PR push + a main push) land in two different groups and, with
a hypothetical second `kvm-intel` runner, could execute the KVM lane
concurrently — violating "one KVM job at a time."

Today this is purely hypothetical: there is exactly one `kvm-intel` runner, and
single-runner serialization is what actually upholds the guarantee (the doc
itself says "that serialization is automatic"). But the sentence as written
overstates ci.yaml's contribution. Consider tightening to something like: "the
single-runner serialization is what enforces one-job-at-a-time today; if a second
`kvm-intel` runner is ever added, ci.yaml's per-ref group would NOT prevent two
different refs running concurrently — switch the KVM lane to a static group (like
nightly's) at that point." This is a doc-precision nit, not a defect in the diff.

## S2 — Cancellation is silent w.r.t. the failure alert (item 4)

`nightly-drift.yaml`'s `alert-on-failure` job is gated on `if: failure()`. A
**cancelled** run is not a failure, so a cancellation would fire no alert. With
`cancel-in-progress: false` this is mostly moot — the group never cancels an
in-flight nightly; only a human can cancel it manually, and a human who cancels
already knows. The reasoning in the diff therefore holds. The only residual gap:
if a nightly is *manually* cancelled mid-measurement, there is no record/alert.
That is acceptable for an ops-initiated action, but if you want belt-and-suspenders
you could add `if: failure() || cancelled()` to the alert job (or note in the doc
that manual cancellation is silent by design). Optional.

## S3 — `.beads` directory permissions warning (unrelated to diff)

`bd` emits: `.beads has permissions 0775 (recommended: 0700)`. Not part of this
change, but worth a one-line `chmod 700 .beads` in a future housekeeping pass.
