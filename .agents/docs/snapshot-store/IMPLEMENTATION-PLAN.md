# snapshot-store — Implementation Plan

Ordered milestones. Each has acceptance criteria (AC) and, where relevant, benchmarks
(BM) run by `cargo bench` (criterion) or `snapstorectl bench` on the Intel box's NVMe.
Do not start milestone N+1 until N's ACs pass in CI. snapshot-store is build-order
phase 1 (with `determinism-hypervisor`); until the hypervisor exists, all integration
testing uses the **synthetic guest generator** built in M0.

## M0 — Skeleton, types, baselines

- Workspace per ARCHITECTURE.md §1; `snapstore-types` complete; CI (fmt, clippy -D
  warnings, test); `/healthz` + `/metrics` HTTP stub; JSON tracing; `config.toml` loader.
- **Synthetic guest generator** (`tests/synthgen`): produces deterministic fake guests —
  128 MiB images seeded by u64, plus "burst mutation" (dirty 256–2,048 random-but-seeded
  pages). This stands in for the hypervisor in every test until integration.
- Record NVMe baseline with `fio` (seq write/read QD32, 4k randread) into
  `docs/bench-baseline.md`; all later BM targets are sanity-checked against it.
- **AC:** CI green; `snapstorectl bench fio-baseline` writes the baseline file.

## M1 — Page store core (packs, index, ingest)

- Pack format + sidecar (ARCHITECTURE.md §2.1–2.3): append, seal, fallocate, crc32c;
  startup rebuild incl. torn-tail truncation; sharded in-memory index; rayon batch
  hashing; single pack-writer task with commit barrier; zero-page short-circuit.
- **AC:**
  - Ingest 1 M synthetic pages, restart process, index identical (full compare).
  - Torn-tail test: truncate open pack at every byte offset of the last record
    (parameterized); startup always recovers to the last whole record, no panic.
  - Dedup: ingesting the same 100k pages twice stores them once (`pages_new==0` second
    pass).
- **BM:** single-stream ingest ≥ 1.5 GB/s (pre-hashed memory source); hash+ingest
  ≥ 1.0 GB/s; index probe ≥ 5 M lookups/s across 8 threads.

## M2 — Manifest codec + snapshot commit/resolve

- `snapstore-manifest`: encode/decode/validate/flatten per API.md §2, pure, fuzzable.
- `PutSnapshot` path with full validation incl. missing-page detection and the
  pages→manifest fsync ordering; loose `.spm` write discipline; flatten LRU.
- **AC:**
  - Round-trip property: ∀ generated manifests, `decode(encode(m)) == m` and ref stable.
  - Canonicality: shuffled-input entries encode to identical bytes (sort enforced).
  - `cargo fuzz` target on `Manifest::decode` runs 10 min in CI nightly, no crashes.
  - Flatten correctness vs a naive reference implementation (proptest, chains ≤ 64).
  - Commit-with-missing-pages returns `FAILED_PRECONDITION` listing exactly the gaps.
- **BM:** flatten 64-deep chain of 2k-entry deltas < 2 ms warm; PutSnapshot (manifest
  already-paged) p50 < 3 ms.

## M3 — Metadata DB (`snapstore-meta`)

- Schema v1 DDL (**experiment-scoped** `nodes`/`tombstones` with composite keys, the
  `kv_metadata` CAS table — ARCHITECTURE.md §5.3), migrations table, meta actor + read
  pool, logical counter, all canonical queries (ARCHITECTURE.md §5.4), caller-assigned
  node-id handling (u64↔i64 bit-cast, idempotent insert path), metadata KV with
  generation CAS, input-log container validation + storage, pins, tombstones,
  PruneSubtree transaction.
