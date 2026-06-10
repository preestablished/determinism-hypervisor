# snapshot-store — Architecture

Single-node Rust service on the Intel box. Local NVMe is the only storage tier.
Everything below is normative for implementation.

## 1. Crate layout

Cargo workspace, repo root `snapshot-store/`:

```
snapshot-store/
├── Cargo.toml                  # [workspace] members below
├── proto/snapshot_store.proto  # canonical until control-plane exists
├── crates/
│   ├── snapstore-types/        # shared types: PageHash, SnapshotRef, NodeId, errors
│   ├── snapstore-pagestore/    # packs, page index, ingest pipeline, GC mark/sweep
│   ├── snapstore-manifest/     # manifest encode/decode/flatten (no I/O beyond &[u8])
│   ├── snapstore-meta/         # SQLite (rusqlite) lineage DB: schema, queries, actor
│   ├── snapstore-localpath/    # SEQPACKET+memfd page channel (server + client lib)
│   ├── snapstore-server/       # tonic services, health/metrics, wiring, main()
│   ├── snapstore-client/       # Rust client lib used by hypervisor/orchestrator/replay
│   └── snapstore-cli/          # `snapstorectl`: fsck, gc, stats, dump-manifest, bench
└── tests/                      # cross-crate integration + crash-injection harness
```

Dependency rules: `types ← {pagestore, manifest, meta, localpath} ← server`;
`client` depends only on `types` + generated proto + `localpath` (client half).
`manifest` is pure (fuzzable, no I/O). `pagestore` and `meta` know nothing about gRPC.

Key dependencies (pin in workspace `Cargo.toml`): `tonic`, `prost`, `tokio` (rt-multi-thread),
`rusqlite` (bundled SQLite, `serde_json` feature off), `blake3` (with `rayon` feature for
batch hashing), `zstd`, `postcard`, `serde`, `tracing`, `prometheus`, `nix` (UDS ancillary
fd passing, memfd), `crossbeam-channel`, `parking_lot`, `proptest` (dev), `criterion` (dev).

### Core types (`snapstore-types`)

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PageHash(pub [u8; 32]);          // BLAKE3-256 of exactly 4096 bytes

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SnapshotRef(pub [u8; 32]);       // BLAKE3-256 of manifest bytes sans footer

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct LogId(pub [u8; 32]);             // BLAKE3-256 of log container sans footer

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u64);                 // CALLER-assigned; unique per experiment; root = 0

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ExperimentId(pub String);        // caller-chosen, 1..=128 UTF-8 bytes

pub const PAGE_SIZE: usize = 4096;

#[repr(u8)]
pub enum NodeStatus { Frontier = 0, Expanded = 1, Pruned = 2, Goal = 3 }

pub struct PackLoc { pub pack_id: u32, pub offset: u64 } // value of the page index
```

Node identity is the composite `(ExperimentId, NodeId)`: the store holds many
independent experiment trees, and node ids are **assigned by the orchestrator** (its
WAL/commit order), never minted here. That is what makes `CreateNode` idempotent
(API.md §1.4). Ordering guarantees come from the logical counter, never from node-id
values.

## 2. On-disk layout

```
/var/lib/snapstore/
├── STORE_VERSION                 # ASCII "1\n"; refuse to start on mismatch
├── store.uuid                    # 16-byte random store identity (written once)
├── config.toml
├── pages/
│   ├── packs/
│   │   ├── pack-00000001.spp     # sealed pack (append-only while open)
│   │   ├── pack-00000001.sppx    # sidecar index, written at seal time
│   │   ├── pack-00000002.spp     # ... the single currently-open pack has no .sppx yet
│   └── gc/
│       └── mark-<epoch>.state    # resumable mark bitmap (deleted after sweep)
├── manifests/
│   └── ab/abcd…ef.spm            # loose manifest containers, sharded by first byte (hex)
├── meta/
│   ├── tree.db                   # SQLite (WAL): nodes, input_logs, pins, tombstones, kv_metadata
│   ├── tree.db-wal
│   └── tree.db-shm
└── tmp/                          # same-filesystem staging for atomic renames
```

### 2.1 Page pack files (`.spp`)

Packs, not loose page files: 4 KiB loose files waste inodes, cost one `open()` per page,
and defeat NVMe sequential bandwidth. A pack is an append-only log of page records:

```
Pack header (64 bytes):
  off  size  field
  0    8     magic              = b"SPPACK01"
  8    2     version u16 LE     = 1
  10   2     flags u16 LE       = 0
  12   4     pack_id u32 LE
  16   16    store_uuid
  32   8     created_epoch u64 LE   (logical counter at creation)
  40   24    reserved (zero)

