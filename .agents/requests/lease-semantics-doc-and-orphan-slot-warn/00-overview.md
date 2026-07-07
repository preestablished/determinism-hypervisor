# Request: Say What Lease Reclamation Actually Is, WARN On The Orphan Signature — And Decide The Reaper Deliberately

## Who Is Asking

The phases track, round 3 (2026-07-07), on behalf of two waiting
consumers: exploration-orchestrator bead `w1v` (needs this repo's
owner-doc statement of orphan-lease semantics to align
`FakeHypervisor::reclaim_session` at their M6) and rom-operator-bridge
bead `72o` (whose worker-side "also consider" tail this answers; the
*durable* 72o fix is bridge-side and gets its own round-3 request in
their repo). This picks up `determinism-hypervisor-umay`, which both
prior requests here explicitly carved out as "separately tracked."

## Standing Relative To The Other Two Open Requests — Read This First

This is the repo's **third** open request and sits **last**: round-1's
guest-sdk handoff (a phase-exit-gate dependency) and round-2's OOM fix
(a P1 production defect) both outrank this P3 cluster. Items 1–3 here
are freely parallelizable with both (disjoint files); item 4 should
follow the bridge's round-3 filing (`slot-lease-persistence-and-
orphan-reconcile/`, filed the same day) so their window-2 requirement
is heard — with a fallback if ordering slips (see item 4). The only
external clock is `w1v`'s M6.

Why this clears the request bar when the orchestrator's leftover beads
(same day) deliberately didn't: two named cross-repo consumers, a
doc-vs-binary contradiction in a production daemon, and a
production-activation decision needing a human — coordination and
decision content that doesn't fit a bead. Items 1–3 alone would be
bead-sized; item 4 plus the choreography is what justifies the slot.

## Why This Chunk, Why Thin

The code assessment changed the shape of this work. The lease engine is
**already built**: `crates/dh-worker/src/slot_manager.rs` has
tokened leases, `LeasePolicy::with_ttl`, and a tested
`reclaim_expired(now_ms)` reaper (child-first release, fork-parent
auto-thaw, deliberate single-pass semantics, wall-clock injected by the
caller so determinism is preserved by construction). What's missing is
smaller and stranger:

1. **The reaper has no production caller.** The module header promises
   "the daemon's housekeeping loop owns the [clock] read" — no such
   loop exists; `service.rs` hard-codes `LeasePolicy::default()` (no
   timeout). The documented design and the running binary disagree.
2. **The owner-doc is thin and now inaccurate by omission.**
   INTEGRATION.md §1 says "leases have no timeout in v1; DestroyVm
   releases" — true, but it doesn't say the reclamation mechanics
   exist-but-unwired, which is exactly what `w1v` needs to know to
   align the fake (whose trigger is *client-disconnect*, a hook the
   real worker doesn't have at all — a trigger mismatch, not a
   mechanics mismatch).
3. **The orphan signature is silent.** `NoFreeSlot` with all slots
   paused at identical icount (the observed 2026-07-01 leak: 4 slots at
   641343512) raises no log at any level — umay's "at minimum" WARN.

So the correctly-sized request is documentation + a log-only WARN + a
fake annotation, with the *activation* questions (wire a TTL reaper?
add a disconnect hook? an admin-destroy RPC?) surfaced as an explicit
recorded decision — not silently bundled into a P3, because the worker
is production-deployed and reaping live slots is a behavior change.

## The Ask In One Paragraph

Write the lease/orphan-semantics section of INTEGRATION.md to match
reality (engine + reaper exist and are tested; no housekeeping loop
calls them; `DestroyVm` is the only active release path; TTL activation
is a config decision; no disconnect-triggered reclamation exists); add
umay's WARN — on `NoFreeSlot`, when all slots are paused and their
icounts are uniform, log the orphan signature with slot ids and icount;
annotate `FakeHypervisor::reclaim_session` (via a note to the
orchestrator) as modeling a disconnect trigger the real worker doesn't
have; and record the activation decision — wire the reaper / add a
hook / defer with reasons — as a short decision doc with operator
sign-off, closing `umay` and giving `w1v` its citation.

## Files In This Request

| File | Contents |
|---|---|
| `01-current-state.md` | Evidence: the built-but-unwired engine, the doc gap, the silent signature |
| `02-requested-work.md` | The ask, acceptance criteria, out of scope |
| `03-verification-offer.md` | Cross-repo choreography (w1v, 72o) and handback |