- **AC:**
  - 1 M-node synthetic tree (branching ~8): GetPath(depth 5k) < 40 ms p99; QueryNodes
    frontier scan streams correctly with `created_after` cursor (no gaps/dupes under
    concurrent writes — verified by interleaving test).
  - **CreateNode idempotency:** replaying any prefix of a synthetic experiment's
    CreateNode stream (duplicates included, any interleaving) yields a byte-identical
    tree; key reuse with different content ⇒ `ALREADY_EXISTS`, zero rows changed.
  - **Multi-experiment isolation:** two interleaved synthetic experiments sharing page
    content never observe each other's nodes via any tree RPC; per-experiment Stats
    match per-driver bookkeeping.
  - **KV CAS:** concurrent writers hammering one key (interleaving test) ⇒ exactly one
    winner per generation, losers get `FAILED_PRECONDITION` + current generation;
    create-only (`expected_generation=0`) and delete-CAS paths covered; value-cap
    (16 MiB) rejection covered.
  - UpdateNodes atomicity: one bad id ⇒ zero rows changed.
  - Kill -9 during a 256-update batch ⇒ on restart the batch is wholly present or
    wholly absent (loop ×200 in the crash harness, see M6).
- **BM:** CreateNode+inline log (16 KiB) p50 < 1.5 ms; UpdateNodes(256) p50 < 3 ms;
  PutMetadata (64 KiB value) p50 < 2 ms; sustained ≥ 5k node-mutations/s through the
  actor.

## M4 — gRPC surface + client lib

- All RPCs in API.md §1 on TCP + UDS (tree CRUD, **metadata KV**, lifecycle);
  structured error details (`MissingPages`, `MissingNodes`, `CurrentGeneration`);
  `snapstore-client` with transport fallback, footer verification, retry policy
  (every content/key-idempotent op blind-retries — including CreateNode, per
  INTEGRATION.md §6; CAS ops surface `FAILED_PRECONDITION` to the caller, never
  auto-retry); `snapstorectl` subcommands
  `stats|dump-manifest|get-node|query|prune|pin|kv|fsck|gc|bench`.
- **AC:** end-to-end test: synthetic "exploration" of 10k steps across **two
  concurrent experiments** through the public API only (commit, create, update, query,
  path, checkpoint-KV writes with CAS) — final per-experiment Stats consistent with
  each driver's own bookkeeping; injected timeouts force CreateNode blind-retries with
  no duplicate nodes; tonic health + Prometheus counters populated.
- **BM:** PutPages over UDS gRPC ≥ 600 MB/s; QueryNodes page of 1,000 p50 < 4 ms over UDS.

## M5 — Fast path (page channel)

- SEQPACKET + memfd protocol per API.md §4, server + client halves; auto-selection in
  `snapstore-client`; cross-check hash plumbing; OVERLOAD backpressure (bounded ingest
  queue).
- **AC:** PUT/GET round-trip property test (random batches, sizes 1..8192); a killed
  client mid-batch leaks nothing (server-side fd audit via `/proc/self/fd` count before
  = after); cross-check mismatch path unit-tested (corrupted memfd ⇒ ERROR + metric).
- **BM:** PUT_BATCH ingest ≥ 1.5 GB/s sustained; GET_BATCH ≥ 2.5 GB/s warm; 16 parallel
  clients each committing 8 MiB deltas: p99 commit (PUT+PutSnapshot) < 40 ms, aggregate
  ≥ 1.2 GB/s. **These numbers gate MAP.md principle 2 — treat misses as release
  blockers, not soft targets.**

## M6 — Durability: crash-injection harness

- Harness in `tests/crash/`: child process runs scripted workloads; parent SIGKILLs at
  randomized (seeded, reproducible) points; restart + `fsck --deep` + invariant checks.
  Add **failpoints** (`fail` crate, compiled under a feature) at every fsync/rename
  boundary in the commit pipeline so kills can be targeted, not just timed.
- Invariants checked after every recovery:
  1. Every node row's `snapshot_ref` resolves; every manifest entry resolves to a page
     whose stored bytes hash to its key (deep fsck).
  2. A `PutSnapshot` that returned success is durable; one that didn't return may be
     wholly absent (never partially visible).
  3. Same for CreateNode/UpdateNodes batches.
