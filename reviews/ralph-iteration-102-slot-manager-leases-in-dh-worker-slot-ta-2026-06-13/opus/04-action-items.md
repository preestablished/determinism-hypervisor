# Action Items

## Critical

_None._

## Important

- [ ] **Stop fabricating a state-machine error for the CoW-child fork refusal.**
  In `crates/dh-worker/src/slot_manager.rs:330-336`, the `ram_is_cow` branch
  returns `SlotError::State(InvalidTransition { from: Paused, to: Frozen })` —
  but `Paused → Frozen` is a *legal* edge, so the error misdescribes the cause
  (the real reason is the fork_engine CoW invariant, not the state machine). Add a
  dedicated `SlotError::CowChildCannotFork { slot_id }` variant and return that
  instead. Update `fork_freezes_parent_accounts_children_and_autothaws` to match
  the new variant (it currently accepts any `State(_)`), so the test proves the
  right reason. (Finding I1.)

## Suggestions

- [ ] **Clarify the `reset_slot_dirty_tracking` doc** so the ring-reset vs
  host-side-RAM-write relationship can't be misread later — add one line noting it
  resets the KVM ring and need not account restore_engine's bypassing writes
  (`slot_manager.rs:586-600`, S1). Doc only.
- [ ] **Add a single-pass invariant note to `reclaim_expired`** stating that the
  Running→Faulted→free and Frozen-thaw→free handoffs rely on the sweep being
  single-pass ascending, so a future fixpoint-loop refactor doesn't silently break
  the documented "next sweep frees it" contract (`slot_manager.rs:503-530`, S2).
- [ ] **(Optional) Use `saturating_add` for `live_children += children as u32`**
  to mirror the `saturating_sub` in `release()` and keep the accounting symmetric
  (`slot_manager.rs:367`, S3). Cosmetic; cannot realistically overflow today.
- [ ] **Add a one-line comment to the two `transition(Empty)?` gate calls** in
  `destroy`/`force_destroy` explaining the result is discarded on purpose
  (gate-only; `release()` resets the entry), to prevent a future "fix"
  (`slot_manager.rs:443`, `:459`, S4).
- [ ] **(No change, note only) `urandom_token` panics on unreadable
  `/dev/urandom`.** Acceptable for v1 grant-at-allocate; revisit only if lease
  minting ever moves onto a request path where a panic would down the daemon
  (`slot_manager.rs:565-572`, S5).
- [ ] **(Open question) Consider whether `default_slot_count` / `parse_core_list`
  belong next to `preflight`** rather than in `slot_manager`, to avoid two homes
  for core/slot config once the daemon (rfv) lands its config reader
  (`slot_manager.rs:160-187`, S6).
