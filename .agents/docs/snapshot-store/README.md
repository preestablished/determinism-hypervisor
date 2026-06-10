# snapshot-store

Content-addressed, page-deduplicated storage for guest snapshots, plus the persistent
lineage metadata (the exploration state tree) for Project Determinism.

**Read [`../MAP.md`](../MAP.md) first.** This service is item 1 in the build order,
co-developed with `determinism-hypervisor`. It runs on the **Intel box** on local NVMe,
co-located with all hypervisor execution workers.

## Purpose

The exploration loop (MAP.md dataflow) forks guest state thousands of times per minute.
Every fork produces a child snapshot that differs from its parent by a small set of dirty
pages. `snapshot-store` makes this affordable:

1. **Page store** — guest memory is stored as 4 KiB pages addressed by their BLAKE3-256
   hash. A page is stored exactly once no matter how many snapshots reference it. Sibling
   forks share almost every page, so the marginal cost of a snapshot is its dirty delta.
2. **Snapshot manifests** — a snapshot is a small, content-addressed manifest: parent
   reference + dirty-page delta (or a full page list for roots) + the hypervisor's opaque
   device/vCPU state blob. The manifest hash is the platform-wide **snapshot reference**
   (MAP.md "Key cross-service contracts").
3. **Lineage / state-tree DB** — every exploration node {experiment, id, parent,
   snapshot ref, input log ref, scores, status, visit stats, logical timestamps} is
   persisted here and queryable by the orchestrator. Trees are **experiment-scoped**
   (one store holds many independent experiment trees) and node IDs are
   **caller-assigned** (`u64`, unique per experiment, root = 0), which makes node
   creation idempotent. This service is the *single source of truth* for the tree; the
   orchestrator holds only a working cache.
4. **Input-log blobs** — the small append-only artifacts that, together with a snapshot,
   make every result replayable. Stored content-addressed and referenced by tree nodes.
5. **Metadata KV** — a small durable key→value map with compare-and-swap (generation
   numbers), housing the orchestrator's checkpoints and write-ahead log so the
   orchestrator host itself can stay stateless.
6. **Pruning & GC** — delete subtrees the orchestrator has abandoned, then garbage-collect
   pages no longer reachable from any live manifest, with safety rules that make it
   impossible to collect a reachable page.

## Capabilities (normative)

| Capability | Summary |
|---|---|
| PutPages | Streaming ingest of 4 KiB pages; server hashes, dedups, persists. Idempotent. |
| PutSnapshot | Commit a manifest (full or delta) + device blob; returns the snapshot ref. Crash-consistent: a returned ref is durable. |
| GetSnapshot / ResolvePages | Resolve a snapshot ref to its manifest and stream the materialized page set (delta chain flattened server-side). |
| PutInputLog / GetInputLog | Store/fetch input-log blobs (opaque payload, versioned container). |
| CreateNode / UpdateNodes / GetNode / GetChildren / GetPath / QueryNodes | Experiment-scoped tree CRUD with caller-assigned node IDs (CreateNode idempotent on `(experiment_id, node_id)`), bulk attribute updates, root-path resolution for replay, filtered scans (e.g. all `FRONTIER` nodes above a score). QueryNodes is the only scan primitive. |
| PutMetadata / GetMetadata / DeleteMetadata | Durable KV with `expected_generation` compare-and-swap; namespaced keys (e.g. `orch/ckpt/<exp>`); orchestrator checkpoints/WAL and single-writer enforcement. |
| PruneSubtree | Tombstone a subtree; pages are reclaimed later by mark-and-sweep GC. |
| Stats | Tree statistics + store statistics (dedup ratio, physical bytes, GC state). |
| Fast path | Unix-domain-socket gRPC plus a shared-memory page channel (fd-passing) for co-located hypervisor workers, avoiding TCP and per-page copies. |

## Non-goals

- **No frontier policy.** Which node to expand next is the `exploration-orchestrator`'s
  decision. This service persists and queries; it never ranks, samples, or schedules.
- **No `ReleaseSnapshot`, no refcounting.** Discarded/uncommitted children are
  unreachable orphans reclaimed by mark-and-sweep GC (ARCHITECTURE.md §4.1); there is
  nothing for a caller to release.
- **No artifact resolution.** Experiment bootstrap (workload image → root snapshot →
  root node) is owned by `exploration-orchestrator`; this store has no image/artifact
  concept.
- **No interpretation of payloads.** Device/vCPU blobs and input-log payloads are opaque
  bytes owned by `determinism-hypervisor` (it versions their inner formats). We store,
  hash, and return them byte-identically — that is part of the determinism contract.
- **No scoring.** Scores are written by the orchestrator (sourced from `state-scorer`);
  we store floats.
- **No network distribution.** Single-node service on the Intel box's NVMe. *Live*
  replication and tiering to the DGX Spark are out of scope for v1; scheduled **cold
  backups** to the Spark/external storage, with a restore drill, are in scope (M9,
  ARCHITECTURE.md §8) — the store is the only durable copy of a campaign.