Page record (repeated, 8-byte aligned; record stride = 4144 bytes):
  0    4     rec_magic u32 LE   = 0x50524543 ("CREC")
  4    4     rec_flags u32 LE   (bit0: zstd-compressed payload — v1 always 0, raw)
  8    32    page_hash          (BLAKE3 of the *uncompressed* 4096 bytes)
  40   4     payload_len u32 LE (= 4096 for raw)
  44   4     crc32c of payload  (cheap torn-write detector; BLAKE3 verify is opt-in)
  48   4096  payload
```

- Pack size cap: 1 GiB (`pack_max_bytes`, ≈ 259k pages); the writer seals the pack
  (writes `.sppx`, fsyncs both, opens the next) when the cap is reached.
- Page payloads are stored **raw** in v1. Guest pages of a 16-bit-era emulator dedup so
  well that compression buys little; `rec_flags` bit0 reserves per-record zstd for v2.

### 2.2 Pack sidecar index (`.sppx`)

```
  0    8     magic = b"SPPIDX01"
  8    2     version u16 = 1
  10   2     reserved
  12   4     pack_id u32 LE
  16   8     entry_count u64 LE
  24   N×40  entries, sorted by page_hash: { page_hash [32], offset u64 LE }
  end  32    BLAKE3 of all preceding bytes
```

Authoritative location data = sidecars. At startup the in-memory page index is rebuilt by
reading every `.sppx` (sequential, fast) and then **scanning the tail of the single
unsealed pack** record-by-record, validating `rec_magic` + crc32c, truncating at the
first torn record (`ftruncate` to last good boundary). This is the crash-recovery path
for pages; no separate WAL needed because packs are append-only and self-framing.

### 2.3 In-memory page index

`page_hash → PackLoc`, sharded 256 ways by `hash[0]`:

```rust
struct PageIndex { shards: [parking_lot::RwLock<hashbrown::HashMap<PageHash, PackLoc>>; 256] }
```

~40 B/entry ⇒ 10 M unique pages ≈ 400 MB RAM. Demo-scale unique-page counts (see §7
dedup math) stay in the low millions; the Intel box's RAM covers 100 M pages worst-case.
v2 escape hatch (documented risk, not built now): spill cold shards to an on-disk hash
table. Dedup check on ingest = one shard read-lock + map probe.

### 2.4 Manifests

One loose `.spm` file per snapshot under `manifests/<first-byte-hex>/<hash-hex>.spm`.
They are small (40 B/entry: a 2,048-page delta ≈ 80 KiB; a full 32,768-page manifest for
a 128 MiB guest ≈ 1.3 MiB) and written once. Byte format in API.md §2. Written via
`tmp/` + `fsync(file)` + `rename(2)` + `fsync(parent dir)`.

## 3. Snapshot commit pipeline (write path)

```
hypervisor worker                          snapstore-pagestore
  pages (fast path / PutPages) ──► ingest stage:
                                     1. hash each page (blake3, rayon batch)
                                     2. probe page index; drop duplicates
                                     3. enqueue novel pages → single pack-writer task
                                   pack writer (one per store):
                                     4. append records to open pack (pwritev, batched)
                                     5. fdatasync on batch boundary / commit barrier
                                     6. publish PackLoc into page index
  PutSnapshot(manifest) ──────────► 7. verify every referenced page_hash is present
                                       AND durable (index entries carry a `synced` bit;
                                       barrier flushes if needed)
                                     8. write .spm atomically (tmp+fsync+rename+dirsync)
                                     9. return SnapshotRef
