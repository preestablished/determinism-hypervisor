# Phase 6 — Operable Platform (the front door)

## Outcome

An operator can drive the platform without touching config files on hosts: push a
workload image, register a feature map and scoring program, create an experiment,
start/pause/resume/watch a run — all through `detctl` against the control-plane, with
auth, audit, and a host/worker registry. The observatory grows the exploration tree
map and the spatial coverage heat map. The platform becomes a product rather than a
lab bench.

## Entry requirements

- Phase 5 exit gate (the loop works; this phase wraps it, it doesn't change it).
- TLS material from Phase 0 provisioning.

## Work, by repo (ordered)

**`control-plane` — the spine of the phase:**

1. M1 — skeleton server: SQLite/WAL DB, scoped bearer tokens, audit log, ops
   endpoints (:7460–7462).
2. M2 — resource registry + content-addressed blob store + `detctl` read/write
   basics (image push, featuremap push, experiment create). *Depends on M1.*
3. M3 — job queue (visibility timeouts, lease fencing) + host registry + host agent
   (Intel box services register and heartbeat). *Depends on M2.*
4. M4 — run lifecycle against the orchestrator: StartRun/Pause/Resume/Cancel/
   WatchRun, config delivery (experiment YAML → orchestrator, scoring program →
   scorer). *Depends on M3; replaces Phase 5's file-based configuration.*

**`observatory` — visualization chain (parallel):**

1. M5 — exploration tree map (server-side LOD: subtree aggregates, high-score spine,
   supernodes; click-through node panel). *Needs only the Phase 5 event store.*
2. M6 — spatial coverage map derived from feature-map discretization hints (room
   grid heat map). *Depends on M5's UI plumbing.*
3. UI proxy actions (pause run, render request stubs) route through control-plane
   M4's API — wire them once M4 lands.

**`reference-workload` / deployment (small, parallel):**

- Register the real guest image + feature map + scoring program as control-plane
  artifacts (the determinism green-stamp gate from its WorkloadImage spec becomes a
  registration precondition).
- Stand up the two-host deployment from control-plane's topology docs (systemd/
  compose), replacing hand-started binaries.

## Cross-repo ordering

```
control-plane M1 ─► M2 ─► M3 ─► M4 ─► (orchestrator runs under control-plane lifecycle)
                     │
                     └─► artifact registration of the real workload
observatory M5 ─► M6                  (parallel; proxy actions wire after CP M4)
```

## Exit gate

1. **Operator end-to-end:** on a clean checkout, an operator with only a token and
   `detctl` pushes the image, feature map, and scoring program, creates the
   experiment, starts a run, watches progress, pauses, and resumes — zero host file
   edits. (This re-runs a short version of the Phase 5 search under control-plane
   lifecycle to prove nothing regressed.)
2. Auth enforced: an unscoped token cannot mutate; every mutation appears in the
   audit log.
3. Host registry shows live heartbeats for every service on both hosts; killing a
   worker shows up within one heartbeat interval.
4. Tree map renders the Phase 5 gate run's tree (hundreds of thousands of nodes)
   interactively; coverage map shows the explored room grid with the boss room hot.

## Parallelism notes

Control-plane and observatory tracks are independent until the proxy-action wiring at
the end. The deployment/registration work is a third small track. Nothing here blocks
Phase 7's replay-renderer M1–M3, which can start concurrently (it needs only Phase 2
artifacts and a mock hypervisor) — start it if hands are free.
