# Critical & Important findings

## Critical

None.

---

## Important

### I-1 — `restore()` leaks pre-restore `metrics`, `last_drain_error`, and `responder.hits` into a reused slot; inconsistent with the `PvBlk` precedent

**Location:** `crates/dh-devices/src/detchannel.rs`, `DetChannelHost::restore()` (lines ~210-235).

`restore()` assigns exactly the seven fields that travel in the EVTC section:

```rust
self.init_lo = init_lo;
self.init_hi = init_hi;
self.init_status = init_status;
self.inject_iseq = inject_iseq;
self.last_quiesce_ack = last_quiesce_ack;
self.channel = channel;
self.channel_gpa = channel_gpa;
self.manifest = manifest;
```

It never touches three other pieces of host state on the struct:

1. `self.metrics` (`DetChannelMetrics` — `drain_failures`, `sdk_digest_failures`, `manifest_read_failures`, etc.)
2. `self.last_drain_error` (`Option<WireError>`)
3. `self.responder` — which wraps the `FaultPlan`, and for `TableFaultPlan` that plan owns `hits: Vec<u32>`, the **per-rule occurrence counters** that drive `occurrence`-indexed injection (guest-sdk `detguest-host/src/inject.rs:85` `hits`, bumped at `:115`).

**Two cases, two different correct answers:**

- **Fork CHILD (§8.4):** the child host is built fresh with `DetChannelHost::new(...)`, so `metrics`, `last_drain_error`, and `responder.hits` all start at their `Default`/zero/empty values *before* `restore()` runs. Leaving them untouched is therefore CORRECT here — the child inherits a clean anomaly slate, which is what a fork wants. ✓
- **In-place restore of a REUSED slot (§8.3):** ARCH §8.3 / §9 (`--slots N`, slot table) reuse a slot for the next tenant. If `restore()` is called on a host that already lived a prior session, the prior tenant's `drain_failures` count, its stale `last_drain_error`, and — most insidiously — its `TableFaultPlan` occurrence hit counts **persist into the new session**. A `Some(0)`-occurrence rule that already fired for tenant A would be silently skipped for tenant B because `hits[i]` is non-zero. This is a determinism / correctness hazard, not just cosmetic: the responder's match decision depends on leaked state.

**Why this matters more given the sibling precedent:** `PvBlk` (the other DHSNAP device, `blk.rs`) treats its anomaly counter as *restorable state* — `snapshot()` writes `host_io_errors` (blk.rs:258) and `restore()` reads it back (blk.rs:303), with `restore_then_snapshot_is_byte_identical_and_keeps_host_io_errors` (blk.rs:707) asserting `host_io_errors == 7` survives a roundtrip. So the established DHSNAP convention is "host anomaly counters are part of the deterministic restorable state." EVTC neither serializes these counters (so a recording's anomaly count is silently dropped on restore — a determinism gap for the state hash if metrics ever feed it) nor resets them (so a reused slot leaks them). It does neither — it ignores them — which is the one outcome that is wrong under *both* the fork model and the blk precedent.

**Recommendation (pick one, document the choice):**
- **(a) Reset on restore** — at the top of `restore()` set `self.metrics = DetChannelMetrics::default(); self.last_drain_error = None;` and reset the responder's plan counters (add a `reset()` to `InjectResponder`/`FaultPlan`, or reconstruct the responder). This matches "restore yields a clean post-boundary slot" and is the safest for reused slots. The `responder.hits` reset is the part that actually affects replay determinism — prioritize it.
- **(b) Serialize them** — mirror `PvBlk`: add `metrics`/`last_drain_error` to the EVTC layout (bump `EVTC_LEN`/`EVTC_VERSION`) so they roundtrip byte-identically. This matches the blk precedent and preserves anomaly history across a cold restore. Note this does NOT fix `responder.hits` (the plan is reconstructed from the log at replay, per the doc comment — confirm that path re-seeds occurrence counts).
- **(c) Document the precondition** — if the integration contract is "restore is only ever called on a freshly-`new()`'d host (fork or a slot wiped between tenants)," state that explicitly in the `restore()` doc comment and add a debug-assert that `self.metrics == Default && self.responder` is fresh. This is the cheapest fix and may match reality, but it must be written down because the current code reads as if it tried to be reuse-safe (it rewrites `manifest` even though the field isn't in the wire layout, suggesting reuse-awareness) and stops short.

My adjudication: **the `responder.hits` leak is the severity driver** — metrics/last_drain_error are observability and the blk precedent makes serializing them merely a consistency nit, but occurrence counters changing a replay's inject decisions is a determinism bug if slots are ever reused in place. At minimum, option (c) must land so the reuse precondition is not left implicit. If reuse-in-place is real, option (a) for the responder is mandatory.

---

### I-2 — No fork (§8.4) breadcrumb tying EVTC restore to the same-worker CoW child path

**Location:** `restore()` doc comment (lines ~206-209) references only "§8.3 restore order."

ARCH §8.4 Tier A (same-worker fork) says the child slot "decode[s] the parent's in-memory DHSNAP" — i.e. fork is structurally `snapshot()` of the parent + `restore()` into a fresh child host, in-memory, no snapshot-store round trip. The EVTC `restore()` is exactly what runs on that path, and it works correctly there: the child's `restore()` does `Channel::attach(self.mem.clone(), gpa)` against the **child's own** `mem` handle (the CoW `MAP_PRIVATE` mapping), not the parent's — so the re-attach binds to the child's memory view. That is the right behavior and worth an explicit note so the fork bead doesn't re-derive it.

The doc currently only cites §8.3. A reviewer or the fork-implementing agent has no signal that this same code is the §8.4 hot path. **Recommendation:** add one line to the `restore()` doc — e.g. "Also the §8.4 Tier-A fork path: a fresh child host restores from the parent's in-memory EVTC bytes; `self.mem` is the child's CoW mapping, so re-attach binds to the child's view." This is a doc breadcrumb only, no code change, and it directly de-risks the downstream fork bead. (It also pairs naturally with the I-1 option-(c) note, since fork is the case where leaving metrics/responder untouched is correct.)