orchestrator
  CreateNode(experiment_id, node_id,
             snapshot_ref, input_log_id…) ► 10. SQLite txn inserts node row, referencing
                                              the log row the worker's PutInputLog stored
```

**Ordering invariant (crash consistency):** pages durable → input log durable →
manifest durable → node row durable (the worker's commit ordering, hypervisor API.md
§5.1; CreateNode validates that `input_log_id` exists). A crash between any two steps leaves only unreachable garbage (orphan pages or
an orphan manifest), never a dangling reference. Orphans are reclaimed by normal GC.
`PutSnapshot` rejects (`FAILED_PRECONDITION`, listing missing hashes) if any referenced
page is absent — the client retries `PutPages` for the gaps (idempotent).

Hashing is the CPU hot spot: BLAKE3 over 4 KiB ≈ 1–3 GB/s/core with AVX2; batch-hash on
a rayon pool sized `min(8, cores/2)` so workers' vCPU threads aren't starved.

## 4. Dedup and GC design

### 4.1 Why mark-and-sweep (not refcounting)

Per-page refcounts charge the **commit path**: a delta commit touching 2k pages would do
2k durable counter updates, and pruning a subtree would walk every manifest in it. With
content-dedup across thousands of sibling forks, refcount write amplification lands
exactly where the latency budget is tightest (MAP.md principle 2). Mark-and-sweep moves
all reclamation cost to a background cycle: commits stay O(novel pages), prune is a
metadata-only tombstone, and the sweeper does sequential pack rewrites at NVMe speed.
Cost accepted: space is reclaimed lazily, and a full mark touches every live manifest —
fine, manifests total < a few GiB even at 1 M nodes.

Corollary, stated normatively because clients keep reinventing it: there is **no
`ReleaseSnapshot` RPC and no refcount for callers to manage**. A child the orchestrator
discards (duplicate, regression, prune verdict) simply never gets a node row; its
manifest and novel pages are unreachable orphans that the next mark-and-sweep cycle
reclaims. "Discard" is the absence of `CreateNode`, not a call.

### 4.2 Roots

A page is **live** iff reachable from a root manifest or any of its parent-chain
ancestors. Root set =

1. `snapshot_ref` of every row in `nodes`, across **all** experiments (pruned subtrees
   are *deleted rows* by sweep time — see §4.4 — so this is "every non-deleted node"),
2. every ref in `pins`,
3. every manifest created at-or-after the **epoch fence** (see below).

Because delta manifests reference a parent manifest, mark must walk parent chains: a
node's pages = union over its manifest chain (child entries shadow parent entries for
the same guest page index, but for *liveness* every page hash named anywhere in any
chain manifest of a root is conservatively live — shadowed pages may still be needed by
a sibling and shadowing analysis buys nothing).

### 4.3 Mark phase

1. Record `fence = logical_counter` and the current open `pack_id` (`fence_pack`).
2. Snapshot the root set from SQLite (single read txn).
3. For each root, walk the manifest chain via `parent_manifest_hash`, inserting every
   `page_hash` into the mark set. Mark set = `hashbrown::HashSet<PageHash>` (same memory
   envelope as the page index). Memoize visited manifest hashes — chains share ancestors
   heavily.
4. Any page written to packs `>= fence_pack`, and any manifest committed after `fence`,
   is unconditionally live this cycle (it cannot have been marked, because its root
   appeared after the root-set snapshot). The sweep simply never touches packs
   `>= fence_pack`.

### 4.4 Prune → sweep pipeline

`PruneSubtree(experiment_id, node_id)` (one SQLite txn):
1. Verify node exists and is not its experiment's root (node_id 0) unless
   `allow_root=true`.
2. Recursive CTE collects the subtree's node ids.
3. Set `status=PRUNED` on all of them **and** insert a `tombstones` row for the subtree
   root; **delete is deferred**. (Two-phase so the orchestrator can still observe what
   was pruned, and so a crash mid-prune is trivially resumable.)

GC cycle (background, manual trigger via `snapstorectl gc` or timer):
1. **Reap tombstones:** delete all node rows belonging to tombstoned subtrees whose
   tombstone is older than `gc_tombstone_grace` (default 1 GC cycle); delete now-orphaned
   `input_logs` rows; delete tombstone rows. One txn per subtree.
2. **Mark** (§4.3).
3. **Sweep:** for each sealed pack `< fence_pack`, compute `live_bytes/total_bytes` from
   the mark set ∩ sidecar. If liveness < `gc_compact_threshold` (default 0.5), copy live
   records into the current open pack (they get fresh `PackLoc`s, index updated under
   shard write-lock **after** the new copies are fsynced), then seal-state delete: write
   `pack-N.spp.dead` marker, fsync dir, unlink `.spp`/`.sppx`, unlink marker. Packs at or
   above threshold are left alone (their dead bytes wait for a later cycle).
4. Delete manifests not visited during mark (same fence rule), same atomic-unlink
   discipline.

### 4.5 Safety rules (normative)

- **R1:** Never delete a page record reachable from any manifest chain rooted at a
  non-deleted node or pin. Enforced structurally: sweep deletes only whole packs below
  the fence whose live records have already been re-copied and re-indexed durably.
- **R2:** The page index entry for a hash must always point at a durable copy. During
  compaction the index is updated only after the new pack region is fsynced; the old pack
  is unlinked only after every live record's index entry points away from it.
- **R3:** `PutSnapshot` and mark-root snapshotting serialize on a `gc_commit_gate`
  (an `RwLock`: commits take read, the fence-taking instant takes write) so a manifest
  can never commit "between" root snapshot and fence record.
- **R4:** GC never runs concurrently with itself; a crashed GC leaves only extra copies
  (compaction is copy-then-delete), which the next cycle reclaims. `mark-<epoch>.state`
  enables resume but may simply be discarded.
- **R5:** `Pin` rows are GC roots, period. `replay-renderer` pins every snapshot on the
  goal path before a long render (INTEGRATION.md §4).

## 5. Lineage metadata DB

### 5.1 Choice: SQLite via `rusqlite`

Chosen over `redb`/`sled`:
- The orchestrator's query set (filtered scans over status+score, bulk updates,
  path-to-root, subtree collection, stats) maps directly onto secondary indexes,
  `UPDATE … WHERE id IN`, and **recursive CTEs**. On redb/sled (pure KV) every one of
  those becomes hand-rolled secondary-index maintenance with its own consistency bugs.
- WAL-mode SQLite gives single-writer/multi-reader concurrency that matches our model
  (one writer actor, many read snapshots), crash safety that has been beaten on for
  decades, and online backup (`VACUUM INTO`) for operator snapshots.
- sled is effectively unmaintained beta; redb is solid but KV-only. Storing input-log
  blobs inline (small, < 1 MiB) is a SQLite sweet spot and makes node+log insertion one
  atomic transaction.
- Scale sanity: 1 M nodes ≈ 200 MB including logs — trivial.

### 5.2 Connection / pragma discipline

- One **writer connection** owned by a dedicated blocking thread ("meta actor"); all
  mutations arrive on a `crossbeam` channel as commands carrying oneshot reply senders.
  Writes batch: the actor drains up to 256 queued commands into one transaction
  (`BEGIN IMMEDIATE … COMMIT`) — this is what makes bulk score updates cheap.
- A pool of N=4 **read connections** (`PRAGMA query_only=ON`) used directly from tokio
  via `spawn_blocking`; WAL gives them stable snapshots.
- Pragmas on every connection: `journal_mode=WAL`, `synchronous=FULL` (writer),
  `foreign_keys=ON`, `wal_autocheckpoint=4000`, `mmap_size=268435456`,
  `busy_timeout=5000`. `synchronous=FULL` keeps commit = durable; the batching actor
  amortizes the fsync.

### 5.3 Schema (DDL, schema_version = 1)

```sql
CREATE TABLE meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
) WITHOUT ROWID;
-- rows: ('schema_version','1'), ('store_uuid', hex), ('logical_counter', u64 as text)
-- logical_counter is flushed on every writer txn; on startup it is max(persisted,
-- max(created_at), max(updated_at)) + 1.

