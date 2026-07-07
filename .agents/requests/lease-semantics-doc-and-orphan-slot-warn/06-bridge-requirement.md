# Bridge Requirement For Item 4 (Delivered At Filing, 2026-07-07)

From the bridge's round-3 request
(`../rom-operator-bridge/.agents/requests/slot-lease-persistence-and-orphan-reconcile/`),
so your activation decision (item 4) has this input regardless of
execution order:

The bridge is adopting a **write-ahead intent protocol**: an intent
record persisted *before* CreateVm/RestoreSnapshot, the full lease
record persisted when the RPC returns (clearing the intent). With that,
the bridge can destroy every orphan it has a token for. The residual
class is a **dangling intent** — crash in the microseconds between the
RPC returning and the lease record landing: the bridge knows *a slot
may exist* but holds no token.

**The requirement, precisely scoped:** not a general admin-destroy RPC —
only **destroy-by-slot-id usable when the caller can name the slot but
not the token**, gated however you like (authenticated, or
reconcile-mode-only). Frequency expectation: rare (a crash inside a
microsecond window). Your item-2 WARN provides detection redundancy for
exactly this class. If your decision lands on (d) defer, the bridge
documents the dangling-intent residual as operator-runbook territory
(worker restart clears it) — livable, but say so in the decision doc so
the residual has a named home.
