# Current State And Code Seams

This plan was resolved against the working tree on 2026-07-10. The request's
older approximate line numbers must not be copied mechanically.

## Lease Authority

`crates/dh-worker/src/slot_manager.rs` is the state-machine and lease authority:

- `LeasePolicy::default()` has `ttl_ms: None`.
- `LeasePolicy::with_ttl` and `renew` implement the optional TTL mechanics.
- `validate`/`validate_entry` distinguish `StaleLease` and `LeaseExpired`.
- `reclaim_expired(now_ms)` is deterministic with respect to its explicit input,
  publishes transitions, and is deliberately single-pass.
- The manager never reads a clock and never logs.
- `SlotManager::list()` returns token-free `SlotInfo` values containing
  `slot_id`, `state`, `icount`, `base_snapshot_id`, and `live_children`.

The module header currently says that a daemon housekeeping loop owns the clock
read. No such production loop exists. `WorkerConfig::default()` in
`crates/dh-worker/src/service.rs` installs `LeasePolicy::default()`, and all
`reclaim_expired` call sites are tests.

The `checkout_write` comment is an activation blocker, not incidental prose. It
says that before TTL can be enabled, the daemon must revalidate after
await/suspension and serialize engine work per slot. A production reaper also
has to coordinate `Running -> Faulted` with the runtime actor/table; the pure
manager cannot stop or tear down a vCPU runtime.

## `NoFreeSlot` Surfaces

`SlotManager` produces `NoFreeSlot` from:

- `allocate`, when no slot is `Empty`;
- `check_fork_entries`, when fewer than the requested number of child slots are
  `Empty`.

Production service allocation reaches those errors at these seams:

- `WorkerService::install_allocated_runtime`, shared by `CreateVm` and
  `RestoreSnapshot`;
- `WorkerService::install_forked_runtimes`, at both `check_fork` and `fork`;
- `HypervisorWorker::verify_replay`, which reserves a temporary slot directly.

Wrapping only `slot_error_to_status` is insufficient because that function has
no manager reference. Wrapping only the public lifecycle methods misses
`VerifyReplay`. The implementation should add a manager-aware mapping/emission
helper and use it only at allocation/fork `NoFreeSlot` seams. Other slot errors
must preserve their current status mapping without warning.

## Warning Inputs And Race Posture

The advisory classifier can consume `SlotManager::list()`; no new
`slot_manager.rs` introspection API is required. It must require:

- the error is exactly `SlotError::NoFreeSlot`;
- the list is nonempty;
- every row is `SlotState::Paused` (therefore no `Empty`, `Running`, `Frozen`, or
  `Faulted` row);
- every row has the same `icount` as the first row.

The post-error list is a diagnostic snapshot, not part of allocation
correctness. A concurrent state change can make the warning disappear or make
it advisory-stale; it must never change the returned gRPC status or retry the
operation.

The warning condition is not proof of an orphan. Same-snapshot fan-out and fork
children can legitimately pause at a common icount. The text must say
`possible orphaned slots` and the docs must state this false-positive class.

## Logging Baseline

The workspace has no `tracing` or `log` dependency. `dh-workerd` uses
`println!`/`eprintln!`, and adding a logging framework is disproportionate to a
single advisory line. Use stderr with an explicit `WARN:` prefix, behind a
small sink seam so unit tests do not globally redirect process stderr.

Never format a `Lease` or token. `SlotInfo` is deliberately token-free.
Represent each slot as a deterministic, slot-id-ordered diagnostic entry that
includes its `slot_id` and `base_snapshot_id`; keep the shared icount once at
the top level. Per-slot base ids are important because uniform icount does not
imply a uniform base snapshot.

## Documentation Baseline

- `.agents/docs/determinism-hypervisor/INTEGRATION.md` section 1 currently says
  only that v1 leases have no timeout and `DestroyVm` releases them.
- `.agents/docs/determinism-hypervisor/API.md` defines the lease wire type and
  stale-lease status behavior but does not describe the built-but-inactive
  reclamation mechanics or absence of a disconnect trigger.
- `docs/decisions/` is the established location for accepted local decisions.
