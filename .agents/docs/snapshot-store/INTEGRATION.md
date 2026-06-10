# snapshot-store — Integration Flows

How the three primary consumers use this service. Caller code should go through
`snapstore-client` (Rust lib in this repo), which handles transport selection
(page channel → UDS gRPC → TCP gRPC), footer verification, and retries.

Consumers (per MAP.md):
- **determinism-hypervisor** workers (same Intel box): snapshot commit & restore, plus
  the sealed per-segment input log (`PutInputLog` at `TakeSnapshot`) — the
  bandwidth-critical path; uses the fast path.
- **exploration-orchestrator** (either host): tree CRUD/queries + the metadata KV
  (checkpoints/WAL, API.md §1.5); refs only, never pages.
- **replay-renderer** (Spark + Intel re-exec): path-to-root + input logs + pins on
  **every** path snapshot (each is a per-segment verification base, §4).

`state-scorer`, `input-synthesizer`, `guest-sdk`, `reference-workload` do **not** talk to
snapshot-store. `control-plane`/`observatory` consume `Stats` and `QueryNodes`
(read-only) when they come online (build-order phase 5).

---

## 1. Bootstrap: the root snapshot and root node

Performed once per experiment. The bootstrap sequence itself
(`CreateVm → Run(until READY beacon) → TakeSnapshot → CreateNode(root, node_id=0)`) is
**owned by `exploration-orchestrator`** — this section shows only the store-facing
half. The store has no artifact/image concept; it first hears about an experiment when
the root pages arrive.

```
orchestrator          hypervisor worker                snapshot-store
    │                        │                               │
    │ boot guest to the      │                               │
    │ READY point ──────────►│                               │
    │                        │ pause guest                   │
    │                        │ PUT_BATCH ×N (ALL 32,768 pages, page channel)
    │                        │──────────────────────────────►│ hash, dedup, pack, fsync
    │                        │ build FULL manifest           │
    │                        │   (entries 0..N-1 + dev blob) │
    │                        │ PutSnapshot(manifest) ───────►│ verify, write .spm, fsync
    │                        │◄────────────── snapshot_ref ──│
    │◄── snapshot_ref ───────│                               │
    │ CreateNode(experiment_id, node_id=0, parent unset,     │
    │   snapshot_ref, status=FRONTIER) ─────────────────────►│ SQLite txn
    │◄───────────────────────────────── NodeMeta(root) ──────│
```

The root is the only node with `node_id = 0`, no `parent_node_id`, and empty
`input_log_id`; its manifest is always FULL (API.md §2). Creating the root brings the
experiment into existence — one store serves many experiments concurrently, and their
trees are fully independent while page storage dedups across all of them.

## 2. Exploration step (the hot loop)

One step = orchestrator selects frontier node N, K workers fork it (MAP.md dataflow).
Per-child store interactions:

### 2.1 Worker side: restore → run → commit

```
orchestrator            hypervisor worker W              snapshot-store
    │ ExpandTask{node_id N,      │                            │
    │  snapshot_ref S, burst} ──►│                            │
    │                            │ (a) GetSnapshot(S) ───────►│
    │                            │◄────────── manifest(S) ────│  (worker caches manifests)
    │                            │ (b) figure needed pages:   │
    │                            │     if W already holds an  │
    │                            │     ancestor A's RAM image:│
    │                            │     ResolvePages(S,        │
    │                            │       baseline_ref=A,      │
    │                            │       hashes_only) ───────►│  delta entries only
    │                            │◄──── entries (idx,hash) ───│
    │                            │ (c) GET_BATCH ×m ─────────►│  page channel
    │                            │◄──────── memfd(pages) ─────│
    │                            │ scatter into guest RAM,    │
    │                            │ load device blob, resume   │
    │                            │ inject burst, run T guest-s, pause
    │                            │ (d) dirty set = hypervisor's dirty-page log
    │                            │ PUT_BATCH(dirty pages) ───►│  hash, dedup, append, fsync
    │                            │◄──── PutOk{new,dedup,xchk}─│  cross-check hash compared
    │                            │ (e) PutInputLog(sealed     │
    │                            │     DHILOG segment) ──────►│  validate container, store
    │                            │◄──────────── log_id L ─────│
    │                            │ (f) build DELTA manifest   │
    │                            │     parent = S             │
    │                            │     entries = dirty pages  │
    │                            │     dev blob from pause    │
    │                            │ PutSnapshot ──────────────►│  verify pages, write .spm
    │                            │◄──────── child_ref C ──────│
    │ result{C, log_id L,        │                            │
    │   state_hash, feature_bytes,                            │
    │   fb_lz4} ◄────────────────│  (orchestrator forwards    │
    │                            │   features inline to scorer)
```

