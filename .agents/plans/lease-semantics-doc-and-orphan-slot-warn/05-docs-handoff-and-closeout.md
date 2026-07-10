# Documentation, Cross-Repo Handoff, And Closeout

## Owner-Doc Content

`INTEGRATION.md` section 1 is the operational owner statement. It must say all
of the following without implying inactive code runs in production:

- `CreateVm`, `RestoreSnapshot`, and `Fork` mint a random 16-byte token tied to
  a slot; mutating RPCs validate the slot/token pair.
- A wrong/replaced token maps to `StaleLease`; a matching token past an enabled
  TTL maps to `LeaseExpired`; both are `FAILED_PRECONDITION` at gRPC.
- `LeasePolicy::with_ttl`, renewal bookkeeping, and
  `reclaim_expired(now_ms)` exist and are unit tested.
- The reaper is single-pass: expired `Running` becomes `Faulted` before a later
  sweep releases it; a frozen parent thawed by its last reclaimed child is
  eligible on a later sweep. Thaw transitions are published.
- Time is caller-injected, so outcomes are deterministic for a given slot table
  and `now_ms`.
- Production installs `LeasePolicy::default()` (no TTL), has no housekeeping
  caller, exposes no renewal RPC, and performs no disconnect-triggered
  reclamation.
- Consequently, tokened `DestroyVm` is the only client-invoked normal release
  path for retained VM leases. Internal rollback, temporary-work cleanup, and
  host-integrity teardown are separate.
- The `NoFreeSlot` uniform-paused/uniform-icount warning is advisory because
  legitimate deterministic fan-out can have the same shape.
- Link `docs/decisions/lease-reclamation-activation.md` for the deliberate
  activation decision.

`API.md` should carry the concise externally relevant subset: validation/status
semantics, no active timeout/disconnect behavior, tokened `DestroyVm` as the
current client-invoked normal release, and a link to the fuller owner
section/decision. Do not present internal `reclaim_expired` as a wire guarantee.

## Exploration-Orchestrator Consumer Note

The resolution must contain the delivered note for
`exploration-orchestrator-w1v` with all three deltas:

1. **Trigger:** `FakeHypervisor::reclaim_session` reclaims on simulated client
   disconnect. The real worker has only an inactive TTL-shaped mechanism, no
   production sweep, and no disconnect hook.
2. **Sweep shape:** real `reclaim_expired` is intentionally single-pass.
   `Running -> Faulted -> Empty` and last-child thaw followed by parent reclaim
   require later sweeps. The fake runs a fixpoint loop and empties the session's
   pool in one call.
3. **Events:** the real reaper publishes the `Frozen -> Paused` parent-thaw
   transition. The fake suppresses that unfreeze event and emits only `Empty`.

Include exact repository-relative links to the updated `INTEGRATION.md` section,
`API.md` subsection, and decision record. State that the fake is useful for M6
but intentionally models an aspirational cleanup trigger and stronger one-call
completion than the deployed worker.

The source request explicitly says the orchestrator owns its implementation
edit. Send the note through the authorized phases-track/request handback channel
and annotate `w1v` through its owning workflow; do not edit the sibling fake.
Record a concrete delivery reference in the resolution. A local copy-ready note
without delivery evidence does not satisfy AC3. If the agent lacks authority or
a channel, leave AC3 and `umay` open and request direction.

## Bridge Residual And Operator Notice

The decision record must name the bridge's remaining dangling-intent window:
the create/restore RPC returned a lease, but the bridge crashed before durably
recording its token. With deferral, the operator recovery remains worker restart;
the new warning supplies detection redundancy but no automatic cleanup.

Notify the operator/work-order escalation channel of the accepted deferral and
named recovery, then cite the delivery in `04-resolution.md`. The operator's
response is non-blocking, but the send itself is required by AC4. Do not claim
notification happened without evidence. If no external-write authority/tool is
available, leave AC4 and `umay` open and request direction; a Beads follow-up is
useful tracking but is not a substitute for delivery.

## Request Resolution Shape

Create
`.agents/requests/lease-semantics-doc-and-orphan-slot-warn/04-resolution.md`
containing:

- first implementation commit id (the resolution is a second commit);
- owner-doc and decision-record paths;
- warning mechanism and the exact production seams wrapped;
- focused/workspace test commands and results;
- the copy-ready orchestrator note;
- bridge residual and operator-notification status;
- `determinism-hypervisor-umay` disposition;
- any follow-up bead id, or an explicit statement that accepted deferral creates
  no activation implementation bead.

## Beads And Git Protocol

1. Create Beads issues for every remaining repository-local follow-up; do not
   use markdown TODO lists as task tracking.
2. Close `determinism-hypervisor-umay` only after its warning, docs, decision,
   tests, and resolution are complete.
3. With a clean worktree before edits, synchronize using `git pull --rebase`.
   If the branch contains intentional local commits, inspect their relationship
   to upstream first and preserve them.
4. Inspect `git diff` and `git status`; stage only intended implementation,
   tests, docs, and decision files, excluding `04-resolution.md`.
5. Commit the implementation with a message such as
   `worker: document lease semantics and warn on orphan signature`.
6. Record that commit SHA in `04-resolution.md`, then commit the resolution and
   issue disposition separately.
7. Follow the repository protocol exactly:

   ```bash
   git pull --rebase
   bd dolt push
   git push
   git status
   ```

8. If the final pull/rebase changes tested code, rerun the affected focused and
   workspace gates. If it rewrites the first commit SHA, correct and recommit
   the resolution before pushing.
9. Inspect `git stash list`; remove only stashes created by this task and never
   drop user stashes. Run `git remote prune origin` and then the final
   `git status --short --branch`.
10. Resolve pull/rebase or push failures and retry until status reports the
    branch is up to date with its upstream. Preserve unrelated worktree changes
    and never use destructive reset/checkout cleanup.
