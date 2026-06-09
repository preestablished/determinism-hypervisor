# Phase 4 — Scoring & Inputs (judgment and hands)

## Outcome

The platform can look at a captured guest state and produce numbers — progress score,
novelty score, canonical dedup hash, goal verdict — and can generate reproducible
candidate input bursts to try next. Both services are validated against **real
captures** from the Phase 3 workload. Everything the search loop will call now exists;
only the loop itself is missing.

## Entry requirements

- Phase 3 exit gate. Specifically: real RAM/framebuffer captures from the in-VM
  emulator (golden-test corpus), and the demo feature map exercised against the real
  region layout.
- DGX Spark preflight from Phase 0 (the scorer deploys there, CPU-only at this stage).

## Work, by repo (ordered)

**`state-scorer` — sequential chain to the first-boss-ready gate:**

1. M1 — feature decoding (consume the canonical feature-map schema; golden tests
   against Phase 3 captures).
2. M2 — expression engine + scoring DSL (EBNF grammar → stack bytecode). *Depends on
   M1.*
3. M3 — canonical hash, cells, count-based novelty archive, goal predicate.
   *Depends on M2.*
4. M4 — gRPC service + archive checkpointing ← **first-boss-ready**. *Depends on
   M3.* GPU tiers (M5/M6) are explicitly deferred to Phase 8 — the first-boss
   milestone needs none of them.

**`input-synthesizer` — sequential chain, shorter:**

1. M1 — pad model + weighted-random generator (per-button Markov chains, geometric
   holds) + gRPC shell.
2. M2 — macro packs + macro generator (handwritten YAML packs for the demo game:
   movement, jumps, weapon use). *Depends on M1.* M1+M2 is the documented v1 for the
   first-boss milestone.
3. M3 — mutation generator (seven operators over parent bursts). *Depends on M2;
   stretch within this phase, hard requirement before Phase 5's gate run.*

**`reference-workload` — close out:**

1. M6 — scoring/goal integration + exploration readiness (joint with state-scorer):
   the demo scoring program (staged milestones: leave start area → first upgrade →
   first boss → … → credits flag) and goal predicate load into the scorer and
   evaluate correctly against captured states. *Depends on scorer M2–M3.*

## Cross-repo ordering

```
scorer M1 ──► M2 ──► M3 ──► M4  (first-boss-ready)
               │      ║ joint
               └──► refwork M6 (scoring program + goal predicate validated)

synthesizer M1 ──► M2 ──► (M3)   (independent of scorer chain)
```

## Exit gate

1. scorer M4 acceptance: ScoreBatch over K=32 real captures within the CPU-path
   latency budget (1.5 ms p50 / 8 ms p99 — the 6/25 ms figure is the GPU-inclusive
   budget, deferred to Phase 8); archive checkpoint → kill → restore → identical
   subsequent scores (archive determinism test).
2. Canonical-hash dedup verified on real states: same room/position/inventory under
   different volatile bytes (frame counters) → equal hashes; distinct states →
   distinct hashes.
3. reference-workload M6: the staged scoring program scores a hand-played trajectory
   monotonically through its stages, and the goal predicate fires only on the
   credits flag.
4. synthesizer golden-seed tests: identical bursts for identical (request, seed)
   across x86_64 and aarch64; χ²/KS distribution tests green; macro packs load and
   instantiate.

## Parallelism notes

The scorer and synthesizer chains are fully independent — one agent each. The
reference-workload M6 close-out belongs to whoever owns the feature map. If Phase 3's
opportunistic orchestrator-on-fakes work (M1–M4) hasn't happened, run it now as a
third parallel track; Phase 5 assumes it is done.
