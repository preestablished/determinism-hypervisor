# Implementation Sequence

## Phase 1: Claim And Reconfirm

1. Run `bd prime`, `bd show determinism-hypervisor-umay`, and
   `bd update determinism-hypervisor-umay --claim`.
2. Inspect `git status --short --branch`; preserve unrelated user changes.
3. Re-run the source searches from `01-current-state-and-code-seams.md`. If a
   production reaper, logging framework, or new lease API has landed, update the
   decision and docs rather than forcing this plan's older premise.
4. Run the focused baseline tests listed in `04-tests-and-validation.md`.

## Phase 2: Add The Pure Classifier And Formatter

In `crates/dh-worker/src/service.rs`:

1. Import `SlotInfo` from `slot_manager` if the classifier takes rows directly.
2. Add private diagnostic structs or a directly formatted warning value. Keep
   all token-bearing types out of the diagnostic representation.
3. Implement the nonempty/all-paused/uniform-icount classifier.
4. Format base snapshot ids as stable full lowercase hex. Prefer an existing
   hex helper if one is already in scope; otherwise add a tiny local formatter
   rather than a new dependency.
5. Add one sink-injected manager-aware emission/status core. Its thin
   production adapter writes one `WARN:` line to stderr, and tests call the
   same core with a recording sink.
6. Document beside the classifier that same-boundary fan-out is a known false
   positive and that this is intentionally advisory.

Do not add a logging dependency, timestamps, random identifiers, lease tokens,
or mutable counters to this path.

## Phase 3: Wrap Every Production Allocation Error

Add a sink-injected core plus a production adapter, for example:

```rust
fn allocation_error_to_status_with_sink(
    manager: &SlotManager,
    error: SlotError,
    sink: impl FnOnce(&str),
) -> Status {
    emit_possible_orphan_warning(manager, &error, sink);
    slot_error_to_status(error)
}

fn allocation_error_to_status(manager: &SlotManager, error: SlotError) -> Status {
    allocation_error_to_status_with_sink(manager, error, |line| eprintln!("{line}"))
}
```

Use it at exactly these production seams:

1. `install_allocated_runtime`: the `manager.allocate(...)` error, covering both
   `CreateVm` and `RestoreSnapshot`.
2. `install_forked_runtimes`: errors from both `manager.check_fork(...)` and
   `manager.fork(...)`.
3. `verify_replay`: the direct temporary-slot `manager.allocate(...)` error.

Retain `slot_error_to_status` for validation, state transition, cleanup, and
rollback errors. This avoids warning on a later error merely because the table
happens to look uniform.

Watch for closure ownership: these paths already hold `Arc<SlotManager>`, so
borrow `manager.as_ref()` in the mapping closure without adding a second state
snapshot before the failed operation.

## Phase 4: Correct Source And Owner Documentation

1. In `slot_manager.rs`, replace the false claim that an existing daemon
   housekeeping loop owns the clock read. Say that any future caller must inject
   the clock and that production currently has no caller/no TTL.
2. Expand `INTEGRATION.md` section 1 with the complete operational contract from
   `05-docs-handoff-and-closeout.md`.
3. Add a concise lease-lifecycle subsection to `API.md` near the common `Lease`
   type or slot lifecycle section. Keep wire schema and runtime policy distinct.
4. Cross-link the new decision record and explain the warning's advisory
   false-positive class.
5. Create `docs/decisions/lease-reclamation-activation.md` using the accepted
   defer decision from `02-contract-and-decision.md`, unless superseded by
   recorded operator direction.

## Phase 5: Resolution And Handoff

1. Deliver the copy-ready fake-delta note through the authorized
   phases-track/request handback channel and annotate `w1v` through its owning
   workflow. Record a concrete message/request/bead reference. Do not modify the
   sibling fake implementation.
2. Notify the operator through the work-order escalation channel of the defer
   decision and worker-restart recovery. A response is non-blocking; sending the
   notice is required. Record a concrete delivery reference.
3. Run all validation gates, stage the code/docs/tests/decision changes except
   the request resolution, and create the implementation commit.
4. Capture that first commit's SHA, then append
   `.agents/requests/lease-semantics-doc-and-orphan-slot-warn/04-resolution.md`
   using the handoff contract in `05-docs-handoff-and-closeout.md`, including
   delivery references and the implementation SHA. Commit the resolution and
   Beads disposition separately so it never claims its own unknown SHA.
5. Close `determinism-hypervisor-umay` with paths and test evidence. If either
   required notice could not be delivered, leave AC3/AC4 and the bead open and
   request the missing authority/direction; a follow-up bead alone is not
   delivery. If the
   activation decision is not deferral, create the separately scoped follow-up
   implementation bead before closing.
6. Complete the mandatory Beads and Git push protocol. If the final rebase
   rewrites the recorded implementation SHA, correct and recommit the resolution
   before pushing.