CREATE TABLE nodes (
  experiment_id  TEXT NOT NULL,               -- caller-chosen; many trees per store
  node_id        INTEGER NOT NULL,            -- caller-assigned u64 (stored as i64 bit-cast);
                                              -- root = 0, unique within its experiment
  parent_node_id INTEGER,                     -- NULL only for the root (node_id 0)
  depth          INTEGER NOT NULL,            -- root = 0; = parent.depth + 1 (enforced in code)
  snapshot_ref   BLOB NOT NULL,               -- 32 bytes
  input_log_id   BLOB REFERENCES input_logs(log_id), -- NULL for the root node
  status         INTEGER NOT NULL DEFAULT 0,  -- 0 frontier 1 expanded 2 pruned 3 goal
  progress_score REAL NOT NULL DEFAULT 0.0,
  novelty_score  REAL NOT NULL DEFAULT 0.0,
  visit_count    INTEGER NOT NULL DEFAULT 0,  -- times selected for expansion
  expand_count   INTEGER NOT NULL DEFAULT 0,  -- children actually committed
  last_visited_at INTEGER,                    -- logical counter, NULL = never
  created_at     INTEGER NOT NULL,            -- logical counter (total order)
  updated_at     INTEGER NOT NULL,            -- logical counter
  attrs          BLOB,                        -- postcard map<string,bytes>, orchestrator-private
  PRIMARY KEY (experiment_id, node_id),
  FOREIGN KEY (experiment_id, parent_node_id)
    REFERENCES nodes(experiment_id, node_id)
) WITHOUT ROWID;

