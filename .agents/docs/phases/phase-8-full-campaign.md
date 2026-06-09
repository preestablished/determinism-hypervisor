# Phase 8 — The Full Campaign (credits roll)

## Outcome

**The flagship result.** From power-on, with zero human gameplay input, the platform
autonomously plays the demo game to the end credits; the goal node's trajectory is
machine-verified and rendered to a watchable MP4; a third party can re-verify the
result from the published input log and image alone. This is the capability the entire
program exists to demonstrate — and everything in it generalizes to autonomous
bug-hunting
(swap the scoring program for coverage + reachability events, the pad model for the
event grammar).

A full-game search is one to two orders of magnitude more work than the first boss:
deeper trees, longer plateaus (ability-gated backtracking across the map), heavier
disk churn, multi-day runs. This phase is about search power and endurance, then the
campaign itself.

## Entry requirements

- Phase 7 exit gate (proof pipeline live — you want verified video of incremental
  records during the campaign, not just at the end).
- Phase 6 operability (multi-day runs need detctl lifecycle, alerts, and backups).
- **Campaign sizing reviewed against Phase 5 measured throughput** (MAP.md's
  capacity-planning section): expansions/s actually achieved × campaign expansion
  estimate → required days, raised budgets, and NVMe/GC headroom — signed off before
  betting weeks of compute.

## Work, by repo (ordered)

**Search power (parallel tracks, all optional-but-expected):**

| Repo | Milestones | Purpose |
|---|---|---|
| `state-scorer` | M5 (pHash visual tier, CPU) → M6 (GPU RND embedding tier on the Spark) | Visual novelty breaks RAM-feature plateaus (new rooms/states the feature map doesn't capture). First GPU dependency on the *search* path (Phase 7's NVENC encode already used the GPU, with a software fallback). |
| `input-synthesizer` | M4 (macro mining from high-scoring paths) → M6 (learned-policy generator, gated; serves on :7480) | Mined macros encode discovered movement tech; the policy tier proposes context-aware actions. M5 (event grammar) only if pursuing the bug-hunting mode. |
| `exploration-orchestrator` | M8 — throughput & scale tuning | Frontier performance at millions of nodes, batch sizing, plateau-ladder tuning for ability-gated progression. |

**Endurance (parallel):**

| Repo | Milestones | Purpose |
|---|---|---|
| `snapshot-store` | M9 — operability polish | Disk-burn management, GC cadence under multi-day churn, integrity scans. |
| `observatory` | M8 — retention, rollups, ops hardening | Multi-day event volume; dashboards stay responsive. |
| `control-plane` | M6 (deployment hardening + backups) → M7 (live integration — its exit criterion) | DB/artifact backups before betting a week of compute; M7 signs off the whole control surface under real load. |
| `guest-sdk` | Milestone 6 — quiesce, hardening, perf | Close out in-guest perf (inject_point cost, ring throughput) before long runs. |
| `determinism-hypervisor` | (none new) | Already complete; campaign is its soak test. |

**The campaign (sequential, after the above):**

1. Tune the full-game scoring program: extend the staged milestones through the full
   progression (all required upgrades, mid/late bosses, escape/endgame sequence,
   credits flag); validate stages against a hand-played reference trajectory.
2. Run the campaign: multi-day autonomous run(s) under detctl, checkpointed,
   alert-monitored; expect iterations — plateau diagnosis via tree/coverage maps,
   macro-pack and weight adjustments between attempts (each attempt resumes from
   checkpoints rather than starting over).
3. `replay-renderer` M7 — **flagship: verified goal-trajectory MP4** ✅. Render and
   verify the credits run end-to-end; publish artifacts.

## Cross-repo ordering

```
scorer M5─►M6 ──┐
synth M4(─►M6) ─┼─► full-game scoring program ─► CAMPAIGN RUNS ─► goal node ─► replay M7 (flagship MP4)
orch M8 ────────┘            ▲
store M9 / obs M8 / CP M6─►M7 / guest-sdk Ms6   (endurance work; before long runs)
```

## Exit gate — the program's definition of done

1. **Credits, autonomously:** an experiment run started via `detctl`, with zero human
   gameplay input, reaches a state where the end-credits goal predicate fires.
2. **Machine-verified:** the root→credits input log re-executes bit-identically
   (hypervisor verification mode digest match) — the platform's proof.
3. **Watchable:** replay-renderer M7 artifact set published — full-run MP4 with input
   HUD, `.dilog`, timeline JSON — registered in control-plane with checksums.
4. **Independently reproducible:** on a clean environment **with a host matching the
   recorded determinism class** (CPU family/microcode/kernel/VMM tuple — published
   with the artifact set), a third party re-verifies the run from (workload image +
   `.dilog`) alone, per the re-verification drill. Reproducibility is
   determinism-class-scoped by design, not hardware-independent.
5. **Honest accounting:** the published result records total wall-clock, guest
   instructions, node count, and snapshot storage consumed — the platform's
   cost-of-result, reported plainly.

## Parallelism notes

All seven pre-campaign tracks are independent — fan out one agent per repo. The
campaign itself is operations, not coding: expect the score-tune → run → diagnose →
adjust loop to dominate the calendar, and resist changing platform code mid-campaign
(every change invalidates the determinism pedigree of in-flight checkpoints; if code
must change, restart the affected runs and say so in the result's accounting).
