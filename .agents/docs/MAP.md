# Project Determinism — System Map

**Audience:** coding agents implementing this platform. Read this file first, then the
per-service docs in `docs/<service>/`.

## Mission

Build a platform that runs arbitrary software inside a **fully deterministic execution
environment**, snapshots its state cheaply, forks execution from any snapshot, and uses
**snapshot-tree search guided by a scoring function** to autonomously explore the
software's state space. The flagship demonstration: the platform autonomously plays a
classic 16-bit console game (running under an emulator inside the deterministic
environment) from power-on to the end credits, with zero human gameplay input — and any
discovered trajectory is perfectly reproducible from the input log alone.

The same machinery generalizes to autonomous bug-hunting in ordinary server software:
replace "game progress score" with "code coverage + reachability events," and replace
"controller inputs" with "network/syscall-level fault and input injection."

## Clean-room source boundary

Implementation agents must treat this project's docs as the complete product
specification for v1. They may use only:

- the docs in this project,
- public hardware references and public test ROM suites for the target console family,
- public language, operating-system, KVM, Rust, and crate documentation,
- artifacts explicitly supplied by the operator for the current experiment.

Agents must not consult third-party deterministic-testing platforms, external
case-study writeups, proprietary service documentation, private SDKs, private APIs, or
non-public implementation notes while implementing this project.

If a requirement cannot be implemented from the allowed source set, stop and file a
documentation issue instead of filling the gap from an external case study.

## Core principles

1. **Determinism is the foundation.** Same snapshot + same injected inputs ⇒
   bit-identical execution, every time. All nondeterminism (time, interrupts, entropy,
   I/O ordering) is virtualized and derived from a seeded, logged source.
2. **State is cheap to save and fork.** Exploration throughput is bounded by
   snapshot/restore cost. Target: fork-and-run-one-second-of-guest-time in well under a
   second of wall time, with page-level copy-on-write dedup.
3. **Exploration is search, not play.** A Go-Explore-style loop: pick a promising saved
   state from the frontier, fork it, apply a short burst of generated inputs, score the
   result, keep it if it's novel or better, repeat — across thousands of parallel
   branches.
4. **Everything is replayable.** A result is a path through the state tree. The replay
   pipeline re-executes the input log from the root and must reproduce the result
   bit-identically; that re-execution is the proof.

## Hardware & toolchain

| Resource | Role |
|---|---|
| 64-bit Intel Linux box (VT-x) | Hosts the deterministic hypervisor and all execution workers. KVM is the virtualization substrate; x86_64-specific determinism work (PMU-based instruction counting, deterministic interrupt delivery) lives here. |
| NVIDIA DGX Spark (aarch64 + Blackwell GPU) | GPU work: framebuffer novelty models, perceptual hashing/embeddings, optional learned input policies, video encoding for replays. Also runs control-plane/observatory services. **Note: aarch64 — the hypervisor does not run here.** |
| Rust | Primary language for every service. gRPC (tonic) + protobuf for inter-service APIs; shared `.proto` files are versioned in `control-plane`. |

## Services / repos

| Repo | One-liner | Runs on |
|---|---|---|
| [`determinism-hypervisor`](determinism-hypervisor/) | Rust VMM on KVM providing bit-deterministic guest execution, snapshot/fork/restore, virtual time, and input injection | Intel box |
| [`snapshot-store`](snapshot-store/) | Content-addressed, page-deduplicated storage for guest snapshots plus the lineage (state-tree) metadata | Intel box (NVMe) |
| [`exploration-orchestrator`](exploration-orchestrator/) | The search brain: frontier management, branch scheduling, the select→fork→act→score→commit loop | Either |
| [`input-synthesizer`](input-synthesizer/) | Generates candidate input bursts: weighted random, macro library, mutation of successful prefixes, optional learned policy | Either |
| [`state-scorer`](state-scorer/) | Turns a guest state into numbers: progress score from mapped RAM features, novelty from count-based stats and GPU frame embeddings, state dedup | DGX Spark (GPU) |
| [`guest-sdk`](guest-sdk/) | In-guest agent + instrumentation library: exposes RAM/framebuffer/input port for the game harness; assertions, reachability events, coverage beacons for general software | inside guest |
| [`reference-workload`](reference-workload/) | The demo target: a deterministic console emulator packaged as a guest image, plus the per-game RAM feature map and goal definition | inside guest |
| [`replay-renderer`](replay-renderer/) | Reconstructs root→node input logs, re-executes them to verify determinism, renders frames to video | Spark (GPU encode) + Intel (re-exec) |
| [`control-plane`](control-plane/) | API gateway, experiment definitions, job queue, artifact registry, authn, `detctl` CLI; owns the shared protobuf schemas | DGX Spark |
| [`observatory`](observatory/) | Telemetry sink and visualization: live exploration-tree map, coverage heat maps, score-over-time dashboards | DGX Spark |

