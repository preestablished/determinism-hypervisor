# Suggestions

## S-1 — Generalize the literal `kvm-intel-nightly-drift` group name (angle #4)

`nightly-drift.yaml` hard-codes `group: kvm-intel-nightly-drift`. `ci.yaml` uses the parameterized `kvm-intel-${{ github.workflow }}-...` form. Today the literal is fine: it scopes the nightly to its own queue and is intentionally NOT shared with CI (different cancel policy). But the doc's framing — "keep the guarantee if a second `kvm-intel` runner is ever added" — implies the concurrency group is the cross-workflow serialization knob, and a literal per-workflow group does NOT serialize two *different* measurement workflows against each other.

If a second measurement workflow ever appears (e.g. a `canary-drift.yaml`), the operator should give it the SAME literal group (`kvm-intel-nightly-drift` or a shared `kvm-intel-measurement`) so the two cannot interleave on the single runner mid-measurement. Suggestion-level: add one sentence to the caveat, e.g.:

> If a second measurement-flavored workflow is added, put it in the *same* `concurrency.group` as `nightly-drift` (not a per-workflow group) so the two serialize against each other, since `cancel-in-progress: false` only queues — it does not cross-workflow-exclude by default.

Note: even without a shared group, the single-runner-one-job-at-a-time property still prevents *simultaneous* execution; a shared group additionally controls *queue ordering / collapse* semantics. The doc could clarify that the runner gives mutual exclusion and the concurrency group only adds hygiene — which is exactly the distinction this iteration is documenting, so it's a natural extension.

## S-2 — The inline comment in nightly-drift.yaml could name the policy asymmetry

The added comment says "never cancel in flight — this is the measurement workflow". Good. One token better: reference *why it differs from ci.yaml* inline, e.g. `# measurement workflow: unlike ci.yaml (cancel-in-progress: true), never cancel a drift run mid-measurement`. The doc already carries this, so this is purely optional polish — keeps the YAML self-explanatory for someone reading only the workflow file.

## S-3 — Consider asserting the concurrency contract in CI rather than only documenting it

Longer-horizon: a tiny lint (e.g. a test that greps `nightly-drift.yaml` for `cancel-in-progress: false` and `ci.yaml` for `true`) would prevent a future edit from silently inverting the policy the doc now promises. Out of scope for this ops bead; file as a follow-up if the team values it.
