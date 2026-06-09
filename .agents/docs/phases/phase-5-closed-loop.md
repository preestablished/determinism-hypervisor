# Phase 5 — The Loop Closes (autonomous search; first boss)

## Outcome

The full select→fork→act→score→commit loop runs against the real platform, with live
visibility. **The platform autonomously progresses past the game's first boss with no
human gameplay input** (MAP.md build-order step 4 — the program's go/no-go milestone),
the search survives a kill-and-resume, and the winning trajectory is replay-verified
through the hypervisor's verification mode. After this phase, the only things missing
are operator ergonomics, watchable video, and scale.

## Entry requirements

- Phase 4 exit gate (scorer first-boss-ready; synthesizer M1–M3).
- Phase 3's snapshot-store GC (long runs churn the tree).
- Orchestrator M1–M4 on fakes — done opportunistically in Phases 3–4, or done first
  thing here.

## Work, by repo (ordered)

**`exploration-orchestrator` — the spine of the phase:**

1. M1 — `orch-core`: tree, frontier, archive, selection policies (pure, no I/O).
2. M2 — fakes: a searchable synthetic grid-world (FakeHypervisor/FakeScorer).
   *Depends on M1.*
3. M3 — scheduler + pipeline + retries (slot leases, backpressure). *Depends on M2.*
4. M4 — experiment runner end-to-end on fakes + checkpoint/resume. *Depends on M3.*
   (M1–M4 have zero platform dependencies — they should already be done; listed here
   for completeness.)
5. M5 — hardening: config surface, metrics, single-writer discipline, soak on fakes.
   *Depends on M4.*
6. M6 — first integration: real snapshot-store + real hypervisor on the Intel box
   (search over the real emulator with trivial scoring). *Depends on M5 + Phase 3
   exit gate.*
7. M7 — **full-stack demo milestone: past the first boss.** Real scorer, real
   synthesizer, demo scoring program, plateau ladder active. *Depends on M6 + Phase 4
   exit gate.*

**`observatory` — eyes on the run (parallel chain; v1 gate is M4):**

1. M1 — event ingest (gRPC :7470) + projections. *Orchestrator M5 emits the canonical
   event stream; integrate as soon as both sides exist.*
2. M2 — scraper, rollups, derived search-health metrics. *Depends on M1.*
3. M3 — web UI core: run list + run dashboard + SSE. *Depends on M2.*
4. M4 — findings feed + alert engine ⇐ **v1 / first-boss gate** (stall alerts matter
   for multi-hour search runs). *Depends on M3.*

**Experiment configuration (no new repo):** the first-boss run is configured by file
(the orchestrator's documented **standalone mode**: file-based ExperimentConfig YAML,
inline artifact bodies for scorer/synthesizer bring-up, local-path workload image) —
control-plane run-lifecycle arrives in Phase 6 and is explicitly **not** a dependency
here.

## Cross-repo ordering

```
orchestrator M1 ─► M2 ─► M3 ─► M4 ─► M5 ─► M6 ─► M7 (first boss)
                                      │      ▲      ▲
                                      │   Phase 3   Phase 4 exit
                                      ▼   exit gate (scorer M4, synth M3)
observatory M1 ─► M2 ─► M3 ─► M4 ─────┘ (consumes M5's event stream; live before the M7 gate run)
```

## Exit gate

1. **First boss, autonomously:** from power-on, with no human gameplay input, the
   search reaches and defeats the first boss within the configured budget. The
   boss-defeated feature flips in a committed tree node.
2. **Resumable:** SIGKILL the orchestrator mid-run; resume from checkpoint; the
   search continues and still reaches the gate (archive/RNG/frontier state restore
   correctly, scorer archive restored in lockstep).
3. **Replay-verified:** the root→boss-node input log re-executes through hypervisor
   verification mode and reproduces the recorded state hash exactly.
   Committed nodes carry their re-execution digests (`state_hash` in node attrs,
   written at commit) and the root node carries the workload attrs — the tree must be
   born replay-ready so Phase 7 can certify it without re-running the search.
4. **Observed:** the run dashboard showed live score-over-time and expansion
   throughput during the gate run; a stall alert fired at least once in testing.
5. Throughput floor: sustained expansions/sec keeps all Intel-box worker slots >80%
   busy over a 4-hour soak.

## Parallelism notes

Two tracks: orchestrator (critical path) and observatory. The orchestrator's
fakes-first design exists precisely so M1–M5 never wait on the platform — enforce
that. Expect the gate run itself to be iterative: scoring-program weight tuning,
macro-pack additions, and plateau-ladder tuning across multiple attempts. Budget real
wall-clock time (days, including multi-hour search runs) for the M7 gate, and use the
observatory dashboard to diagnose stalls rather than guessing.