## Dataflow (one exploration step)

```
exploration-orchestrator                      state tree lives in snapshot-store
        │
        │ 1. select frontier node N (weighted by score, novelty, visit count)
        ▼
input-synthesizer ── 2. propose K input bursts for N's context
        │
        ▼
determinism-hypervisor (×K workers, Intel box)
        │ 3. restore snapshot(N), inject burst, run T guest-seconds, pause
        │ 4. capture (hypervisor-owned): dirty pages → snapshot-store; packed RAM
        │    feature bytes (per the experiment's compiled extraction list, resolved
        │    through the guest-sdk region manifest) + lz4 framebuffer → returned to
        │    the orchestrator, which forwards them inline in ScoreBatch
        ▼
state-scorer (Spark GPU)
        │ 5. progress score + novelty score + dedup verdict per child
        ▼
exploration-orchestrator
        │ 6. commit interesting children as new tree nodes (snapshot ref + input delta);
        │    discard duplicates/regressions; update frontier weights
        ▼
observatory  ── 7. stream events: nodes added, best score, coverage map
```

Terminal condition for the demo: a child state satisfies the goal predicate defined in
`reference-workload` (end-credits RAM flag). Then `replay-renderer` walks the tree from
root to that node, concatenates input deltas, re-executes the full log for verification,
and renders the proof video.

## Key cross-service contracts

- **Snapshot reference** (`snapshot-store`): the BLAKE3-256 of the manifest's
  canonical bytes — nothing else. Tree node IDs are a separate identifier space.
  Hypervisor workers resolve refs to page sets; the orchestrator only ever handles
  refs, never pages.
- **Input log** (`determinism-hypervisor`): the canonical serialized record of every
  injected event (input, virtual-time tick, entropy draw) between two snapshots.
  Replay = snapshot + input log. This format is the platform's most stability-critical
  schema; version it explicitly.
- **Feature map** (`reference-workload` → `state-scorer`): declarative description of
  guest RAM addresses/widths and their semantic meaning (progress flags, position,
  inventory), so the scorer is game-agnostic.
- **Event stream** (`guest-sdk` → host): hypercall/virtio channel carrying structured
  events (assertion hit, reachability event, coverage beacon) out of the guest without
  perturbing determinism.

## Build order (dependency-driven)

1. `determinism-hypervisor` + `snapshot-store` — nothing works without deterministic
   fork/restore. Milestone: fork a guest 1000× and verify bit-identical re-execution.
2. `guest-sdk` + `reference-workload` — get the emulator running in-guest with RAM and
   input port exposed. Milestone: scripted input log plays the game's first room.
3. `state-scorer` (RAM features first, GPU novelty later) + `input-synthesizer`
   (weighted random first).
4. `exploration-orchestrator` — close the loop. Milestone: autonomous progress past the
   first boss with no human input.
5. `control-plane`, `observatory`, `replay-renderer` — operability, visibility, proof.

## Contract ownership & arbitration

Ports are arbitrated by the table below; **API shapes and formats are arbitrated by
ownership**: every cross-service contract has exactly one owner doc, clients cite it
and never restate it normatively. On any conflict, the owner doc wins.

