# Positive Notes

## P-1 — The as-built reconciliation is honest and accurate (verified against the real workflows)

The old doc prescribed a `concurrency` block (`group: kvm-intel-${{ github.workflow }}`, `cancel-in-progress: false`) as aspirational. The new text replaces the prescription with what is actually deployed and explains the *split*:
- `ci.yaml` → per-ref group, `cancel-in-progress: true` (stateless, safe to cancel superseded runs).
- `nightly-drift.yaml` → `cancel-in-progress: false` (never kill a measurement in flight).

I confirmed `ci.yaml:14-16` is exactly `group: ${{ github.workflow }}-${{ github.ref }}` / `cancel-in-progress: true`. The doc's "per-ref group" description is precise. The rationale (CI is stateless, the gate re-runs on next push; nightly is a measurement) is the correct reason for the asymmetry — this is good ops writing.

## P-2 — Single-runner serialization claim is true and now adequately hedged (angle #3)

Verified on the box:
- `gh api .../actions/runners` lists exactly ONE runner with the `kvm-intel` label: `infra-control-kvm-intel`, status `online`. No second `kvm-intel` runner exists.
- `~infra-admin/actions-runner-determinism-hypervisor/.runner` carries no worker-slot / parallelism field — the Actions runner protocol leases one job at a time by design; there is no multi-slot config to accidentally enable.

So "a single runner instance already runs one job at a time — that serialization is automatic" is correct, and the doc rightly frames concurrency groups as *hygiene + future-proofing if a 2nd runner is added*, not as the primary mutual-exclusion mechanism.

## P-3 — The non-ralph checkpoint commit message is provably harmless (angle #2)

This iteration adopted a pre-existing chore commit (`ops: finish runner bead 6eb ...`, NOT `ralph: iteration 56 checkpoint`). The ralph skill derives `N` from `main`'s log with:

```
git log main --format='%s' | grep -oE '^ralph: iteration [0-9]+ merge' | awk '{print $3}' | sort -n | tail -1
```

It matches ONLY the `merge` commits, anchored with `^...merge`. Checkpoint and review-fix commit messages are never parsed for `N`. The `ops:`-prefixed commit therefore cannot perturb the next iteration's number — the merge commit that the ralph merge step will create (`ralph: iteration 56 merge - ...`) is the sole anchor. The merge-commit-only assumption in the skill holds.

## P-4 — `cancel-in-progress: false` is the right default for a drift measurement

A nightly drift run that gets cancelled mid-flight on the single quiesced host would produce a torn/partial measurement (PMU counters, slot-core timing). `false` is correct — and it pairs well with the cron-only trigger (plus `workflow_dispatch`), where overlapping runs are unlikely but a manual re-dispatch during an active run would otherwise be tempted to cancel.

## P-5 — Live preflight is green

Independent of the diff: `dh-workerd --preflight` runs clean on this box (17 checks `ok`, `preflight OK`), confirming the §7.4/§2.1 host contract the runner doc describes is actually satisfied — including `dev.kvm rw`, `perf_event_paranoid=1`, `isolcpus 2-5`, `thp.mode madvise`, and the KVM cap/dirty-ring/slot-VM smoke. The runner doc and the host reality are in agreement.