Notes:
- (b)/(c): a worker that just ran N's parent usually holds ≥ 94% of S's pages already —
  the delta top-up is the common case and is why restore p50 is milliseconds, not the
  full 64 MiB transfer (ARCHITECTURE.md §7.1).
- (e) **The worker stores the input log.** `TakeSnapshot(seal_input_log = true)` closes
  the segment's DHILOG, `PutInputLog`s it here, and returns the `log_id` in
  `TakeSnapshotResponse` — the commit ordering is the hypervisor's (its API.md §5.1;
  INTEGRATION.md §2). The single writer of **tree** state is still the orchestrator
  (step 2.2): workers write pages, logs, and manifests, never node rows.
- (f) **FULL-manifest cadence:** the worker emits a FULL manifest (no parent) whenever
  the delta chain depth would exceed `max_delta_chain` (default 64). Chain depth is
  tracked by the worker via a `chain_depth` entry it keeps alongside cached manifests;
  it can recompute it any time by walking `GetSnapshot` parents.
- Discarded children (duplicates/regressions) cost snapshot-store only orphan pages +
  one orphan manifest + one orphan log, swept by the next GC — no tree write ever
  happens for them.

### 2.2 Orchestrator side: score → commit/discard

```
orchestrator                    state-scorer            snapshot-store
    │ score(child features) ───────►│                        │
    │◄── progress, novelty, dedup ──│                        │
    │                                                        │
    │ keep? ── yes:                                          │
    │   CreateNode{experiment_id, node_id=next from my WAL,  │
    │     parent_node_id=N, snapshot_ref=C,                  │
    │     status=FRONTIER, progress, novelty,                │
    │     input_log_id=L} ──────────────────────────────────►│  txn: node row → log L
    │◄────────────────────────────────── NodeMeta(child) ────│
    │ keep? ── no: do nothing (orphan manifest C ⇒ GC'd;     │
    │   there is no ReleaseSnapshot — discard is a no-op)    │
    │                                                        │
    │ end of step, batched bookkeeping:                      │
    │   UpdateNodes[{N: visit_count_delta=1, expand_count_delta=k,
    │     touch_visited, status=EXPANDED-if-policy-says},    │
    │     {…re-scored siblings…}] ──────────────────────────►│  one txn
    │◄──────────────────────────── updated_at counter ───────│
```

- `input_log_id` = `L`, the value the worker's `PutInputLog` returned, passed through
  `TakeSnapshotResponse` (hypervisor API.md §5.1; orchestrator API.md §3.1) — the
  orchestrator never holds log bytes. `CreateNodeRequest.input_log_container` (API.md
  §1.4) remains as a single-txn alternative for callers that do hold the bytes; no v1
  consumer uses it.
- All policy (when N stops being FRONTIER, score thresholds, dedup verdict handling)
  lives in the orchestrator. snapshot-store applies the updates verbatim.
- Goal hit: orchestrator sets the child `status=GOAL` (in the CreateNode or a later
  UpdateNodes) — this is just data here, but replay-renderer queries for it.

### 2.3 Orchestrator checkpoints, WAL, and warm start

The orchestrator keeps **all** of its durable state here — node rows for the tree,
plus the metadata KV (API.md §1.5) for everything that isn't a node (frontier weights,
seen-set/archive cursors, RNG state, plateau state, its write-ahead log):