- **AC:** 1,000 randomized crash cycles in nightly CI with zero invariant violations;
  the failpoint matrix (each of the ~9 ordering boundaries × kill) passes ×50 each.

## M7 — GC: mark-and-sweep + pruning end-to-end

- Mark (with epoch fence + commit gate), tombstone reaping, pack compaction with
  index repoint + retry-on-race read path, manifest sweep, `TriggerGc`, auto-trigger at
  disk watermark, `gc_*` metrics.
- **Property tests (proptest, model-based)** — the centerpiece:
  - Model: a reference implementation tracking exact reachable-page sets. Generate
    random op sequences (commit chains, fork siblings, prune subtrees, pin/unpin, GC at
    random points, **concurrent commits during GC** via controlled interleaving).
    Invariants: (a) GC never removes a reachable page or manifest [safety R1]; (b) after
    a quiescent GC, physical pages == model's reachable set exactly [completeness];
    (c) reads served during GC always return correct bytes [R2 retry].
  - Refcount-free oracle: model uses brute-force mark from scratch each step.
- **AC:** property suite (≥ 10k cases nightly, ≥ 500 in PR CI) green; crash harness
  extended with kills inside GC (compaction copy, index repoint, unlink) — recovery
  never loses reachable data, at worst leaks space reclaimed by the next cycle.
- **BM:** GC of a 100k-node tree (~30 GB physical, 50% garbage after pruning) completes
  < 60 s with concurrent 200 MB/s ingest load; p99 commit latency during GC < 2× idle.

## M8 — Hypervisor integration + determinism regression (joint milestone)

Runs when `determinism-hypervisor` reaches its fork/restore milestone (MAP.md build
order 1 shares this gate: *fork a guest 1000× and verify bit-identical re-execution*).

- Wire the worker to `snapstore-client` (flows in INTEGRATION.md §1–2), including the
  FULL-manifest cadence and baseline-delta restore.
- **AC (the platform milestone):** fork one guest 1000× through snapshot-store; restore
  each child and re-execute its burst; every re-execution's `PutSnapshot` returns a ref
  identical to the original child's ref (content-address equality = bit-identity proof).
  This test becomes the permanent **determinism regression** in both repos' CI
  (MAP.md convention).
- **BM (the real numbers):** measured fork→commit and restore latencies with the real
  guest fit the ARCHITECTURE.md §7.1 table; measured sibling dedup ≥ 94% shared pages;
  record actuals in `docs/bench-baseline.md`.

## M9 — Operability polish + cold backup

This store is the **only durable copy** of a Phase 8 campaign — the tree, every
snapshot, and every input log of a multi-week search exist exactly once, on one NVMe.
M9 therefore ships a real backup story, not a stub; Phase 8's entry gate requires the
restore drill below to have passed.

- `fsck_interval` scheduled deep scans at idle io-priority; RESOURCE_EXHAUSTED
  watermarks; dashboards-ready metric names finalized; proto file handed to
  `control-plane` when that repo opens; README runbook section (start, fsck, gc,
  backup/restore, disk-full recovery).
- **Cold backup** (`snapstorectl backup --to <dest>`, schedulable via `[backup]`
  config): periodic consistent copy to the DGX Spark or external storage —
  crash-consistency point via the `gc_commit_gate`, then incremental ship of sealed
  packs + sidecars (append-only ⇒ copy-once), the loose-manifest dir, a `VACUUM INTO`
  snapshot of `tree.db` (including `kv_metadata`, so the orchestrator's checkpoints
  travel with the tree), and the unsealed pack's valid tail (ARCHITECTURE.md §8).
  Backup runs concurrently with normal commits; only the consistency-point instant
  takes the gate.
- **Restore drill** (documented runbook step, automated in CI on synthetic data):
  restore a backup onto a clean data root, start the server, pass `fsck --deep`, and
  resume a synthetic exploration from the backed-up orchestrator checkpoint — proving
  the "campaign survives a dead NVMe, minus at most one backup interval" claim.
