# Critical And Important Issues

## Critical

None.

## Important

### Important: Nightly M7 canary can silently skip if the runner environment inherits the local-smoke escape hatch

- File: `.github/workflows/nightly-drift.yaml:125-127`
- Related code: `crates/dh-worker/tests/m7_fork_verify.rs:245-251`

Problem: The M7 harness intentionally allows local smoke runs to skip hard prerequisites when `DH_M7_ACCEPT_ALLOW_SKIP=1`. The new nightly job sets `DH_M7_ACCEPT_JOBS` and `DH_M7_ACCEPT_SLOT_CORES`, but it does not override `DH_M7_ACCEPT_ALLOW_SKIP`. On a persistent self-hosted runner, step processes inherit the runner service environment. If this variable is ever left set in that environment, missing KVM access or an affinity/core mismatch would print a skip message, return from the ignored test, and leave the nightly canary green. That defeats the purpose of adding this as a drift tripwire and prevents `alert-on-failure` from filing the visible issue.

Suggested fix:

```yaml
    env:
      DH_M7_ACCEPT_JOBS: ${{ inputs.m7_fork_jobs || '100' }}
      DH_M7_ACCEPT_SLOT_CORES: ${{ inputs.m7_slot_cores || '2-5' }}
      DH_M7_ACCEPT_ALLOW_SKIP: "0"
```

This keeps the local-smoke escape hatch available for manual developer commands while making the scheduled canary fail closed.
