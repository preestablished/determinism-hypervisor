# Positive notes

- **Fail-closed by construction.** `Faulted` was added to the closed `match` in
  `ensure_write_path` (`lib.rs:142-147`), so the compiler — not a reviewer —
  guarantees a fault state cannot fall through to a writable arm. The grep
  across the workspace confirms `SlotState` has exactly one definition site and
  no `_ =>` arm anywhere swallows the new variant; nothing treats `Faulted` as
  writable. Engine `NotPaused`-style checks that compare against `Paused` will
  correctly see `Faulted` as not-Paused (fail-closed), which is the right
  behavior.

- **Exhaustive, relation-pinning tests.** `transition_matrix_is_exactly_the_spec_relation`
  walks the full 5×5 grid against an explicit allow-list and asserts both
  `can_transition` *and* `transition`'s Ok/Err shape — so a stray future edge
  can't sneak in. `no_self_transitions` keeps `Faulted → Faulted` rejected, and
  `faulted_is_terminal_short_of_destroy` directly encodes the design intent
  (only `Empty` exits; no resurrection; `Frozen`/`Empty` cannot reach
  `Faulted`). This is the right way to test a state machine.

- **Every edge and non-edge is justified with a spec citation.** The doc comment
  ties `Faulted → Empty` to API.md §2.4 "slot needs Destroy/Restore" — which I
  verified appears verbatim at API.md:261 — and explicitly reasons about the two
  *deliberately absent* edges (`Frozen → Faulted`, any resurrection) rather than
  leaving them as silent omissions. Future readers won't have to reverse-engineer
  intent.

- **Correct terminal semantics.** Making `Faulted → Empty` the sole exit, with
  restore landing in a *fresh* slot, is the right call: a slot that violated a
  determinism contract has by-definition-untrustworthy state, so resuming or
  forking it would propagate corruption. The change refuses to offer a
  resurrection edge, matching the "DATA_LOSS is P0, page a human" posture in
  API.md §2.9.

- **Correctly scoped commit.** Landing the pure state-machine relation *before*
  the slot table (ol1) consumes it — and *not* bolting on a half-wired producer
  in the same diff — keeps the change reviewable and the test surface honest.
  The `INTEGRATION (not yet wired)` note (`lib.rs:137-140`) is preserved and
  still binds future call sites to adopting the guard.

- **proto numbering already pinned.** `crates/dh-proto/src/lib.rs:163-168`
  pins `SlotState::FaultedS as i32 == 5` (and the full enum), so the proto half
  of any future mirror is already frozen — only the dh-vmm→proto coupling is
  missing (see I2), not the proto values themselves.