```
PutMetadata{key="orch/wal/<exp>/<seq>", value, expected_generation=0}   → WAL append
PutMetadata{key="orch/ckpt/<exp>", value, expected_generation=G}        → checkpoint (CAS)
DeleteMetadata{key="orch/wal/<exp>/<seq>"}                              → WAL truncate
```

The checkpoint CAS doubles as **single-writer enforcement**: a second orchestrator
instance (or a stale one after failover) loses the generation race, gets
`FAILED_PRECONDITION`, and must stand down.

On (re)start the orchestrator rebuilds from the store — the store is the source of
truth:

```
GetMetadata{key="orch/ckpt/<exp>"} (+ any orch/wal/<exp>/* tail)    → search state
QueryNodes{experiment_id, statuses=[FRONTIER], order_by=CREATED_AT} → full frontier
QueryNodes{experiment_id, updated_after=last_seen_counter}          → incremental catch-up
Stats{experiment_id}                                                → sanity + dashboards
```

`created_at`/`updated_at` logical counters give a stable cursor (strictly increasing per
txn), so incremental sync never misses or double-counts a mutation. Because node ids
are caller-assigned from the orchestrator's WAL, replaying the WAL after a crash
re-issues the same `CreateNode` calls — idempotency absorbs the duplicates.

Two warm-start rules the orchestrator's resume sequence (its ARCHITECTURE.md §8.2)
anchors on this store: the **frontier source of truth is the store's rows**
(`QueryNodes{statuses=[FRONTIER]}` above — the checkpoint supplies only selection
weights), and **`next_node_id` is re-derived as `1 + max(node_id)` over
`QueryNodes{experiment_id}`**, never from a checkpoint counter. Nodes committed after
the last checkpoint are thereby adopted, not re-keyed, and `ALREADY_EXISTS` stays what
§6 says it is: a bug, never a recovery artifact.

## 3. Pruning flow

```
orchestrator                                   snapshot-store
    │ policy decides subtree at node X is dead  │
    │ PruneSubtree{experiment_id, X} ──────────►│ txn: subtree CTE, status=PRUNED ∀,
    │◄──────────────── nodes_pruned=n ──────────│      tombstone(X)
    │                                           │
    │            … later, GC cycle (auto at 80% disk, or snapstorectl gc /
    │              TriggerGc) …                 │
    │                                           │ reap tombstones (delete rows+logs)
    │                                           │ mark from live nodes+pins (fence)
    │                                           │ sweep/compact packs, drop manifests
    │ Stats{} ─────────────────────────────────►│ gc_last_freed_bytes, dedup_ratio
```

Safety recap (ARCHITECTURE.md §4.5): pages shared with live siblings are marked via the
siblings' manifests and survive; anything committed after the fence is untouchable this
cycle; pins always mark.

## 4. Replay flow (replay-renderer)

Triggered when a GOAL node exists (or on operator demand for any node). Verification
is **per-segment**: each path edge is its own proof, chained into a whole-trajectory
proof by root-anchored induction through content-addressed base refs. The canonical
model is the hypervisor's INTEGRATION.md §3; the caller behavior below is
replay-renderer's INTEGRATION.md §1.1. There is no flat single-root re-execution.

```
replay-renderer                              snapshot-store              hypervisor (Intel)
    │ QueryNodes{experiment_id, statuses=[GOAL]}►│                            │
    │◄──────────────── goal node G ──────────────│                            │
    │ GetPath{experiment_id, G,                  │                            │
    │   include_input_logs=true} ───────────────►│ recursive CTE              │
    │◄── nodes root→G + log containers ──────────│                            │
    │ Pin(node_i.snapshot_ref, job_id) ∀i ──────►│  (EVERY path snapshot: each is a
    │                                            │   per-segment verification base;
    │                                            │   pins survive any pruning)
    │ verify container footers; assemble ordered │                            │
    │ (base_ref_{i-1}, DHILOG segment_i) pairs   │                            │
    │ for each edge i in 1..=n:                  │                            │
    │   VerifyReplay{base=snapshot_ref_{i-1},    │                            │
    │     log_i} ────────────────────────────────────────────────────────────►│
    │                                            │◄─ GetSnapshot(ref_{i-1}) ──│ (flow
    │                                            │   + pages                  │  §2.1 a–c)
    │◄─ VerifyDone{end_state_hash_i} ────────────────────────────────────────│
    │   compare vs node_i's recorded state_hash (an orchestrator-written attr)│
    │ all n edges ✓ ⇒ trajectory certified; frame capture + render on Spark   │
    │ Unpin(node_i.snapshot_ref) ∀i ────────────►│                            │
```

