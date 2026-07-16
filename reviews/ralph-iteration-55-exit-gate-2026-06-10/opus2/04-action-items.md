# Action Items — Second Reviewer (Claude Opus)

Verdict: **APPROVE**. No Critical items. One Important documentation fix; the
rest are Suggestions. Each item below is self-contained.

### Critical

_None._

### Important

- **A-1. Add a scope note to `docs/phase-1-exit-gate.md` covering phase-doc
  exit-gate criteria 3 & 4.** The phase doc
  (`.agents/docs/phases/phase-1-deterministic-execution.md`, "Exit gate", lines
  64–72) has FOUR criteria. This sign-off covers criterion 1 (determinism) and 2
  (landing) but is silent on criterion 3 (snapshot-store M1/M2 benchmark gates on
  synthetic data) and criterion 4 (guest-sdk agent boots in-guest, streams log
  events). Both live in **sibling repos** (`snapshot-store` is not a dep of this
  workspace; `guest-sdk` enters only as the `detguest-host`/`detguest-wire` path
  deps), and Phase 1 frames them as independent parallel tracks — so scoping this
  record to the hypervisor critical path is correct, but the doc must say so.
  Add a short paragraph, e.g.: *"Scope: this record signs off the hypervisor
  critical-path exit-gate criteria (phase-doc gate 1 & 2 + M0–M3 acceptance),
  which gate hypervisor M4. Phase-doc exit-gate criteria 3 (snapshot-store) and 4
  (guest-sdk agent) are owned by their sibling repos as independent parallel
  tracks and do not gate hypervisor M4."* This does not change the verdict — the
  M4 sequencing guard the doc invokes is the determinism guard, which IS proven.

- **A-2. Mirror the A-1 scope line into the dk1 bead.** `bd show
  determinism-hypervisor-dk1` enumerates only the hypervisor-track gate items
  (consistent with the doc, but the omission of criteria 3 & 4 is systemic). When
  A-1 lands, add a one-line note/update to dk1 so its close record is
  self-explanatory: criteria 3 & 4 are tracked in their own repos.

### Suggestions

- **A-3. Add a "what this gate does NOT cover" subsection** to the doc, with
  quotes, for: (a) cross-host-boot/cross-kernel determinism — deferred to the
  nightly drift/canary "long-baseline" job per `crates/dh-verify/src/gate.rs`
  lines 6–9 HONESTY NOTE; (b) `Until::NextSdkEvent` and `Until::FrameBudget`
  returning `RunError::NotYetWired` pending the device run loop
  (`crates/dh-vmm/src/runctl.rs` lines 9–10, 204–205) — deferred wiring, typed
  error already shipped; (c) whether PIO IN retirement is covered by the §3.1
  "PIO retires zero" assertion in `counting_semantics` or only transitively
  (`crates/dh-vmm/src/kvm.rs` lines 297–315).

- **A-4. Make the M4 sequencing guard machine-checkable.** Either state in the
  doc that M4 beads MUST reference `docs/phase-1-exit-gate.md`, or add a bd
  dependency edge (`bd dep add <m4-bead> dk1`) so "do not start M4 until dk1
  closes" shows up in `bd ready` rather than living only in closed-bead prose.

- **A-5. Note bead 6eb is a no-op close.** `bd list` shows 6eb (kvm-intel runner
  registration, P0, assignee codex) still `in_progress`, but its own notes
  confirm the runner has driven 30+ green CI runs. Add one line to the handoff so
  a future reader does not misread an open P0 runner bead as a live gate risk. It
  is not — gate row 6 depends on the runner, and the runner works.

- **A-6. Fix the stale comment in `ci/determinism-class.lock`** that says the
  nightly comparator "does not exist yet" — `ci/check-determinism-class.sh`
  exists and is invoked by `.github/workflows/nightly-drift.yaml`. Out of this
  diff's scope; file a follow-up bead if worthwhile.
