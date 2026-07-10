# Review Resolution

Two independent subagents reviewed the initial plan on 2026-07-10. Both returned
`REQUEST_CHANGES`; all substantive findings were accepted.

After the edits, both reviewers performed a bounded follow-up audit and returned
`RESOLVED` with no remaining blockers in their review scopes.

## Applied Changes

- AC3 now requires actual delivery through an authorized handback channel,
  annotation of `w1v`, and a concrete reference. Missing authority leaves AC3
  and `umay` open.
- AC4 now requires the operator notice itself to be sent and cited. Only the
  response is non-blocking; a follow-up bead is not a substitute for delivery.
- Lease docs now distinguish tokened `DestroyVm` as the only client-invoked
  normal release for retained VM leases from internal rollback, temporary-work
  cleanup, and host-integrity teardown.
- The warning design now uses one sink-injected manager-aware emission/status
  core shared by tests and a thin production stderr adapter. Tests must assert
  both one emitted line and unchanged `ResourceExhausted` status.
- Closeout now uses separate implementation and resolution commits so the
  resolution can cite a real first SHA, with correction required if rebase
  rewrites it.
- Closeout now includes explicit source audits of every allocation seam, stash
  inspection, preservation of user stashes, remote pruning, and retesting after
  a material rebase.
- Warning metadata guidance now explicitly approves content-addressed base ids,
  requires structurally token-free diagnostics, injection-safe formatting,
  distinct test patterns, and slot-count-bounded line size.
- The acceptance file uses evidence statements rather than markdown task-list
  tracking; Beads remains the status authority.

## Remaining Judgment

The plan deliberately recommends deferral rather than leaving item 4 open-ended.
That recommendation is based on current code: activating TTL or tokenless
reconciliation safely requires runtime ownership, renewal/session, teardown, and
authorization contracts absent from this request. New recorded operator direction
may supersede the recommendation, but its implementation remains a separately
signed-off bead.