CREATE INDEX idx_nodes_parent  ON nodes(experiment_id, parent_node_id);
CREATE INDEX idx_nodes_status  ON nodes(experiment_id, status, progress_score DESC);
CREATE INDEX idx_nodes_novel   ON nodes(experiment_id, status, novelty_score DESC);
CREATE INDEX idx_nodes_created ON nodes(experiment_id, created_at);
CREATE INDEX idx_nodes_snap    ON nodes(snapshot_ref);   -- GC root scan + ref lookups (global)

CREATE TABLE kv_metadata (
  key        TEXT PRIMARY KEY,                -- 1..=512 bytes, "/"-namespaced (API.md §1.5)
  value      BLOB NOT NULL,                   -- ≤ metadata_value_max_bytes (16 MiB)
  generation INTEGER NOT NULL,                -- 1 on create, +1 per successful write (CAS token)
  updated_at INTEGER NOT NULL                 -- logical counter
) WITHOUT ROWID;

CREATE TABLE input_logs (
  log_id         BLOB PRIMARY KEY,            -- 32 bytes (container hash, API.md §3)
  size_bytes     INTEGER NOT NULL,
  inner_version  INTEGER NOT NULL,            -- hypervisor's input-log format version
  content        BLOB NOT NULL,               -- the full container, ≤ 4 MiB (config cap)
  created_at     INTEGER NOT NULL
) WITHOUT ROWID;

CREATE TABLE pins (
  snapshot_ref   BLOB PRIMARY KEY,            -- 32 bytes
  reason         TEXT NOT NULL,
  created_at     INTEGER NOT NULL
) WITHOUT ROWID;

