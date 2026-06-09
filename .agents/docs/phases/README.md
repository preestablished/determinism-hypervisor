# Project Determinism — Phase Plan

**Audience:** coding agents executing the build. Read `../docs/MAP.md` first for the
system design; this directory sequences the work from **nothing** (Phase 0: no repos,
no code) to the flagship result (Phase 8: the platform autonomously plays the demo
game from power-on to end credits, machine-verified and rendered to video).

Milestone IDs (M0, M1, …) refer to each repo's `IMPLEMENTATION-PLAN.md` in
`../docs/<repo>/`. A phase is **done** when its exit gate — always empirical, never
"code merged" — passes. Do not start a phase's gated work before the prior gate is
green; *do* pull forward the explicitly-marked early-start tracks.

## The phases

| Phase | Name | What becomes possible | Hard gate |
|---|---|---|---|
| [0](phase-0-bootstrap.md) | Bootstrap | Ten repos build green; proto crate published; hosts provisioned | CI green everywhere; preflight checklist |
| [1](phase-1-deterministic-execution.md) | Deterministic execution | One guest timeline, bit-repeatable; events land at exact instruction counts | 100× re-run, zero hash divergence |
| [2](phase-2-fork-and-replay.md) | Fork & replay | Snapshot/restore/fork via the store; input-log replay; gRPC worker | **Platform Milestone 1:** fork 1000× + verified re-execution |
| [3](phase-3-workload-in-the-box.md) | Workload in the box | The emulator + game runs in-VM; RAM/framebuffer host-readable | Determinism suite 20× zero-flake; scripted log plays first room in-VM |
| [4](phase-4-scoring-and-inputs.md) | Scoring & inputs | States become scores/hashes/goal verdicts; reproducible input bursts | Scorer first-boss-ready on real captures; cross-arch golden seeds |
| [5](phase-5-closed-loop.md) | The loop closes | Autonomous search over the real game, observable, resumable | **First boss beaten autonomously**, replay-verified |
| [6](phase-6-operability.md) | Operable platform | Full operator workflow via `detctl`; tree + coverage visualization | Clean-checkout operator end-to-end, zero host file edits |
| [7](phase-7-proof-pipeline.md) | Proof pipeline | Any node → verified replay + MP4; divergence auto-bisection | First-boss trajectory re-verified third-party + rendered |
| [8](phase-8-full-campaign.md) | The full campaign | **Credits roll: the flagship result** | Autonomous credits run, verified, rendered, independently reproducible |

## Phase dependency graph

Phases gate sequentially, but each phase carries marked early-start tracks that
overlap the next/previous phase (emulator host-side work spans 1–2; orchestrator
fakes span 3–4; replay splice/encode spans 6–7):

```
0 ─► 1 ─► 2 ─► 3 ─► 4 ─► 5 ─► 6 ─► 7 ─► 8
     │         ▲    ▲    ▲         ▲
     └ refwork M1–M2 ┘    │    │         │   (early-start, host-side)
               └ orch M1–M4 on fakes ┘   │   (early-start, zero platform deps)
                              └ replay M1–M3 ┘ (early-start, mock hypervisor)
```

## Repo × phase milestone matrix

Which milestones of each repo land in which phase (parentheses = early-start or
pulled-forward; see the phase doc for the reasoning):

| Repo | P0 | P1 | P2 | P3 | P4 | P5 | P6 | P7 | P8 |
|---|---|---|---|---|---|---|---|---|---|
| determinism-hypervisor | M0 | M1–M3 | M4–M7 | M9 ⚠ | — | — | — | M8 + H1–H8 | soak |
| snapshot-store | M0 | M1–M3 | M4–M6, M8 | M7 | — | — | — | — | M9 |
| guest-sdk | Ms0 | Ms1–Ms2 | — | Ms3–Ms5 | — | — | — | — | Ms6 |
| reference-workload | M0 | (M1) | (M2) | M3–M5 | M6 | — | registration | — | score tuning |
| state-scorer | wksp | — | — | — | M1–M4 | — | — | — | M5–M7 |
| input-synthesizer | M0 | — | — | — | M1–M3 | — | — | — | M4–M7 |
| exploration-orchestrator | M0 | — | — | (M1–M4) | — | M5–M7 | — | — | M8 |
| control-plane | **M0 first** | — | — | — | — | — | M1–M4 | M5 | M6–M7 |
| observatory | M0 | — | — | — | — | M1–M4 | M5–M6 | M7 | M8 |
| replay-renderer | M0 | — | — | — | — | — | (M1–M3) | M4–M6 | **M7 flagship** |

⚠ Hypervisor M9 (minimal-Linux guest) is listed last in that repo's own plan but is
**pulled forward to Phase 3**: the workload's guest-image pipeline and the
agent-as-PID-1 design assume a Linux guest. The nanokernel guest satisfies the Phase
1–2 gates; re-run both gates against the Linux guest when M9 lands.

## Critical path

```
CP M0 → hyp M1→M2→M3 → store M4/M5 → hyp M4→M5→M6→M7 → hyp M9 → sdk Ms3→Ms4 ⇄ refwork M4 → refwork M5
      → scorer M1→M4 → orch M6→M7 (FIRST BOSS) → [operability/proof in parallel] → campaign → replay M7 (CREDITS MP4)
```

Everything not on this path is a parallel track; the phase docs mark what can be
fanned out. The two highest-risk segments — instruction-precise determinism (Phase 1)
and the joint in-VM bring-up (Phase 3) — sit early by design.

## Standing rules across all phases

- **Gates are empirical.** Every exit gate re-runs, observably, on the real hosts.
  "It builds" never closes a phase ([MAP.md conventions](../docs/MAP.md)).
- **Determinism regressions are P0 in every phase**, and each phase's gate re-runs
  the prior determinism gates it builds on (Phase 3 re-runs Phase 1/2 gates against
  the Linux guest; Phase 5 replay-verifies its trajectory; Phase 8 re-verifies
  third-party).
- **The proto crate only grows.** Released fields are never broken
  (control-plane's versioning policy); any cross-service schema change lands in
  `control-plane/proto/` before the consuming code.
- **Early-start tracks are free parallelism, not scope creep:** they're listed in
  each phase doc; anything else from a later phase waits.
