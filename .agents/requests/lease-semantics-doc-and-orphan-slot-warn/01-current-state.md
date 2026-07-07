# Current State (Evidence-Based)

Assessed 2026-07-07 (round 3). Repo `main` at `7084456`; two open
requests precede this one (round-1 frame-caps/handoff/backstop, round-2
OOM/capture) — both explicitly excluded this cluster.

## The Engine That Exists

`crates/dh-worker/src/slot_manager.rs` (~1250 lines):

- `Lease{slot_id, token}` (16 random bytes) minted on allocate/fork;
  every mutating RPC re-validates (`validate`/`validate_entry`,
  ~:297–320); stale → `StaleLease`, expired → `LeaseExpired`.
- `LeasePolicy` (~:65–90): `default()` = no timeout ("v1 — trusted
  single orchestrator; DestroyVm releases"); `with_ttl(ms)` enables
  expiry. Header: "turning expiry on is a config change, not a code
  change."
- `reclaim_expired(now_ms)` (~:694–738): sweeps expired leases,
  releases Paused/Faulted slots with no live children, stages
  Running→Faulted for the next sweep, auto-thaws a Frozen fork-parent
  when its last child is reclaimed. Unit-tested, single-pass by design.
- Determinism by construction: the manager never reads a wall clock;
  callers inject `now_ms` (header ~:24–27).

## The Three Gaps

1. **No production caller.** `reclaim_expired` is referenced only by
   unit tests (plus `service.rs:10062` setting `with_ttl(1)` in a
   test). No housekeeping loop, no reaper, no sweep, no tick in
   `service.rs` / `runtime.rs` / `dh-workerd.rs`. `service.rs:209`
   hard-codes `LeasePolicy::default()`. The module header's promised
   "daemon housekeeping loop" does not exist.
2. **No WARN on the orphan signature.** `NoFreeSlot` raised at
   `slot_manager.rs:369`/`479`, mapped to `ResourceExhausted` + a
   `no_free_slot` metric at `service.rs:1108/1137/1152` — zero log
   statements in either file, and nothing computes the
   all-slots-paused-at-uniform-icount signal. The observed leak
   (2026-07-01, bridge restarts): 4 slots paused at identical icount
   641343512 until a worker restart.
3. **Owner-doc thin.** `.agents/docs/determinism-hypervisor/
   INTEGRATION.md` §1 (~:22–26): "Leases have no timeout in v1
   (trusted single orchestrator); DestroyVm releases." Accurate but
   silent on the existing mechanics, the unwired reaper, and the
   absence of any disconnect-triggered path. **`API.md` exists in the
   same directory** (defines the `Lease` message ~:74;
   `slot_manager.rs:99–101` cites its §2.9) — its lease section rides
   the same accuracy pass.

One more nuance the WARN must respect: **uniform paused icounts is
also a legitimate state** — determinism means N slots restored from
the same snapshot and run to the same boundary pause at identical
icounts (INTEGRATION.md §2's canonical fan-out), and fork children
inherit the parent's icount (`slot_manager.rs:416`). Nothing in
`SlotEntry` (no last-activity timestamp; no lease age under
`default()`) can distinguish legit-uniform from orphaned-uniform. The
WARN is therefore *advisory by design*.

## The Fake's Model (What w1v Needs Reconciled)

`exploration-orchestrator/crates/orch-fakes/src/hypervisor.rs:135–178`
`reclaim_session`: "reclaims every live lease as a real worker would
after observing its client connection drop … destroys leased slots
child-first, unfreezes fork-parents … so a dead orchestrator's leases
never wedge the pool." Its own doc-comment hedges: the real semantics
are "its owner doc's territory … re-verified at M6."

The mismatch is the **trigger**, not the mechanics: the fake reclaims
on client disconnect; the real worker has TTL-based reclamation that is
off-by-default and never wired, and **no disconnect hook at all**
(grep: nothing). The fake's child-first/thaw mechanics match the real
`reclaim_expired` behavior well.

## Related, Deliberately Not This Request

- **`rom-operator-bridge-72o`'s durable fix is bridge-side** (persist
  leases; reconcile orphans on startup) — their round-3 request. This
  request supplies the worker-side WARN and the semantics doc their
  reconcile design needs.
- **`rom-operator-bridge-l1w`/the OOM** is a memory-during-capture
  problem — unrelated to
  leasing despite arriving in the same incident week; folding them
  would entangle a clean P3 with a P1 investigation.
- **Phase-5-soak claims are unverified in-repo** — no phase-5 doc here;
  the honest stake is: orchestrator M6 needs the owner-doc (`w1v`), and
  any long unattended operation benefits from slots not silently
  wedging.