CREATE TABLE tombstones (
  experiment_id  TEXT NOT NULL,
  root_node_id   INTEGER NOT NULL,            -- subtree root that was pruned
  node_count     INTEGER NOT NULL,
  created_at     INTEGER NOT NULL,
  PRIMARY KEY (experiment_id, root_node_id)
) WITHOUT ROWID;
```

Notes:
- Input-log blobs live **inline** (atomicity with node insert; SQLite outperforms the
  filesystem for blobs this size). `input_log_max_bytes` config rejects oversized logs
  with `INVALID_ARGUMENT`; if the hypervisor ever needs bigger logs, v2 adds an overflow
  file column. Logs are content-addressed ⇒ `PutInputLog` is idempotent (`INSERT OR
  IGNORE`, verify size matches on conflict).
- Multiple nodes may share an `input_log_id` (replayed/duplicated bursts) and *do* share
  `snapshot_ref` only if states are bit-identical — allowed, not special-cased.
- `attrs` is the orchestrator's extension bag; this service never reads inside it.
- **Experiments are implicit:** there is no experiments table — an experiment exists
  iff its root row `(experiment_id, 0)` exists, created by the orchestrator's bootstrap
  `CreateNode`. Page/manifest/log storage is global (content-addressed dedup works
  *across* experiments); only the tree rows carry the experiment dimension.
- `node_id` is a caller-assigned u64 stored as SQLite INTEGER via i64 bit-cast
  (round-trips exactly; never compared by SQL ordering — cursors use the logical
  counters). CreateNode idempotency falls out of the composite primary key: `INSERT`
  conflict ⇒ re-read the row, compare immutable fields, return it or `ALREADY_EXISTS`
  (API.md §1.4).
- `kv_metadata` is the orchestrator's checkpoint/WAL home (API.md §1.5). CAS is one
  `UPDATE … WHERE key=? AND generation=?` inside the writer actor's transaction —
  serialized writes make the generation check race-free by construction.

### 5.4 Canonical queries

```sql
-- get children
SELECT * FROM nodes WHERE experiment_id = ?1 AND parent_node_id = ?2;

-- path to root (replay): returns node rows root→leaf after reversing in code
WITH RECURSIVE path(n) AS (
  SELECT node_id FROM nodes WHERE experiment_id = ?1 AND node_id = ?2
  UNION ALL
  SELECT parent_node_id FROM nodes JOIN path ON nodes.node_id = path.n
  WHERE nodes.experiment_id = ?1 AND parent_node_id IS NOT NULL
)
SELECT nodes.* FROM nodes JOIN path ON nodes.node_id = path.n
WHERE nodes.experiment_id = ?1;

-- subtree collect (prune)
WITH RECURSIVE sub(n) AS (
  SELECT ?2
  UNION ALL
  SELECT node_id FROM nodes JOIN sub ON nodes.parent_node_id = sub.n
  WHERE nodes.experiment_id = ?1
)
SELECT n FROM sub;

-- filtered scan, e.g. frontier above a progress floor, page-able
SELECT * FROM nodes
WHERE experiment_id = ?1 AND status = 0 AND progress_score >= ?2 AND created_at > ?3
ORDER BY created_at LIMIT ?4;

-- bulk attribute update (executed inside one actor txn, per id)
UPDATE nodes SET status=?3, progress_score=?4, novelty_score=?5,
       visit_count=visit_count+?6, last_visited_at=?7, updated_at=?7
WHERE experiment_id=?1 AND node_id=?2;

-- metadata CAS write (writer actor; changes()==0 ⇒ FAILED_PRECONDITION)
UPDATE kv_metadata SET value=?2, generation=generation+1, updated_at=?3
WHERE key=?1 AND generation=?4;

-- tree statistics (per experiment; omit the WHERE for store-wide)
SELECT status, COUNT(*), MAX(progress_score), AVG(progress_score), MAX(depth)
FROM nodes WHERE experiment_id = ?1 GROUP BY status;
```

## 6. Concurrency model

```
                       tokio runtime (snapstore-server)
   tonic TCP :7410 ─┐
   tonic UDS  sock ─┼─► service handlers ──► PageStore (Arc): ingest pool (rayon hash)
   page channel ────┘                        │   └► pack-writer task (1, owns open pack)
                                             ├──► PageIndex (sharded RwLock, read-mostly)
                                             └──► MetaActor (1 blocking thread, SQLite
                                                  writer) + 4 read connections
   background: GC task (≤1), WAL checkpointer, metrics scraper
