# Warning Contract And Reclamation Decision

## Advisory Warning Contract

Add a private, pure classifier in `crates/dh-worker/src/service.rs`, with an
equivalent shape to:

```rust
struct PossibleOrphanSlots {
    shared_icount: u64,
    slots: Vec<PossibleOrphanSlot>,
}

struct PossibleOrphanSlot {
    slot_id: u64,
    base_snapshot_id: Option<[u8; 32]>,
}

fn possible_orphan_slots(
    error: &SlotError,
    slots: &[SlotInfo],
) -> Option<PossibleOrphanSlots>;
```

Names may change to fit local style. Preserve these semantics:

- return `None` for every non-`NoFreeSlot` error;
- return `None` for an empty table;
- return `None` unless every slot is `Paused`;
- return `None` unless every icount equals the first slot's icount;
- retain rows in `SlotManager::list()` order, which is slot-id order;
- include `None` base ids explicitly as `none`/`null`, rather than omitting
  entries or claiming all slots share one base.

Add one shared manager-aware emission/status core that accepts a sink
closure/function in tests. A thin production adapter supplies `eprintln!` and
all allocation seams call that adapter. This prevents tests from exercising a
different path than production. Emit exactly one line per failed service
operation, for example:

```text
WARN: possible orphaned slots after NoFreeSlot: shared_icount=641343512 slots=[{slot_id=0,base_snapshot_id=<hex-or-none>},...]; advisory signature may also be legitimate same-boundary fan-out; leak_class=rom-operator-bridge-72o
```

Exact punctuation is not contractual. The following are contractual:

- `WARN:` and `possible orphaned slots`;
- the shared icount;
- every slot id and its full base snapshot id or `none`;
- the `rom-operator-bridge-72o` leak-class pointer;
- no token, entropy seed, guest data, or other secret;
- no change to the original `ResourceExhausted` response.

Base snapshot ids are approved content-addressed operational identifiers for
this warning, not lease credentials. Numeric fields, lowercase hex, and static
punctuation make the line injection-safe. Keep the diagnostic structs
structurally token-free and bound line size by the configured fixed slot count.

The shared core should call the emitter before delegating to the existing
`slot_error_to_status`; its unit test must assert both sink output and returned
status. Do not move logging into `SlotManager` and do not make the classifier a
precondition for returning `NoFreeSlot`.

## Recommended Activation Decision: Defer

Create `docs/decisions/lease-reclamation-activation.md` with status `accepted`
and choose option (d): retain explicit tokened `DestroyVm` as the only
client-invoked normal release path for retained VM leases in this release,
while deferring TTL, disconnect-triggered reclamation, and tokenless
destroy/reconcile. Internal rollback, `VerifyReplay` temporary-slot cleanup,
and host-integrity teardown are separate active release paths and must not be
described as client lease reclamation.

Record these reasons:

1. **TTL is not merely a configuration flip in the daemon.** There is no
   production sweep loop or public renewal protocol. Long engine operations can
   span suspension points, and `checkout_write` documents the required
   revalidation/serialization work. `Running -> Faulted` also needs runtime actor
   teardown coordination. Turning on `with_ttl` first could expire a live job or
   desynchronize `SlotManager` and `WorkerRuntimeTable`.
2. **A transport disconnect is not a lease session boundary.** The current API
   does not bind leases to a connection/session, and tonic connections can carry
   unrelated concurrent RPCs. Matching the fake requires a designed session
   identity/ownership contract, not a drop handler.
3. **Tokenless destroy changes the authorization boundary.** The bridge's
   write-ahead protocol narrows its residual to a rare crash after the create
   response but before durable token storage. This repo has no authenticated
   reconcile mode or admin authorization substrate. Worker restart remains the
   named operator recovery for that residual until such a control plane is
   designed.
4. **Detection improves without mutation.** The new warning makes the residual
   visible while preserving the deterministic worker state machine and current
   security model.

Also record reconsideration triggers:

- bridge evidence shows the dangling-intent window is operationally material;
- an authenticated admin/reconcile identity becomes available;
- lease/session ownership and renewal are added to the API;
- runtime actor teardown is made safe for asynchronous expiry.

Option (d) requires notifying the operator/work-order escalation channel but
does not require sign-off before recording the decision. If the operator gives
new direction before implementation, the agent may choose (a), (b), or (c)
instead, but must file a separate implementation bead whose execution is gated
on explicit operator sign-off. Do not implement that branch in this change.
