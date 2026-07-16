# Positive Notes

### P-1. Fail-closed by construction across all consumers — the positive-check style pays off

Every engine gates with a *positive* equality check (`slot_state != Paused` in
`snapshot_engine.rs:116` and `restore_engine.rs:122`; `parent_state != Frozen`
in `fork_engine.rs:97`). Because of this, the new `Faulted` variant is rejected
automatically by every gate with **zero consumer edits** — a faulted slot can
never enter take_snapshot, restore_snapshot, or fork_slot. Had any gate used a
*negative* check (`== Frozen` to deny), the new variant could have slipped
through. This is exactly the discipline a determinism kernel wants, and it is
why a one-file enum addition is genuinely safe here.

### P-2. No misleading error text and no retry-suggesting language

The reject errors (`NotPaused { state }`, `ParentNotFrozen { state }`,
`EngineError::NotPaused`) are `#[derive(Debug)]` only — no `Display`/`thiserror`
prose exists, so there is no "must be Paused, retry" message that would become
wrong when `state = Faulted`. The carried `state` field surfaces `Faulted`
truthfully in the Debug output. The new `FaultedWriteDenied` doc
(`lib.rs:77-78`) explicitly says "the only exits are Destroy" — no resurrection
implied anywhere.

### P-3. Documentation ties the variant to the spec, not just the code

The enum doc (`lib.rs:47-53`) and the `can_transition` doc (`lib.rs:94-103`)
cite API.md §2.4, proto `FAULTED_S`, and `StopReason::FAULTED`, and explain
*why* each edge exists and why `Frozen→Faulted` / `Faulted→{Running,Paused}` are
absent. This is the rare state-machine change where the rationale for the
*missing* edges is written down — which is what makes S-1 a wording refinement
rather than a "where did this decision come from" investigation.

### P-4. `Faulted → Empty` as the single exit is correctly modelled

The terminality is enforced in code (only `(Faulted, Empty)` in the relation),
checked positively (`faulted_is_terminal_short_of_destroy` asserts the four
non-exits and the one exit), and matches the Destroy-only semantics
("destroy-then-restore-into-a-fresh-slot"). `Empty→Faulted` absence is also
correct: a slot with no state cannot violate a determinism contract, and slot
construction failures surface as `create_slot_vm` → `KvmError` *before* any
`SlotState` exists (`kvm.rs:121`), never as a transition. Good edge reasoning.

### P-5. Derives and proto-numbering pin are complete

`SlotState` keeps `Clone, Copy, Debug, PartialEq, Eq` and the new
`FaultedWriteDenied` variant inherits `SlotStateError`'s full
`Clone, Copy, Debug, PartialEq, Eq` derive (line 63) — so the new `assert_eq!`
on it (lines 294-297) compiles and the type stays freely copyable. On the wire
side, `dh-proto/src/lib.rs:163-168` pins `FAULTED_S == 5` and the proto comment
(`hypervisor.proto:406-408`) documents *why* the `_S` suffix and the value
ordering exist (avoiding the `StopReason` PAUSED/FAULTED C++-scoping collision).
All six `slot_state` unit tests pass locally.