```

- **Pack writer is single and serial** — append-only files want exactly one appender;
  it batches enqueued pages into `pwritev` calls and issues `fdatasync` per batch or on
  an explicit commit barrier from `PutSnapshot`.
- **Reads are lock-light:** resolve = index probe (shard read lock) + `pread` at
  `PackLoc`. Sealed packs are immutable ⇒ reads need no coordination with the writer.
  Keep an LRU of `File` handles per pack (cap 256). Optional `mmap` of sealed packs
  behind a config flag (off by default; `pread` is predictable and avoids page-cache
  blowups during GC).
- **GC vs reads:** compaction updates the index entry before unlinking; a reader that
  raced and holds an old `PackLoc` may hit ENOENT/short-read → it retries the index probe
  once (the entry now points at the new pack). One retry is provably sufficient because a
  pack is unlinked only after all its live entries are repointed (R2).
- **Meta actor** serializes all SQLite writes; batching gives bulk-update throughput.
  Read RPCs never enter the actor.
- Backpressure: ingest channel bounded (`ingest_queue_pages`, default 65,536 pages =
  256 MiB); fast-path and gRPC ingest block (async) when full.

## 7. Performance engineering

### 7.1 Budget (cross-check MAP.md principle 2)

Principle 2: fork + run 1 guest-second ≪ 1 s wall. Allocate per exploration step
(per child, on one worker): hypervisor restore+run+pause ~600 ms ⇒ **snapshot-store gets
≤ 100 ms of the step**, split:

| Operation | Target p50 | Target p99 |
|---|---|---|
| Commit dirty delta, 2,048 pages (8 MiB) via fast path, incl. fsync + manifest | 8 ms | 25 ms |
| Restore: resolve full working set 16,384 pages (64 MiB — the non-zero half of the 128 MiB guest; zero pages short-circuit) via fast path | 25 ms | 60 ms |
| Restore: delta-only top-up (worker reuses parent's pages), 2,048 pages | 5 ms | 15 ms |
| PutInputLog (sealed segment ≤ 64 KiB typical) + CreateNode | 1.5 ms | 8 ms |
| UpdateNodes batch of 256 | 3 ms | 12 ms |
| GetPath, depth 5,000 | 15 ms | 40 ms |
| QueryNodes page of 1,000 | 4 ms | 15 ms |

Sustained bandwidth targets on the Intel box's NVMe (assume PCIe 4.0 class, ~5 GB/s seq
write, ~7 GB/s read; measure first with `fio`, see IMPLEMENTATION-PLAN.md M0):
- Page ingest ≥ **1.5 GB/s** via page channel (bound by BLAKE3 + one memcpy), ≥ 600 MB/s
  via TCP gRPC.
- Page resolve ≥ **2.5 GB/s** via page channel from page cache / ≥ 1.5 GB/s cold.
- Aggregate across 16 concurrent workers: ≥ 1.2 GB/s ingest with p99 commit < 40 ms.

### 7.2 Dedup expectations

Demo guest = 128 MiB RAM = 32,768 pages. A 1-second burst on a 16-bit-console emulator
dirties roughly 1–8 MiB (emulator heap + framebuffer + audio ring), i.e. **256–2,048
pages ≈ 1–6% of RAM** ⇒ sibling forks share ≥ 94%, typically ≥ 99%. Expected ratios:

- Logical bytes (Σ nodes × 128 MiB) vs physical: ≥ **20×** at 10k nodes, ≥ 50× at 100k
  (dirty pages also repeat across the tree: zero pages, common emulator states).
- Plan capacity: 100k nodes × ~1,000 novel pages avg ≈ 400 GB worst case before GC,
  realistically ≤ 150 GB physical. Alert at 70% NVMe utilization; GC trigger at 80%.

Zero-page special case: the all-zeros page hash is precomputed; ingest short-circuits it
and the page channel never carries it (manifest entries still record it normally).

### 7.3 Hot-path notes

- Hash before dedup-probe, always (the hash *is* the identity); use `blake3::hash` with
  rayon batch over the incoming memfd region — no per-page thread hops.
- `pwritev` with ≥ 64-record batches; pre-touch pack file with `fallocate` (1 GiB) at
  open to avoid extent churn; `fdatasync` not `fsync` for record batches (size already
  allocated), full `fsync` at seal.
- Manifest flattening (delta chain → full page list) is pure in-memory merge over sorted
  entry arrays; memoize flattened manifests in an LRU (`flatten_cache_entries`, default
  1,024 manifests) keyed by SnapshotRef — restore of siblings hits this cache constantly.
- Metrics to export from day one: `snapstore_pages_ingested_total{dedup="hit|miss"}`,
  `snapstore_commit_seconds` histogram, `snapstore_resolve_seconds`,
  `snapstore_pack_bytes{state}`, `snapstore_dedup_ratio`, `snapstore_gc_*`,
  `snapstore_meta_txn_seconds`, `snapstore_nodes{status}`.

## 8. Durability & recovery

- **Commit ordering** (§3) is the contract: pages → manifest → node row, each level
  fsynced before the next is written. `PutSnapshot` returns only after the manifest
  rename + directory fsync complete. `CreateNode` returns only after the SQLite txn
  commits (`synchronous=FULL`).
- **Startup sequence:**
  1. Check `STORE_VERSION`; load `store.uuid`, `config.toml`.
  2. Open `tree.db`; SQLite WAL recovery is automatic; run `PRAGMA integrity_check`
     (fast at our size) — on failure refuse to start, point operator at backups.
  3. Load sidecars → page index; tail-scan the unsealed pack, truncate torn tail (§2.2).
  4. Clean `tmp/`; remove `.spm` files with bad footers (incomplete writes); finish or
     roll back `*.dead` pack markers.
  5. Reconcile: every `snapshot_ref` and pinned ref must resolve to a manifest, and every
     manifest entry to an indexed page. Missing manifest/page ⇒ mark node `PRUNED`,
     log loudly, increment `snapstore_integrity_errors_total` (should never happen given
     ordering; treat as a P0 bug signal per MAP.md).
- **Integrity scan option** (`snapstorectl fsck [--deep]`): shallow = step 5 above plus
  sidecar footers; deep = re-read every pack record, recompute crc32c and BLAKE3,
  verify hash matches record header. Deep scan is also runnable on a schedule
  (`fsck_interval`) at low io-priority (`ioprio_set` idle class).
- **Backups (cold, scheduled — see IMPLEMENTATION-PLAN.md M9):** this store is the
  **only durable copy** of a campaign's tree, snapshots, and input logs; during a
  multi-week Phase 8 run a dead NVMe must not mean a restarted search. The format is
  built for cheap consistent copies: packs are append-only and sealed packs/manifests
  are immutable, so `snapstorectl backup --to <dest>` takes a crash-consistency point
  (`gc_commit_gate` write-lock instant, same fence GC uses), then ships (1) all sealed
  packs + sidecars not yet at `<dest>` (incremental by pack id), (2) the loose-manifest
  dir (rsync-style, files are write-once), (3) a `VACUUM INTO` snapshot of `tree.db`
  (nodes, input logs, **kv_metadata** — the orchestrator's checkpoints ride along), and
  (4) the unsealed pack's valid tail. `<dest>` is the DGX Spark over the lab network or
  any external mount. Restore = lay the files down, start the server, let startup
  recovery (§8) replay its normal path; anything committed after the consistency point
  is lost, which is exactly the semantics of resuming from the orchestrator's last
  checkpoint in the backed-up `kv_metadata`. The restore drill is a normative M9
  acceptance criterion, not an aspiration.

## 9. Configuration (`config.toml`, defaults)

```toml
data_root = "/var/lib/snapstore"
grpc_tcp_addr = "0.0.0.0:7410"
grpc_uds_path = "/run/snapstore/grpc.sock"
page_channel_path = "/run/snapstore/pages.sock"
http_addr = "0.0.0.0:7411"             # /healthz, /metrics

[pagestore]
pack_max_bytes = 1073741824            # 1 GiB
ingest_queue_pages = 65536
ingest_hash_threads = 8
flatten_cache_entries = 1024
mmap_sealed_packs = false

[meta]
input_log_max_bytes = 4194304          # 4 MiB — THE platform per-segment cap (API.md §3)
metadata_value_max_bytes = 16777216    # 16 MiB — KV value cap (API.md §1.5)
read_connections = 4
write_batch_max = 256

[backup]                               # cold backup, M9 (ARCHITECTURE.md §8)
auto = false                           # operator/control-plane scheduled
dest = ""                              # e.g. "spark:/backups/snapstore" or a mount path

[gc]
auto = true
trigger_disk_pct = 80
compact_threshold = 0.5
tombstone_grace_cycles = 1
```
