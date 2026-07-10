# Lease Semantics Documentation And Orphan-Slot Warning

Plan name: `lease-semantics-doc-and-orphan-slot-warn`

Primary implementation bead: `determinism-hypervisor-umay` - Worker-side
orphan-slot hardening: WARN on `NoFreeSlot` with uniform paused icounts.

Planning bead: `determinism-hypervisor-zs20` - Produce this reviewed handoff.

Source request:
`.agents/requests/lease-semantics-doc-and-orphan-slot-warn/`.

## Outcome

The implementing agent should leave the repository with four coordinated
results:

1. `INTEGRATION.md`, `API.md`, and the `slot_manager.rs` module header tell the
   same truth as the production binary: token validation and a TTL-shaped reaper
   exist, but production uses no timeout, calls no reaper, and has no
   disconnect-triggered reclamation. Tokened `DestroyVm` remains the only
   client-invoked normal release path for retained VM leases; internal rollback,
   temporary-work cleanup, and host-integrity teardown remain separate.
2. Every production `NoFreeSlot` allocation surface emits one advisory warning
   when the current slot table is nonempty, entirely `Paused`, and has one
   shared icount. The warning is diagnostic only and contains no lease token.
3. `docs/decisions/lease-reclamation-activation.md` deliberately records whether
   to activate TTL, add disconnect/session teardown, add privileged tokenless
   reconciliation, or defer. This plan recommends deferral for the current
   release; see `02-contract-and-decision.md`.
4. The exact three-part real-worker/fake delta is delivered through the
   authorized phases-track/request handback channel and cited in the request
   resolution, without editing the sibling repository's implementation.

## Scope Boundary

This plan does not implement a TTL loop, a lease-renewal RPC, a transport
disconnect hook, or a destroy-by-slot-id RPC. It also does not implement the
bridge's write-ahead lease persistence/reconcile work. Those are behavior and
security changes, while this request's worker behavior change is log-only.

## File Map

- `01-current-state-and-code-seams.md` records current source anchors and risks.
- `02-contract-and-decision.md` fixes the warning contract and recommended
  reclamation decision.
- `03-implementation-sequence.md` gives an ordered edit sequence.
- `04-tests-and-validation.md` specifies focused and workspace gates.
- `05-docs-handoff-and-closeout.md` covers documentation, cross-repo handoff,
  Beads, and the mandatory push protocol.
- `06-acceptance-checklist.md` maps the request acceptance criteria to evidence.
- `07-review-correctness.md` and `08-review-implementation.md` record the two
  independent subagent reviews.
- `09-review-resolution.md` records which review findings were applied.
