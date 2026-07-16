# Action Items

## Action Items

### Critical

None.

### Important

- [ ] **Reject `fork(parent, children = 0)` before any mutation.**
  In `crates/dh-worker/src/slot_manager.rs::fork`, add an early
  `if children == 0 { return Err(...) }` guard (before taking the lock). As
  written, a zero-child fork freezes the parent (`Paused → Frozen`) with
  `live_children == 0`, and the only edge out of that state is `DestroyVm` — the
  slot can no longer run, pause, or be re-forked, so a "successful" no-op RPC
  silently discards a prepared VM. Add a regression test asserting the parent
  stays `Paused` after a rejected zero-child fork. (Detail in `01`, finding I1.)

### Suggestions

- [ ] **Add a reuse-then-orphan-destroy regression test.** The wrong-tenant
  auto-thaw is prevented solely by `force_destroy` setting cascaded children's
  `parent = None`. Add a test (`slot_manager.rs` tests module) that force_destroys
  a frozen parent, reallocates and re-forks the same slot id into a new tenant,
  then destroys the OLD orphaned child and asserts the new tenant's
  `Frozen`/`live_children` are untouched. Locks the invariant against future
  refactors. (`02`, S1.)

- [ ] **Document the `checkout_write` TOCTOU contract.** Add a doc line stating
  that the `Ok` is a point-in-time check with no lock held during the caller's
  engine call; under a `with_ttl` policy the daemon must re-validate after any
  await or serialize per slot. Safe in v1, a live race the day TTL is enabled.
  (`02`, S2.)

- [ ] **Make the `children as u32` cast explicit.** Replace the unchecked narrow
  in `fork` with `u32::try_from(children).expect(...)` or a `debug_assert!`
  documenting that `children` is already bounded by the free-slot count — the same
  silent-`as`-cast shape the module's own deny-grep polices for enums. (`02`, S3.)

- [ ] **Redact the token in `Lease`'s `Debug`.** Replace the derived `Debug` on
  `slot_manager::Lease` with a manual impl that prints `token: [redacted; 16]`, so
  a future `{lease:?}` log line at a daemon call site cannot leak the
  control-plane lease token. (`02`, S4.)

- [ ] **Trim the `reclaim_expired` fallthrough comment.** The "Paused parents
  mid-thaw race" case it describes is unreachable — a `Paused` slot always has
  `live_children == 0` (fork freezes atomically; auto-thaw sets Paused only after
  reaching zero). Reword to describe only the real case: `Frozen` with live
  children. (`02`, S5.)

- [ ] **Annotate the validate-only `transition(Empty)?` calls.** In `destroy` and
  `force_destroy`, add `// validate-only: release() sets Empty below` to the two
  sites whose `transition` result is intentionally discarded, so the dropped
  `Ok(Empty)` does not read as a bug. (`02`, S6.)