Per-segment adjacency replaces any whole-log concatenation: segment *i*'s base ref
must equal the verified child ref of segment *i−1*, and content-addressed refs make
the induction sound (equal ref ⇒ bit-identical state). The per-segment re-execution
**is** the proof (MAP.md principle 4). The recorded `state_hash` each segment is
checked against lives in node `attrs`, written by the orchestrator at commit — the
`.spm` manifest carries pages + device blob only, no metadata section. This store's
part is deliberately small: path resolution, byte-identical log containers, pins that
hold **every** base snapshot alive for the duration of the job, and the restore fast
path the hypervisor uses per segment. replay-renderer never writes here (its
INTEGRATION.md §1.2); spliced `.dilog` containers go to the artifact registry, not
this store (API.md §3).

## 5. Observability consumers (later phases)

- **observatory**: polls `Stats` (5 s) and, per live experiment,
  `QueryNodes{experiment_id, updated_after=cursor}` (1 s) to drive the live tree map.
  Read-only; uses TCP gRPC from the Spark.
- **control-plane**: `detctl snapstore stats|fsck|gc` proxies to `Stats`/`TriggerGc`
  and the CLI; owns the proto file long-term (API.md §5).

## 6. Failure-mode matrix (what callers must handle)

| Caller op | Failure | Caller action |
|---|---|---|
| PutSnapshot | FAILED_PRECONDITION MissingPages | re-send listed pages (PUT_BATCH/PutPages), retry once; if it repeats → P0 (durability ordering bug) |
| PutSnapshot / PutPages | RESOURCE_EXHAUSTED (disk watermark) | orchestrator pauses expansion, fires TriggerGc, alerts |
| CreateNode | FAILED_PRECONDITION (parent PRUNED) | drop the child (race with pruning is benign); orphan manifest GC'd |
| CreateNode | NOT_FOUND snapshot_ref / input_log_id | bug in worker commit ordering (§2.1: log durable, then manifest, then node row) — P0, halt experiment |
| CreateNode | timeout / UNAVAILABLE | **blind-retry the identical request** — idempotent on `(experiment_id, node_id)`; the retry returns the stored NodeMeta if the first attempt landed |
| CreateNode | ALREADY_EXISTS | node-id reuse with different content — orchestrator WAL bug, P0, halt experiment |
| PutMetadata / DeleteMetadata | FAILED_PRECONDITION (CAS mismatch) | another writer holds the key (or own retry raced its first attempt): re-read with GetMetadata; a checkpoint-CAS loss means **stand down** (single-writer rule, §2.3) |
| GET_BATCH | ERROR NOT_FOUND | only possible for refs never committed or post-prune-GC; treat as P0 if ref came from a live node |
| any | UNAVAILABLE (startup/fsck) | exponential backoff ≤ 30 s; workers idle, orchestrator pauses the loop |
| store crash | — | restart runs recovery (ARCHITECTURE.md §8); committed refs/nodes are durable by the ordering contract; clients simply retry in-flight ops (all idempotent by content/id) |

Idempotency summary: PutPages/PUT_BATCH (content), PutSnapshot (content), PutInputLog
(content), **CreateNode (caller-assigned `(experiment_id, node_id)` key)** — every
hot-loop write is safely blind-retryable. The only non-idempotent writes are metadata
CAS operations, which fail loudly (`FAILED_PRECONDITION` + current generation) rather
than double-apply.