- **No guest execution, no replay verification.** `replay-renderer` re-executes; we only
  hand it the path and the logs.
- **No authn.** Trusted lab network; `control-plane` fronts external access later.

## Documents

| File | Contents |
|---|---|
| [ARCHITECTURE.md](ARCHITECTURE.md) | Crate layout, on-disk layout, pack files, dedup & GC design, metadata DB schema, concurrency, durability, performance engineering |
| [API.md](API.md) | Complete gRPC surface (tonic), byte-level manifest and input-log container formats, the local fast-path protocol |
| [INTEGRATION.md](INTEGRATION.md) | Sequence flows with determinism-hypervisor, exploration-orchestrator, replay-renderer |
| [IMPLEMENTATION-PLAN.md](IMPLEMENTATION-PLAN.md) | Ordered milestones, acceptance criteria, benchmarks, testing strategy, risks |

## Glossary

| Term | Definition |
|---|---|
| **Page** | Exactly 4096 bytes of guest physical memory. The unit of dedup and transfer. |
| **Page hash** | BLAKE3-256 (32 bytes) of the page's 4096 bytes. The page's identity and address. |
| **Page index** (guest) | `u64` index of a page within guest physical memory: `gpa / 4096`. |
| **Pack** | Append-only on-disk file containing many page records (`.spp`), with a sidecar index (`.sppx`). The unit of GC compaction. |
| **Page index (store)** | The in-memory map `page_hash → (pack_id, offset)` rebuilt from sidecars at startup. |
| **Manifest** | The byte-precise snapshot description (`.spm` file): full page list or parent+delta, plus the device blob. See API.md §2. |
| **Snapshot ref** | BLAKE3-256 of the manifest's canonical bytes. 32 bytes. Globally unique, content-derived, immutable. |
| **Delta manifest** | Manifest carrying only pages that differ from its parent manifest. Resolution flattens the chain. |
| **Device blob** | Opaque, versioned bytes from the hypervisor: vCPU registers, in-kernel + emulated device state, virtual-time state. Embedded in the manifest container. |
| **Input log** | Opaque, versioned record of every injected event between two snapshots (canonical schema owned by `determinism-hypervisor`). Stored in a hashed container; `log_id` = BLAKE3 of container minus footer. |
| **Experiment** | One independent exploration tree, keyed by a caller-chosen `experiment_id` string. A store holds many; page storage dedups across all of them. |
| **Node** | One vertex of an experiment's tree: snapshot ref + input log ref + scores + status + stats. |
| **Node ID** | Caller-assigned `u64`, unique within its experiment; `0` is always the experiment root. Makes `CreateNode` idempotent on `(experiment_id, node_id)`. |
| **Status** | One of `FRONTIER`, `EXPANDED`, `PRUNED`, `GOAL`. Stored verbatim; semantics owned by the orchestrator. |
| **Generation** | Per-key monotonic counter in the metadata KV; the compare-and-swap token for `PutMetadata`/`DeleteMetadata`. |
| **Logical counter** | Monotonic `u64` (`created_at`, `updated_at`) issued by this service; total order over tree mutations, independent of wall clock. |
| **Pin** | An explicit hold on a snapshot ref that makes it a GC root regardless of tree state (used by replay-renderer and operators). |
| **Tombstone** | Record of a pruned subtree root awaiting sweep. |
| **GC epoch fence** | Logical-counter value snapshotted at mark start; anything created at-or-after the fence is unconditionally live for that GC cycle. |
| **Fast path** | The local interface for co-located workers: gRPC over UDS for control, plus the SEQPACKET + memfd page channel for bulk page bytes. |

## Conventions honored (MAP.md)

- Rust 2021+, `tonic` gRPC; on-disk formats are hand-specified binary (this doc) with
  `postcard` used only for the extensible `attrs` blob. Every persisted format carries an
  explicit version field.
- The canonical `snapshot_store.proto` is contributed to the shared proto set versioned
  in `control-plane`; until that repo exists, the file lives at `proto/snapshot_store.proto`
  in this repo and is the temporary source of truth.
- Exposes `/healthz` (HTTP, port 7411), Prometheus metrics (`/metrics`, port 7411), and
  structured JSON logs (`tracing` + `tracing-subscriber` JSON formatter).
- Determinism regression: CI includes a store-roundtrip test — commit snapshot, restore,
  byte-compare every page and the device blob (see IMPLEMENTATION-PLAN.md).

## Default ports & paths

| Item | Value |
|---|---|
| gRPC (TCP, for off-box callers: orchestrator/scorer on Spark) | `0.0.0.0:7410` |
| gRPC (UDS fast path) | `/run/snapstore/grpc.sock` |
| Page channel (SEQPACKET + memfd) | `/run/snapstore/pages.sock` |
| Health + metrics (HTTP) | `0.0.0.0:7411` |
| Data root | `/var/lib/snapstore` (NVMe mount) |
