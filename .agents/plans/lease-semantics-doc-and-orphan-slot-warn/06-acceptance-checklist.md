# Acceptance Evidence

These are evidence requirements, not a parallel task tracker. Beads remains the
authority for work status.

## AC1: Owner Documentation Matches Code

Required evidence:

- `INTEGRATION.md` contains the complete active/inactive lease contract.
- `API.md` contains an accuracy pass without promoting internals to wire
  guarantees.
- The `slot_manager.rs` header no longer claims an existing housekeeping caller.
- Every claim was checked against current source after implementation.

Record doc paths, source anchors, and documentation-audit output in request
`04-resolution.md`.

## AC2: Advisory Warning And Tests

Required evidence:

- Warning is emitted in `service.rs`, not `slot_manager.rs`.
- `CreateVm`, `RestoreSnapshot`, `Fork`, and `VerifyReplay` allocation exhaustion
  are covered through their shared/direct service seams.
- Condition is nonempty + all `Paused` + uniform icount after `NoFreeSlot`.
- Payload has all slot ids, shared icount, per-slot base snapshot context,
  `rom-operator-bridge-72o`, and no tokens.
- Wording is advisory and acknowledges legitimate fan-out.
- The shared sink-injected production core is tested, and the original
  `ResourceExhausted` status is unchanged.
- Required emit/differing-icount/not-all-paused tests pass.
- Logging choice (`eprintln!` with `WARN:` and a sink seam) is recorded.

Record focused test output, source-audit line numbers, and the service diff in
`04-resolution.md`.

## AC3: Fake Delta Is Delivered

Required evidence:

- The delivered note contains trigger, sweep-shape, and event deltas.
- It links the owner docs and decision record.
- `w1v` is annotated through its owning workflow.
- A concrete authorized handback delivery reference is recorded.
- No unauthorized sibling implementation edit was made.

A local draft is not delivery. If authority/channel access is unavailable,
leave AC3 and `determinism-hypervisor-umay` open and request direction.

## AC4: Activation Was Deliberately Decided

Required evidence:

- `docs/decisions/lease-reclamation-activation.md` exists with status, context,
  considered options, decision, reasons, consequences, and reconsideration
  triggers.
- Bridge dangling-intent residual and worker-restart recovery are named.
- For deferral, the operator notice was actually sent and has a delivery
  reference; only the response is non-blocking.
- If activation/admin work was selected instead, a follow-up bead exists and
  explicitly requires operator sign-off before execution.
- No activation branch was silently implemented in this scope.

## Completion Evidence

Required evidence:

- Focused and workspace quality gates completed truthfully.
- `determinism-hypervisor-umay` closed only after AC1-AC4 are satisfied.
- Remaining work has Beads tracking.
- Implementation and resolution commits, plus Beads state, pushed successfully.
- Task-created stashes are cleared, remote refs are pruned, and user stashes are
  preserved.
- Final Git status is clean or contains only pre-existing unrelated changes and
  reports up to date with upstream.
