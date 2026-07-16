# Critical & Important Findings

## CRITICAL

None. The sign-off's load-bearing claim — the hypervisor determinism + landing
gates are green, which is what the M4 sequencing guard actually requires — is
correct and re-verifiable. No false statements found.

---

## IMPORTANT

### I-1. The sign-off is silent on two of the FOUR phase-doc exit-gate criteria

`.agents/docs/phases/phase-1-deterministic-execution.md` "Exit gate" section
lists four numbered criteria (lines 64–72):

1. Determinism gate (run-to-N twice + injected-timer, 100 runs, zero divergence).
2. Landing-precision gate (10,000 targets exact, incl. REP boundaries).
3. **snapshot-store M1/M2 benchmark gates met on synthetic data** (≥1.5 GB/s
   fast-path ingest, manifest round-trip property tests green).
4. **guest-sdk agent boots in-guest and streams log events host-ward.**

The sign-off table maps cleanly onto criteria 1 and 2 (and additionally pulls in
M2's skid gate, M3's accepts, the CI-required regression, and the TSC decision —
all good). **Criteria 3 and 4 appear nowhere in the document** — not in the
table, not in a "what this gate does NOT cover" note, not in the "What this
unblocks" section.

Why this matters: the very first line of the doc says *"Every item below was
re-run LIVE on this date for this sign-off"* and the doc titles itself the
"Phase 1 exit gate — sign-off record." A reader holding only the phase doc and
this record cannot distinguish "criteria 3 and 4 are owned by sibling repos and
out of scope for this repo's gate" from "criteria 3 and 4 were forgotten."

What is actually true (I verified): `snapshot-store` is **not** a workspace
member or path dep of this repo, and `guest-sdk` enters only as the
`detguest-host`/`detguest-wire` path deps (`Cargo.toml` workspace deps). Phase 1
explicitly frames snapshot-store and guest-sdk as **independent parallel tracks**
(phase doc lines 29–44, 51–61, "Four independent tracks"). So scoping this
sign-off to the hypervisor critical path is correct. But the doc must *say so*.

**Recommended fix (one short paragraph in the doc):** add a scope note, e.g.:

> *Scope: this record signs off the **hypervisor critical-path** exit-gate
> criteria (phase-doc gate 1 & 2 + M0–M3 acceptance), which are what gates
> hypervisor M4. Phase-doc exit-gate criteria 3 (snapshot-store M1/M2 benchmarks)
> and 4 (guest-sdk agent boots in-guest) are owned by the `snapshot-store` and
> `guest-sdk` sibling repos respectively and are tracked/signed-off there; they
> are independent parallel tracks (phase doc §"Parallelism notes") and do not
> gate hypervisor M4.*

This converts a silent gap into an explicit, auditable scope statement. It does
not change the verdict — the M4 guard the doc invokes is the determinism guard,
and that is satisfied.

### I-2. The dk1 bead description also omits gate criteria 3 & 4

`bd show determinism-hypervisor-dk1` enumerates the gate items the bead is meant
to verify, and it too lists only the hypervisor-track items (determinism,
landing, skid, counting_semantics, M3 accepts, CI-required, TSC). This is
*consistent* with the sign-off doc (good — no internal contradiction), but it
means the omission is systemic: the bead, not just the doc, scopes Phase 1's
gate to the hypervisor track without saying the other two criteria are
elsewhere. If I-1 is fixed in the doc, mirror one line into the dk1 description
(or a bd note) so the close record is self-explanatory. Important, not Critical,
because the scoping is defensible and the runner/snapshot/guest-sdk tracks are
tracked by their own beads/repos.

### I-3. bd open-bead state — judged consistent with "Phase 1 complete"

`bd list --status=open` → **0 open**. Two beads are `in_progress`:

- **dk1** — this sign-off itself; closes via bd after merge (expected).
- **6eb** — "Register self-hosted GitHub runner labeled kvm-intel," assignee
  `codex`, P0, chore. Its own notes (iteration 53) state: *"the kvm-intel runner
  IS registered and has run every main-push CI for 30+ iterations ... This bead
  appears complete in reality; left open for its assignee to confirm and close."*

**Judgment:** 6eb being open does **not** contradict the sign-off. Gate row 6
(CI determinism regression required-for-merge and green) *depends on* the runner
working, and the runner demonstrably works — `ci/branch-protection.json` lists
`kvm-intel` as a required check and the doc cites a SUCCESS main run. 6eb is a
paperwork/ownership close, not blocking work. I would not hold the gate on it.
Recommend a one-line note in the handoff acknowledging 6eb is a no-op close so a
future reader does not read "P0 in_progress runner bead" as an open risk against
the gate. (Suggestion-level; tracked in 02.)
