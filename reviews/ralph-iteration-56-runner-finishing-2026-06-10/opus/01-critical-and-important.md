# Critical & Important Findings

## Critical

None.

## Important

None blocking. The change is correct as written. The two items below are
**observations / accuracy notes**, deliberately filed as Suggestions (02) rather
than Important because neither breaks the current single-runner reality and
neither is a defect in the diff — they are precision gaps in the doc's
forward-looking claims. They are surfaced here only so the verdict is fully
informed.

### Verified — no defects found

- **YAML semantics (checklist item 1):** CONFIRMED. `concurrency.group:
  kvm-intel-nightly-drift` has no `${{ }}` expression, so it is one global group
  spanning both triggers (`schedule` cron + `workflow_dispatch`). With
  `cancel-in-progress: false`, a manual dispatch fired while the 03:17 cron run
  is in flight QUEUES behind it rather than cancelling it — exactly the desired
  "never kill a measurement run" behavior. No collision with ci.yaml: that
  workflow's group resolves to `ci-<github.ref>` (workflow name is `ci`), a
  disjoint namespace from the static `kvm-intel-nightly-drift`.

- **Statelessness claim (checklist item 3):** CONFIRMED TRUE for the kvm-intel
  CI job. `grep -niE 'artifact|cache|baseline|upload|persist|save|lock'
  .github/workflows/ci.yaml` returns nothing. The job is checkout → toolchain →
  `cargo build --workspace` → `cargo test --workspace`. A cancelled run leaves
  no half-written artifact, cache, or baseline; the gate re-runs cleanly on the
  next push. `cancel-in-progress: true` is safe.

- **Doc vs ci.yaml fidelity (checklist item 2):** CONFIRMED. ci.yaml lines 12-16
  are `group: ${{ github.workflow }}-${{ github.ref }}` /
  `cancel-in-progress: true` with the inline comment "superseded runs are
  cancelled (keeps the single self-hosted box from queueing stale work)". The
  doc's "per-ref group with cancel-in-progress: true … keeps the box from
  queueing stale work" matches this faithfully.

- **Bead 6eb deliverables (checklist item 5):** ALL MET. Service `active` +
  `enabled`; runner `online` / `busy:false` with the four expected labels; live
  security policy (token=read, can_approve=false) matches the doc; the full
  workspace suite passes with the live-KVM legs actually executing (proving
  /dev/kvm rw for the run-as user). Nothing in the bead text is unmet. The bead
  is correctly IN_PROGRESS pending PR #1 merge.
