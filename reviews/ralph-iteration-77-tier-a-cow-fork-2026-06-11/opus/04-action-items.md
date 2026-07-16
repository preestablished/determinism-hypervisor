# Action Items

### Critical

_None._

### Important

_None._

### Suggestions

- [ ] [crates/dh-vmm/src/kvm.rs:168] Document the `MAP_NORESERVE` posture in the
  `fork_slot_vm` doc: commit is deferred, so a CoW write fault under memory pressure can
  `SIGBUS` the child mid-run (asymmetric vs the tier-B path). Note that per-host child-count
  admission control (slot manager, bead ol1) owns the cap.
- [ ] [crates/dh-worker/src/fork_engine.rs:176] File a bead to decide and pin fork-of-fork
  behavior: a child's `guest_mem` retains a backing memfd (parent's sealed clone), so
  `fork_slot_vm(child)` passes the kernel seal check. Decide whether grandchildren are
  intended (add a test) or denied, and document; the `fork_slot` layer's `Frozen` guard
  blocks it in practice but `fork_slot_vm` alone does not.
- [ ] [crates/dh-worker/src/fork_engine.rs:138] Add a bead note that the five `ForkError`
  variants need distinct gRPC status codes when bead ol1 wires the RPC (precondition-failed
  for `AgendaNotEmpty`/`ParentNotFrozen` vs internal for `Kvm`/`Apply`/`Capture`), so callers
  can distinguish "retry at a boundary" from "child is scrap."
- [ ] [crates/dh-vmm/src/kvm.rs:156] Consider a structured `KvmError` variant (e.g.
  `ParentNotSealed`) instead of the free-text "UNFROZEN parent" message that the fork test
  matches by substring, so the test asserts on the error *kind* rather than a human string.
- [ ] [crates/dh-worker/src/fork_engine.rs:176] Add a one-line precondition note that
  `child_bus` must be a freshly-built bus matching the parent's machine shape (mirroring the
  "slot must be FRESH" note at restore_engine.rs:105-110); the shape checks in `apply_dhsnap`
  enforce it, but the contract should be stated at the entry point.
