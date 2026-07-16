# Critical & Important Findings

## Critical

**None.** No correctness, safety, or security defects were found. The fuzz target
drives exactly the surface that must be total, the workspace isolation is verified,
and the CI failure routing fires correctly on a scheduled fuzz-found crash.

## Important

**None that block merge.** The two items below are the closest things to
"important," but both are intentional, documented design choices that work as
written — recorded here only because the review brief asked them to be cross-checked.

### (Non-blocking, verified-correct) `runs-on` label degrades from 2 labels to 1 on dispatch

`.github/workflows/nightly-drift.yaml:79`

```yaml
dhilog-fuzz:
  runs-on: ${{ inputs.fuzz_runner || 'ubuntu-latest' }}
```

The two existing self-hosted jobs target the runner with a two-label array
`runs-on: [self-hosted, kvm-intel]`. When an operator dispatches with
`-f fuzz_runner=kvm-intel`, this job resolves to the single string label
`kvm-intel` — it omits the `self-hosted` label.

This is NOT a bug for the documented setup: GitHub matches a runner when it carries
ALL requested labels, and a single requested label `kvm-intel` is a subset of the
runner's label set, so the lab box still matches. It only becomes wrong if a *second*
runner ever also advertises `kvm-intel`. Given there is exactly one such runner
(the workflow's whole concurrency story is built on "the single kvm-intel runner"),
this is fine today.

Optional hardening for symmetry with the other jobs (do NOT block on this):

```yaml
# would require fuzz_runner to be the string "kvm-intel" and then:
runs-on: ${{ inputs.fuzz_runner == 'kvm-intel' && fromJSON('["self-hosted","kvm-intel"]') || 'ubuntu-latest' }}
```

That expression is uglier than the value it protects; I'd leave the current form and
just be aware of the single-runner assumption. Filed as a suggestion, not a change-request.

### (Non-blocking, verified-correct) hosted 6h cap silently truncates a misconfigured 24h run

`.github/workflows/nightly-drift.yaml:80-83` (`timeout-minutes: 1500`) +
the `-max_total_time=${{ inputs.fuzz_seconds || '3600' }}` arg.

If an operator dispatches `fuzz_seconds=86400` but forgets `fuzz_runner=kvm-intel`,
the job lands on `ubuntu-latest`, GitHub's hard 6h cap kills it at ~21600s, and the
job ends as a *cancelled/failed* run after 6h — which then trips `alert-on-failure`
and files a "nightly-drift FAILED" issue even though nothing crashed. The inline
comments and the dispatch input descriptions already warn about the 6h cap, so this
is a documented operator footgun rather than a code defect. Mentioned here only so a
future reader knows the failure mode. No change required.

---

For the genuinely optional improvements (cargo-fuzz install caching, a `splice`
follow-up target, an assertion that the seed corpus is non-empty), see
`02-suggestions.md`.
