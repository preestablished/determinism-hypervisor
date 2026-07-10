# Lease Reclamation Activation

Status: accepted, 2026-07-10.

## Context

The worker has token validation, optional `LeasePolicy::with_ttl` bookkeeping,
renewal, and a deterministic caller-timed `reclaim_expired(now_ms)` sweep. The
production service uses `LeasePolicy::default()` with no timeout, has no sweep
caller or public renewal RPC, and does not bind leases to a transport session.

The bridge is adopting a write-ahead lease protocol. Its residual window is a
crash after create/restore returns a lease but before the token is durably
recorded. The bridge can then name a possible slot but cannot authorize normal
`DestroyVm`. A uniform-paused/uniform-icount `NoFreeSlot` warning improves
detection of this class, but the signature can also be legitimate deterministic
fan-out and does not prove an orphan.

## Options Considered

1. Activate a daemon TTL and housekeeping sweep.
2. Reclaim leases on disconnect or explicit session teardown.
3. Add authenticated privileged tokenless destroy/reconciliation.
4. Defer all three for this release while retaining tokened `DestroyVm`.

## Decision

Choose option 4. Tokened `DestroyVm` remains the only client-invoked normal
release path for retained VM leases. Internal rollback, `VerifyReplay`
temporary-slot cleanup, and host-integrity teardown remain separate active
paths. The bridge's dangling-intent residual is operator-runbook territory;
worker restart is the recovery because it clears the in-memory slot table. The
new warning supplies detection redundancy without mutating worker state.

## Reasons

- TTL activation is not only a configuration change. Long engine operations
  cross suspension points, and `checkout_write` documents the need for
  post-suspension revalidation and per-slot serialization. The daemon also has
  no sweep loop or renewal protocol. Expiring a `Running` slot requires runtime
  actor teardown coordinated with `WorkerRuntimeTable`; enabling TTL first could
  expire live work or desynchronize the manager and runtime table.
- A transport disconnect is not a lease session boundary. Connections can
  carry unrelated concurrent RPCs, and the API defines no session identity or
  lease ownership tied to a connection. A drop handler cannot safely reproduce
  the orchestrator fake's session model.
- Tokenless destroy changes the authorization boundary. This repository has no
  authenticated reconcile mode or administrator authorization substrate. The
  bridge protocol narrows the unauthorizable residual to a rare crash window,
  which does not justify silently weakening lease credentials.
- Advisory detection improves operations while preserving the deterministic
  state machine and existing security model.

## Consequences

Production performs no automatic timeout- or disconnect-based reclamation.
Operators may see false-positive warnings for legitimate same-boundary fan-out
and must use workload context before restarting a worker. No activation
implementation bead is created by this decision. Activating options 1-3 later
requires separately scoped work and explicit operator sign-off before execution.

## Reconsideration Triggers

- Bridge evidence shows that the dangling-intent window is operationally
  material.
- An authenticated administrator/reconcile identity becomes available.
- The API gains explicit lease/session ownership and renewal.
- Runtime actor teardown becomes safe for asynchronous expiry.
