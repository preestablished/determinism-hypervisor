# Phase 2 — Fork & Replay (the timeline tree exists)

## Outcome

The platform can snapshot a running guest, restore it, **fork it into many divergent
children**, and replay any (snapshot, input log) pair to a bit-identical result. The
hypervisor is reachable over gRPC as a worker daemon with slot management. This phase
ends at **Platform Milestone 1** (MAP.md build-order step 1): fork a guest 1000× and
verify bit-identical re-execution.

## Entry requirements

- Phase 1 exit gate (single-timeline determinism proven; snapshot-store core working
  on synthetic data).

## Work, by repo (ordered)

**`snapshot-store` — finish the service surface first (it blocks hypervisor M4):**

1. M4 — gRPC surface + client lib.
2. M5 — fast path (UDS SEQPACKET page channel with memfd fd-passing, for the
   co-located hypervisor workers). *Depends on M4.*
3. M6 — durability: crash-injection harness. *Parallel with hypervisor work below.*

**`determinism-hypervisor` — the integration chain:**

1. M4 — snapshot / restore / fork + snapshot-store integration (XSAVE
   canonicalization, dirty-page tracking, memfd-sealed tier-A forks). *Depends on
   snapshot-store M4; switch to the M5 fast path when it lands.*
2. M5 — input log (DHILOG v1) + replay. *Depends on M4.*
3. M6 — worker daemon (gRPC :7400) + introspection (ReadGuestMemory,
   GetFramebuffer, slot leases). *Depends on M5.*
4. M7 — **Platform Milestone 1: fork 1000× + verified re-execution.** *Depends on
   M6.*

**`snapshot-store` — joint close-out:**

4. M8 — hypervisor integration + determinism regression (joint milestone — this is
   the same test as hypervisor M7, owned from the store side: page round-trip
   fidelity, manifest lineage correctness under 1000-way forking).

**`reference-workload` — parallel track (still host-side, no platform deps):**

1. M2 — demo game first room, host-side. This is the **build-vs-vendor review
   gate** for the emulator: pass it before committing to Phase 3 scope.

## Cross-repo ordering

```
snapshot-store M4 ──► M5
       │                │
       └────────────────┴─► hypervisor M4 ──► M5 ──► M6 ──► M7 ◄══ joint ══► snapshot-store M8
snapshot-store M6 (crash injection)                       (parallel, must be green before M8 signs off)
reference-workload M2                                      (independent)
```

## Exit gate

1. **Platform Milestone 1:** from one mid-boot snapshot, fork 1000 children with
   distinct input logs; every child re-executed from (root snapshot + spliced log)
   reproduces its recorded chained state hash exactly. Zero divergences.
2. Fork latency and snapshot-commit latency within the per-stage budget (hypervisor
   tier-A fork <10 ms; snapshot-store delta commit 8 ms p50; full exploration-step
   storage budget ≤100 ms).
3. snapshot-store crash-injection suite green (commit ordering: pages → manifest →
   node row survives kill -9 at every failpoint).
4. The worker daemon serves the full v1 gRPC surface; two concurrent slots fork and
   replay without cross-talk.
5. reference-workload M2 review gate decided (build vs vendor), first room playable
   host-side from a scripted input log.

## Parallelism notes

The hypervisor M4→M7 chain is strictly sequential and is the phase's critical path.
snapshot-store M6 (crash injection) and reference-workload M2 run alongside it.
Start `exploration-orchestrator` M1–M2 (pure core + fakes) opportunistically in this
phase if hands are free — they have zero platform dependencies (see Phase 5).
