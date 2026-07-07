# Requested Work

## What We Need (Behavioral)

1. **The owner-doc section (closes `w1v`'s need).** Extend
   `.agents/docs/determinism-hypervisor/INTEGRATION.md` §1 (or a
   linked section) — and give `API.md`'s lease section (same dir,
   ~:74) the same accuracy pass — to state, accurately:
   - the lease model (token, per-RPC validation, `StaleLease` /
     `LeaseExpired`);
   - that `reclaim_expired` + `LeasePolicy::with_ttl` exist, are
     tested (child-first, fork-parent thaw, single-pass), and are
     **not wired** — v1 runs `default()` (no timeout) and `DestroyVm`
     is the only active release path;
   - that **no disconnect-triggered reclamation exists** — the
     orchestrator fake's `reclaim_session` trigger is aspirational,
     and the doc says which model (TTL vs disconnect vs both) is the
     settled direction once item 4 decides;
   - determinism posture: `now_ms` is caller-injected; reclamation
     outcomes are deterministic given the inputs.
   Fix the `slot_manager.rs` header while there — it promises a
   housekeeping loop that doesn't exist; make the comment tell the
   truth (or wire the loop under item 4, and then the comment stands).
2. **The WARN (umay's floor) — advisory by design.** Emitted from
   **`service.rs`, next to the existing `no_free_slot` metric
   mapping** (:1108/:1137/:1152 — the slot-manager module is
   deliberately logging-free "pure bookkeeping"; don't break that),
   covering all `NoFreeSlot` surfaces including the fork path (same
   signature computation, one helper). Condition: all slots paused
   with uniform icounts. Wording: "possible orphaned slots" — because
   legitimate uniform-icount states exist (same-snapshot fan-out;
   fork-inherited icounts, see `01-`), the WARN advises, it does not
   accuse. Payload: slot ids, the shared icount, `base_snapshot_id`
   (the incident's four orphans shared one base), and a pointer to
   the leak class (`72o`). Log-only; no behavior change to slot
   handling — but note honestly: **the worker has no logging framework
   at all** (no `tracing`/`log` dependency anywhere in the workspace;
   `dh-workerd` uses `println!`/`eprintln!`). So "emit one WARN" is
   also a small design decision: `eprintln!` with a `WARN:` prefix
   (consistent with the binary's current style), or introduce a
   logging crate — your call, but say which and keep it testable (a
   log-sink seam or capturing the emission in the unit test). Tests:
   fires on the signature; **silent when icounts differ**; **silent
   when not all slots are Paused** (one Running/Frozen — the clean
   discriminator). The known false-positive class is acknowledged in
   the WARN's doc-comment and the item-1 doc section.
3. **Annotate the fake (via the orchestrator).** A short note to
   `exploration-orchestrator` (their request-dir convention or a
   comment on `w1v`) carrying the *full* delta list, not just the
   trigger: (a) trigger — the fake reclaims on client disconnect; the
   real worker's reclamation is TTL-shaped and unwired, no disconnect
   hook exists; (b) sweep shape — the real `reclaim_expired` is
   deliberately single-pass (Running→Faulted staging and
   parent-thaw-then-reclaim each take a *subsequent* sweep) while the
   fake's `reclaim_session` runs a fixpoint loop emptying the pool in
   one call; (c) events — the real reaper publishes the Frozen→Paused
   thaw transition; the fake deliberately suppresses the unfreeze
   event and emits only `Empty`. Plus the doc-section pointer to align
   against at M6. Their repo makes its own edit — don't reach into it.
4. **The activation decision — recorded, not defaulted.** A short
   decision doc in `docs/decisions/` (the repo's existing convention):
   should the deployed worker (a) wire the housekeeping loop + a TTL
   (what value?), (b) grow a disconnect/session-teardown hook to
   match the fake, (c) add an authenticated admin-destroy/reconcile
   RPC, or (d) defer all three with reasons. The bridge's round-3
   request (`slot-lease-persistence-and-orphan-reconcile/`, filed the
   same day) owes you its window-2 requirement before this closes —
   read it; **fallback if ordering slips**: decide provisionally on
   the `72o` bead text alone (which already names the admin/reconcile
   option), mark the doc provisional, reopen only if their filed
   requirement conflicts. Sign-off mechanics: **operator sign-off
   gates the *implementation* of (a)–(c) (i.e., before the follow-up
   bead executes), not the decision record itself**; option (d) is
   record-with-reasons + notify the operator (the work-order
   escalation channel), non-blocking. Implementing the chosen branch
   is a follow-up bead, not this request — unless the decision is
   (d), which closes the cluster outright.

## Acceptance Criteria

(AC1↔item 1, AC2↔item 2, AC3↔item 3, AC4↔item 4.)

1. The doc section exists, matches the code (a reviewer can check
   every claim against `slot_manager.rs`), and the stale header
   comment is fixed or made true.
2. WARN implemented with all three tests (fires on the signature;
   silent when icounts differ; silent when not all slots are Paused),
   the logging-mechanism choice recorded, and `umay` closed citing it.
3. The orchestrator note delivered with the three-part delta list;
   `w1v` annotated with the doc pointer (their flip is theirs, at M6).
4. The decision doc committed in `docs/decisions/`; if (a)–(c), a
   follow-up bead filed with the chosen scope and operator sign-off
   gating its execution; if (d), reasons recorded and the operator
   notified (non-blocking); provisional status noted if the bridge's
   window-2 requirement hadn't landed yet.

## Out Of Scope For This Request

- Implementing TTL activation, a disconnect hook, or an admin RPC —
  follow-up bead per item 4's decision.
- The bridge-side 72o fix (lease persistence, startup reconcile) —
  their round-3 request; this supplies its worker-side context.
- The OOM investigation (`rom-operator-bridge-l1w`; this repo's round-2 request) — unrelated mechanism.
- Round-1/round-2 scopes — untouched.
