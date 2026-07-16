# Suggestions

## S1 — `inject_at_boundary` should assert the caller actually landed at `at`

**File:** `crates/dh-vmm/src/inject.rs:113-120`.

`at: &Boundary` is taken but only its `icount`/`rip` are *recorded* — the function never
checks that the vCPU is actually at that boundary (`counter.read() == at.icount`). A
caller that lands at B, then runs the guest, then calls `inject_at_boundary(at=B)` would
silently inject at the wrong point and record a stale `requested_icount`/`delivered_icount`
— a determinism-relevant lie in the AUX record. A one-line guard makes the precondition
self-enforcing:

```rust
let c = counter.read().map_err(|e| InjectError::Boundary(BoundaryError::Counter(e)))?;
debug_assert_eq!(c, at.icount, "inject_at_boundary called off the landed boundary");
```

Cheap, read-only, and matches the boundary engine's "loud on stale target" discipline
(boundary.rs Overshoot).

## S2 — document the no-irqchip `KVM_INTERRUPT` overwrite semantics in the module/`queue_interrupt` doc

The module doc explains delivery timing well but never states that, without an in-kernel
irqchip, a second `KVM_INTERRUPT` before an entry *overwrites* the pending vector rather
than failing (see I1). A reader could reasonably assume the kernel serializes/rejects.
One sentence on `queue_interrupt` ("KVM overwrites any prior pending vector; the caller
must enter the guest between injections") prevents a future determinism bug. Pairs with
the I1 fix.

## S3 — `injectable()` could note why `nmi.masked`/`smi` are intentionally not checked

The predicate checks `if_flag`, `ready_for_interrupt_injection`, `exception.pending`,
`exception.injected`, and `interrupt.shadow`. That is the correct and complete gate for
**maskable external interrupt** delivery via `KVM_INTERRUPT`:

- `nmi.masked` gates NMIs, not maskable IRQs — irrelevant to `KVM_INTERRUPT`. Correctly
  omitted.
- `smi` / SMM: the determinism design never enters SMM (no SMI source exists in this
  hypervisor), so it cannot gate here. Correctly omitted.

The omissions are *right*, but a one-line comment ("NMI/SMI masking do not gate
`KVM_INTERRUPT`; this hypervisor has no NMI/SMI source") would preempt exactly the
"shouldn't you also check nmi.masked?" review question and document the judgment. Worth a
sentence given how load-bearing the predicate's purity is to the determinism claim.

## S4 — `max_defer_steps` budget has no documented derivation; consider tying it to a §3.x constant

**File:** `crates/dh-vmm/src/inject.rs:111`, test uses 250 and 16.

The budget is a bare caller-supplied `u64`. For determinism it doesn't matter (same guest
+ same count → same step count), but for the *loud-failure* contract (`WindowNeverOpened`)
the number that distinguishes "guest is legitimately deferring across a long critical
section" from "guest will never accept this interrupt" deserves a documented basis — e.g.
a `MachineConfig` field analogous to `Margins`, or at least a doc note that run control
must size it to the longest expected interrupt-disabled region. As written, a too-small
budget turns a slow-but-valid guest into a deterministic false failure. Not a bug; a
config-hygiene gap to capture before this gets wired into run control.