| Contract | Owner |
|---|---|
| Hypervisor worker API, DHILOG input log, capture engine (CaptureSpec/feature_bytes/fb_lz4), chained state hash, DEV_EVENT encodings | `determinism-hypervisor` |
| Snapshot manifest, page store, tree/lineage API (experiment-scoped, caller-assigned node IDs), metadata KV | `snapshot-store` |
| ScoreBatch/ScoreResult, scoring-DSL evaluation, novelty archives + checkpointing | `state-scorer` |
| ProposeBursts, burst wire format, macro packs | `input-synthesizer` |
| Feature-map schema, scoring-program DSL surface, goal predicate (inside the program) | `reference-workload` |
| Guest channel (detchannel rings), region manifest, SDK API, READY point | `guest-sdk` |
| `.dilog` replay container, render job API | `replay-renderer` |
| Event envelope + event-type catalog | `observatory` |
| Resource registry, ExperimentConfig delivery, proto repo layout | `control-plane` |
| GPU policy-serving gRPC contract (Phase 8, served on the Spark) | `input-synthesizer` (interface owner/consumer; server implementation scheduled with the Phase 8 policy tier) |
| ExperimentConfig contents, worker-driver composition, bootstrap, commit rules | `exploration-orchestrator` |

## Capacity planning (read before Phase 8)

Multiply before you commit compute. With documented defaults — K=16 jobs/expansion,
~2 guest-seconds/burst, <450 ms hypervisor budget + overhead ≈ 0.5–0.7 s wall per
job, slots = Intel-box cores − 2 — sustained throughput is order **1–3 expansions/s
(~10⁵/day)**. A first-boss search is plausibly ~10⁵–10⁶ expansions (days); a
credits-length trajectory for a multi-hour game is realistically **10⁶–10⁷ expansions
— weeks at default budgets**. Consequences: the default `max_wall_clock_s` (1 day) is
a smoke-test budget, not a campaign budget; the campaign needs the Phase 5 run as a
measured calibration point, raised budgets, tuned burst lengths, and an explicit
slot-count requirement for the Intel box. Storage: at 1–3 committed children/s and
MiB-scale deltas, expect **0.5–1.5 TiB/day of pre-GC churn** — size NVMe and GC
cadence accordingly (see snapshot-store M9 and the Phase 8 entry gate).

Canonical figures all service docs derive from: **demo guest = 128 MiB**, hypervisor
per-job budget **<450 ms** with snapshot-store's **≤100 ms** storage share inside it.

## Canonical port plan

One pair per service: first port = gRPC (tonic), second = HTTP `/healthz` + `/metrics`.
Per-service docs confirm these; on any conflict, this table wins.

| Service | Ports | Host |
|---|---|---|
| determinism-hypervisor (`dh-workerd`) | 7400 / 7401 (+ UDS fast path) | Intel box |
| snapshot-store | 7410 / 7411 (+ UDS fast path) | Intel box |
| exploration-orchestrator | 7420 / 7421 | either |
| input-synthesizer | 7430 / 7431 | either |
| state-scorer | 7440 / 7441 | DGX Spark |
| replay-renderer `replayd` | 7450 / 7451 | DGX Spark |
| replay-renderer `reexec-agent` | 7455 / 7456 | Intel box |
| control-plane | 7460 gRPC / 7461 REST / 7462 ops | DGX Spark |
| observatory (ingest gRPC / UI+REST) | 7470 / 7471 | DGX Spark |
| GPU policy-serving (optional, input-synthesizer) | 7480 / 7481 | DGX Spark |

## Conventions for all repos

- Rust 2021+, `tonic` for gRPC, `serde`+`postcard` or protobuf for on-disk formats.
- Every persisted format carries an explicit version field.
- Every service exposes `/healthz`, Prometheus metrics, and structured JSON logs.
- Determinism bugs are P0 everywhere: any service that touches execution or replay must
  include a determinism regression test (re-execute, compare hashes) in CI.