- **AC:** disk-full simulation (small loopback fs): commits refuse cleanly at 95%, GC
  recovers space, no corruption per deep fsck; backup-while-committing produces a
  restorable image (loop in crash harness); restore drill green in CI and performed
  once manually against the Spark before Phase 8 entry; docs reviewed against
  implemented behavior (no drift).

## Testing strategy summary

| Layer | Technique |
|---|---|
| Manifest/log codecs | proptest round-trip + canonicality, cargo-fuzz decode |
| Page store | torn-tail matrix, restart-equivalence, dedup invariants (proptest) |
| Dedup/GC | model-based proptest with interleaved GC + commits (M7) — safety & completeness invariants |
| Durability | crash-injection harness with failpoints at every fsync/rename boundary (M6), kill-loops on SQLite batches |
| API | end-to-end synthetic exploration driver (multi-experiment); idempotency/retry tests incl. CreateNode blind-retry and KV CAS contention |
| Performance | criterion micro-benches + `snapstorectl bench` macro runs, gated against fio baseline; perf regression check in nightly CI (±15% tolerance) |
| Determinism | M8 ref-equality regression, permanent in CI (P0 on failure, per MAP.md) |
| Concurrency | loom tests for the index-repoint/read race and the gc_commit_gate; tsan job nightly |

## Risks & mitigations

| # | Risk | Mitigation |
|---|---|---|
| R1 | BLAKE3 hashing starves worker vCPUs (shared box) | dedicated bounded rayon pool (`ingest_hash_threads`), measure with workers under load in M8; fallback: pin hash pool to dedicated cores via cpuset |
| R2 | In-memory page index outgrows RAM at extreme node counts | 40 B/page; alert metric on index size; v2 design noted (cold-shard spill); demo scale leaves >10× headroom |
| R3 | Deep delta chains make flatten/restore slow | `max_delta_chain`=64 cadence enforced by worker + flatten LRU; M2 BM gates flatten cost; monitor `snapstore_flatten_depth` histogram |
| R4 | SQLite writer becomes the step bottleneck at high parallelism | batching actor (256/txn) measured to ≥5k mutations/s in M3 — 10× the planned step rate; if exceeded, shard UpdateNodes batching window upward before considering a DB swap |
| R5 | GC compaction I/O trashes commit latency | sweep only below `compact_threshold`, io-priority idle class for GC reads, M7 BM gates p99-under-GC ≤ 2× idle |
| R6 | memfd fast path still too slow (copy + syscall overhead) | numbers gated in M5; escape hatch documented: persistent shared-memory ring per worker — only build if M5/M8 BMs fail |
| R7 | Hypervisor's dirty-page tracking under-reports (missed dirty page ⇒ wrong child state, silent) | not detectable here by design (we store what we're given) — but M8's ref-equality regression catches it the first time a replay diverges; surface `pages_deduped==dirty_set` anomaly metric as an early hint |
| R8 | Disk fills before GC reclaims | 80% auto-GC trigger, 95% hard refusal (RESOURCE_EXHAUSTED), orchestrator pause protocol (INTEGRATION.md §6), M9 disk-full drill |
| R9 | Schema evolution breaks replayability of old experiments | explicit versions everywhere (containers, STORE_VERSION, schema_version, proto v1); readers reject unknown versions loudly; old data is migrated by explicit `snapstorectl migrate`, never silently |
| R10 | NVMe failure during a multi-week campaign loses the only durable copy of the tree, snapshots, and logs | M9 cold backup (incremental, consistent, to Spark/external) + restore drill; Phase 8 entry gate requires a verified restore; loss bounded to one backup interval |

## Definition of done (service v1)

All M0–M9 ACs green in CI; M5/M8 benchmark gates met on the Intel box; determinism
regression installed in both this repo's and the hypervisor's CI; docs (these five
files) updated to match as-built behavior.
