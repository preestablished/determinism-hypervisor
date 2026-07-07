# Choreography And Handback

## The Cluster's Three Repos

- **rom-operator-bridge round-3**
  (`slot-lease-persistence-and-orphan-reconcile/`): owns the durable
  72o fix. Their reconcile-on-startup design consumes this request's
  doc section (what the worker guarantees) and may want the
  admin-destroy/reconcile RPC — item 4's decision should hear their
  requirements before ruling. Whichever request resolves first leaves
  a note in the other's dir.
- **exploration-orchestrator `w1v`**: consumes item 1's doc + item 3's
  note; flips at their M6 per the bead's own text. No orchestrator
  work is requested now.

## Phases-Track Verification

1. Doc-vs-code audit: every claim in the new INTEGRATION.md section
   checked against `slot_manager.rs` (we will do this line-by-line —
   the whole point is that doc and binary stop disagreeing).
2. Both WARN unit tests re-run from a clean checkout.
3. The decision doc names its operator sign-off and, if applicable,
   the follow-up bead exists with matching scope.

## Handback Shape

Append `04-resolution.md` (doc diff pointer, WARN commit, note
delivery, decision-doc path, bead dispositions for `umay`); we respond
with `05-verification.md`.

## Contact / Tracking

- Beads: `determinism-hypervisor-umay` (this request);
  `rom-operator-bridge-72o` (bridge round-3);
  `exploration-orchestrator-w1v` (consumer, flips at M6).
- Provenance: the leak observation in
  `requests/rom-bridge-getframebuffer-region-contract/04-related-slot-leak.md`;
  the 2026-07-01 four-slots-at-641343512 incident.
