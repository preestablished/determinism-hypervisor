# Suggestions

### S1 — Free fns vs `impl From<SlotState> for proto::SlotState` — keep the free fns, but say why

**File:** `crates/dh-worker/src/proto_map.rs:20,36`

The idiomatic Rust shape for an infallible, exhaustive, total domain→proto mapping is
`impl From<SlotState> for proto::SlotState` (and `From<runctl::StopReason> for proto::StopReason`).
It gives the same compile-time exhaustiveness, composes with `.into()` at call sites, and is what a
reader reaches for first.

My honest judgment: **the free fns are the better call here, but the module should justify it**,
because the reason is non-obvious and load-bearing:

- The whole point of this module is to be the *single sanctioned crossing* so that a `grep` for
  forbidden `domain_enum as i32` casts has a clean allowlist. A named free fn (`slot_state_to_proto`)
  is greppable as the one legitimate site; a `From` impl scatters the crossing across every
  `.into()` call site and makes the cast-ban audit (the bead's "grep-forbid 'as i32'" intent)
  harder to mechanize.
- `From` also invites the *reverse* temptation (`impl From<proto::SlotState> for SlotState`), which
  is **not** total (proto carries `*_UNSPECIFIED = 0`, which no domain variant represents) — so a
  blanket `From` convention would push someone toward a lossy or panicking reverse impl.

Recommendation: keep the free fns, and add one line to the module doc: *"Free fns (not `From`
impls) so the cast-ban grep has a single named allowlist entry per crossing, and so the
non-total proto→domain direction is never tempted into a blanket `From`."* As written, a future
contributor will "tidy" these into `From` impls without knowing the constraint.

### S2 — The cast-ban is asserted in prose but not enforced anywhere

**File:** `crates/dh-worker/src/proto_map.rs:5-9` (module doc) — and absence in `scripts/` / CI.

The module doc states "an `as i32` cast on a domain enum is ALWAYS a bug" and the bead's extended
scope explicitly asked to "grep-forbid 'as i32' casts on SlotState." I searched `scripts/`,
`.github/`, and `docs/` — **no such grep guard exists**. The discipline currently lives only as a
comment and as the existence of this module. Until ol1 adds real callers there's nothing to
violate, but the guard is cheap to add now (a `grep -rn 'SlotState as i32' crates/ | grep -v
proto_map.rs` in the lint lane) and closes the loop the doc promises. See action items; this is
flagged as a scope-discharge gap in `04-action-items.md`, not a blocker for this iteration.

### S3 — The "lying casts == 4" pin is good, but its failure message could name the recovery action

**File:** `crates/dh-worker/src/proto_map.rs:73-74`

`assert_eq!(lying_casts, 4, "the order-divergence trap moved")` is a *meaningful* brittleness, not
noise: if the domain adds a `SlotState` variant the count shifts and the test fails, which forces a
re-audit of the mapping at the same commit — exactly the desired behavior. Keep it.

Minor: when it fires, the maintainer's correct response is usually "a new variant landed; add its
arm and update the pin," but the message ("the trap moved") reads like an alarm. Consider:
`"SlotState↔proto cast divergence changed — a variant was added/renumbered; add the match arm and
re-derive this count"`. Self-documenting failure messages are the payoff of a pin test; this one
under-delivers on that.

### S4 — Reverse conversions (proto→domain) are correctly out of scope; record that explicitly

**Files:** `crates/dh-worker/src/proto_map.rs` (whole module); bead sr5 scope.

The module only does domain→proto. The reverse (proto request enums → domain) will be needed when
ol1 wires the gRPC handlers (e.g. decoding a requested target state). That direction is **partial**
(proto `*_UNSPECIFIED = 0` has no domain counterpart and several proto `StopReason` values —
`NextSdkEvent`, `Faulted` — have no `runctl::StopReason` producer yet), so it cannot be a `From`
and must be a `TryFrom`/`Result`-returning fn that rejects `UNSPECIFIED`. That is genuinely **out
of sr5's scope** (sr5 is about the *mirror pin*, and the bead blocks ol1 which owns the request
path). No action needed this iteration — but add a one-line module note that the reverse direction
is deliberately absent and belongs to ol1, so the asymmetry doesn't read as an oversight.
