# snapshot-store — API Reference

Three surfaces:
1. **gRPC** (tonic) — full functionality. TCP `:7410` for off-box callers, and the same
   service bound on UDS `/run/snapstore/grpc.sock` for on-box callers.
2. **Page channel** — local fast path for bulk page bytes (SEQPACKET UDS + memfd
   fd-passing). Control still goes over gRPC; only page payloads use this channel.
3. **On-disk / on-wire byte formats** — manifest container (`.spm`) and input-log
   container, specified byte-precisely (§2, §3).

All multi-byte integers in binary formats are **little-endian**. All hashes are
**BLAKE3-256** (32 bytes). "Canonical bytes" of a container = every byte except the
trailing 32-byte footer; the container's content hash (snapshot ref / log id) is BLAKE3
of the canonical bytes, and the footer **equals** that hash (self-verifying files).

---

## 1. gRPC service (`proto/snapshot_store.proto`)

```proto
syntax = "proto3";
package determinism.snapstore.v1;

service SnapshotStore {
  // ---- pages & snapshots ----
  rpc PutPages(stream PutPagesRequest) returns (PutPagesResponse);
  rpc PutSnapshot(PutSnapshotRequest) returns (PutSnapshotResponse);
  rpc GetSnapshot(GetSnapshotRequest) returns (GetSnapshotResponse);
  rpc ResolvePages(ResolvePagesRequest) returns (stream ResolvePagesResponse);
  rpc HasPages(HasPagesRequest) returns (HasPagesResponse);

  // ---- input logs ----
  rpc PutInputLog(PutInputLogRequest) returns (PutInputLogResponse);
  rpc GetInputLog(GetInputLogRequest) returns (GetInputLogResponse);

  // ---- lineage tree (all experiment-scoped) ----
  rpc CreateNode(CreateNodeRequest) returns (CreateNodeResponse);
  rpc UpdateNodes(UpdateNodesRequest) returns (UpdateNodesResponse);
  rpc GetNode(GetNodeRequest) returns (GetNodeResponse);
  rpc GetChildren(GetChildrenRequest) returns (GetChildrenResponse);
  rpc GetPath(GetPathRequest) returns (GetPathResponse);
  rpc QueryNodes(QueryNodesRequest) returns (stream QueryNodesResponse);

  // ---- metadata KV (CAS) ----
  rpc PutMetadata(PutMetadataRequest) returns (PutMetadataResponse);
  rpc GetMetadata(GetMetadataRequest) returns (GetMetadataResponse);
  rpc DeleteMetadata(DeleteMetadataRequest) returns (DeleteMetadataResponse);

  // ---- lifecycle ----
  rpc PruneSubtree(PruneSubtreeRequest) returns (PruneSubtreeResponse);
  rpc Pin(PinRequest) returns (PinResponse);
  rpc Unpin(UnpinRequest) returns (UnpinResponse);
  rpc TriggerGc(TriggerGcRequest) returns (TriggerGcResponse);
  rpc Stats(StatsRequest) returns (StatsResponse);
}
```

This list is exhaustive, and three RPCs that clients have historically wished for are
**deliberately absent**:

- **No `ReleaseSnapshot` / refcounting.** Discarded or never-committed children are
  unreachable orphans; mark-and-sweep GC reclaims them (ARCHITECTURE.md §4.1). There is
  nothing for a caller to release.
- **No `ResolveArtifact`.** The store has no artifact concept; experiment bootstrap
  (image → root snapshot → root node) is owned by `exploration-orchestrator`.
- **No `ListNodes`.** `QueryNodes` is the one scan primitive — filtered, ordered,
  cursorable via the logical counters.

### 1.1 Common messages

```proto
// 32-byte hashes travel as `bytes`; servers MUST validate length.
// The tree is EXPERIMENT-SCOPED: one store holds many independent experiment trees.
// `experiment_id` is a caller-chosen UTF-8 string (1..=128 bytes); `node_id` is a
// CALLER-ASSIGNED uint64, unique within its experiment. node_id 0 is the experiment
// root, always and only.
message NodeMeta {
  string experiment_id   = 1;
  uint64 node_id         = 2;  // caller-assigned; unique per experiment; root = 0
  optional uint64 parent_node_id = 3; // unset only for the root (node_id 0)
  uint64 depth           = 4;
  bytes  snapshot_ref    = 5;  // 32 bytes
  bytes  input_log_id    = 6;  // 32 bytes; empty for the root node
  NodeStatus status      = 7;
  double progress_score  = 8;
  double novelty_score   = 9;
  uint64 visit_count     = 10;
  uint64 expand_count    = 11;
  uint64 last_visited_at = 12; // logical counter; 0 = never
  uint64 created_at      = 13; // logical counter
  uint64 updated_at      = 14; // logical counter
  bytes  attrs           = 15; // opaque postcard blob, orchestrator-private
}

enum NodeStatus {
  NODE_STATUS_FRONTIER = 0;
  NODE_STATUS_EXPANDED = 1;
  NODE_STATUS_PRUNED   = 2;
  NODE_STATUS_GOAL     = 3;
}
```

### 1.2 Pages & snapshots

```proto
message PutPagesRequest {
  // Each message carries a batch; 4096 % — every page entry is exactly 4096 bytes.
  // Server hashes each page itself (the hash is the identity; clients cannot lie).
  repeated bytes pages = 1;     // each exactly 4096 bytes; ≤ 256 per message (1 MiB)
}
message PutPagesResponse {
  uint64 pages_received  = 1;
  uint64 pages_new       = 2;   // novel (stored)
  uint64 pages_deduped   = 3;   // already present
  repeated bytes hashes  = 4;   // 32 B each, in arrival order across the whole stream
}

message PutSnapshotRequest {
  bytes manifest = 1;           // full .spm container bytes (§2), footer included
}
message PutSnapshotResponse {
  bytes snapshot_ref = 1;       // 32 B; equals BLAKE3(canonical bytes) == footer
}
// Errors: INVALID_ARGUMENT (malformed/bad footer/version),
//         FAILED_PRECONDITION (missing pages or unknown parent manifest;
//           error detail = MissingPages{repeated bytes page_hashes; bytes parent_ref})

message GetSnapshotRequest {
  bytes snapshot_ref = 1;
}
message GetSnapshotResponse {
  bytes manifest = 1;           // the stored .spm container, byte-identical
}

message ResolvePagesRequest {
  bytes snapshot_ref = 1;
  // Mode A (flatten): resolve the full materialized page set of the snapshot.
  // Mode B (delta):   only entries this snapshot's chain adds/changes relative to
  //                   `baseline_ref` (must be an ancestor manifest; workers that still
  //                   hold the parent's RAM use this for cheap restore top-ups).
  bytes baseline_ref = 2;       // empty = Mode A
  bool  hashes_only  = 3;       // true ⇒ omit payloads (planning / page-channel mode)
}
message ResolvePagesResponse {
  // Stream of batches, ascending by page_index.
  repeated PageEntry entries = 1;     // ≤ 256 per message
  message PageEntry {
    uint64 page_index = 1;            // guest page number (gpa / 4096)
    bytes  page_hash  = 2;            // 32 B
    bytes  payload    = 3;            // 4096 B, or empty when hashes_only
  }
}

message HasPagesRequest  { repeated bytes page_hashes = 1; }   // ≤ 4096 per call
message HasPagesResponse { repeated bool present = 1; }         // parallel array
```

### 1.3 Input logs

```proto
message PutInputLogRequest {
  bytes container = 1;          // full input-log container (§3), footer included
}
message PutInputLogResponse {
  bytes log_id = 1;             // 32 B
}
// Idempotent: re-putting an identical container returns the same log_id.

message GetInputLogRequest  { bytes log_id = 1; }
message GetInputLogResponse { bytes container = 1; }  // byte-identical to what was put
```

### 1.4 Tree CRUD & queries

```proto
message CreateNodeRequest {
  string experiment_id  = 1;    // required
  uint64 node_id        = 2;    // caller-assigned; 0 ⇒ creating the experiment root
  optional uint64 parent_node_id = 3; // unset only when node_id == 0 (the root)
  bytes  snapshot_ref   = 4;    // must resolve to a stored manifest
  bytes  input_log_id   = 5;    // must exist; empty allowed only for the root
  NodeStatus status     = 6;    // normally FRONTIER
  double progress_score = 7;
  double novelty_score  = 8;
  bytes  attrs          = 9;
  // Optional inline log: if set, stored atomically with the node and used as
  // input_log_id (which must then be empty). Single-txn alternative for callers
  // that hold the log bytes; the v1 commit path is worker-side PutInputLog at
  // TakeSnapshot instead (INTEGRATION.md §2.1) — no v1 consumer sets this.
  bytes  input_log_container = 10;
}
message CreateNodeResponse { NodeMeta node = 1; }
// IDEMPOTENT on (experiment_id, node_id): if the node already exists with identical
// immutable fields (parent_node_id, snapshot_ref, input_log_id), the stored NodeMeta
// is returned with success — callers MAY blind-retry a timed-out CreateNode. If the
// key exists with DIFFERENT immutable fields ⇒ ALREADY_EXISTS (caller bug: node-id
// reuse). Other errors: NOT_FOUND (parent/snapshot/log), FAILED_PRECONDITION (parent
// PRUNED; root already exists when node_id == 0; node_id == 0 with parent set;
// node_id != 0 with parent unset), INVALID_ARGUMENT.

message UpdateNodesRequest {
  // Bulk partial updates within ONE experiment, applied in ONE transaction
  // (all-or-nothing).
  string experiment_id        = 1;
  repeated NodeUpdate updates = 2;          // ≤ 4096
  message NodeUpdate {
    uint64 node_id = 1;
    optional NodeStatus status     = 2;
    optional double progress_score = 3;
    optional double novelty_score  = 4;
    uint64 visit_count_delta       = 5;     // added, not assigned
    uint64 expand_count_delta      = 6;
    bool   touch_visited           = 7;     // set last_visited_at = txn logical counter
    optional bytes attrs           = 8;     // full replace
  }
}
message UpdateNodesResponse {
  uint64 updated_at = 1;        // logical counter assigned to this transaction
  uint32 applied    = 2;
}
// NOT_FOUND if ANY id is unknown (txn rolls back; error detail lists missing ids).

message GetNodeRequest  { string experiment_id = 1; uint64 node_id = 2; }
message GetNodeResponse { NodeMeta node = 1; }

message GetChildrenRequest  { string experiment_id = 1; uint64 node_id = 2; }
message GetChildrenResponse { repeated NodeMeta children = 1; }

message GetPathRequest {
  string experiment_id = 1;
  uint64 node_id = 2;
  bool include_input_logs = 3;  // true ⇒ replay-renderer one-shot (logs inline)
}
message GetPathResponse {
  repeated NodeMeta nodes = 1;          // ordered ROOT FIRST … leaf last
  repeated bytes input_log_containers = 2; // parallel to nodes[1..] when requested;
                                           // nodes[0] (root) has no log
}

message QueryNodesRequest {
  // Conjunctive filter; unset fields don't constrain. Server streams pages of results.
  // QueryNodes is the ONLY scan primitive (there is no ListNodes).
  string experiment_id          = 1;   // required
  repeated NodeStatus statuses  = 2;
  optional double min_progress  = 3;
  optional double max_progress  = 4;
  optional double min_novelty   = 5;
  optional uint64 min_depth     = 6;
  optional uint64 max_depth     = 7;
  optional uint64 created_after = 8;   // logical counter, exclusive — incremental sync
  optional uint64 updated_after = 9;   // exclusive
  OrderBy order_by              = 10;
  uint32 limit                  = 11;  // 0 = unlimited (stream until done)
  enum OrderBy {
    ORDER_BY_CREATED_AT   = 0;   // ascending — stable for cursor-style sync
    ORDER_BY_PROGRESS_DESC = 1;
    ORDER_BY_NOVELTY_DESC  = 2;
  }
}
message QueryNodesResponse { repeated NodeMeta nodes = 1; }   // ≤ 512 per message
```

### 1.5 Metadata KV

A small, durable key→value map with compare-and-swap, stored in the same SQLite DB as
the tree (one fsync discipline, one backup story). **Purpose:** the
`exploration-orchestrator`'s checkpoints and write-ahead log — keys are namespaced
(`orch/ckpt/<experiment_id>`, `orch/wal/<experiment_id>/<seq>`) — so the orchestrator
host can be wiped and the experiment resumed elsewhere. The `expected_generation` CAS
is the **single-writer enforcement** primitive: a stale orchestrator instance loses the
CAS race and must stand down. The store never parses values.

```proto
message PutMetadataRequest {
  string key   = 1;             // UTF-8, 1..=512 bytes; "/"-namespaced by convention
  bytes  value = 2;             // ≤ 16 MiB (metadata_value_max_bytes)
  // CAS: unset ⇒ unconditional write (last-writer-wins).
  //      0     ⇒ key must not exist (create-only).
  //      N>0   ⇒ current generation must equal N.
  optional uint64 expected_generation = 3;
}
message PutMetadataResponse {
  uint64 generation = 1;        // new generation (1 on first create, then +1 per write)
}
// FAILED_PRECONDITION on CAS mismatch (detail CurrentGeneration{uint64 generation};
// generation 0 in the detail means the key does not exist). INVALID_ARGUMENT on
// key/value size violations.

message GetMetadataRequest  { string key = 1; }
message GetMetadataResponse { bytes value = 1; uint64 generation = 2; }
// NOT_FOUND if the key does not exist.

message DeleteMetadataRequest {
  string key = 1;
  optional uint64 expected_generation = 2;  // unset ⇒ unconditional
}
message DeleteMetadataResponse {}
// NOT_FOUND if absent; FAILED_PRECONDITION on CAS mismatch. After a delete the key's
// generation history restarts at 1 on the next create.
```

### 1.6 Lifecycle

```proto
message PruneSubtreeRequest {
  string experiment_id = 1;
  uint64 node_id  = 2;
  bool allow_root = 3;          // safety interlock for pruning an experiment's root (node 0)
}
message PruneSubtreeResponse {
  uint64 nodes_pruned = 1;      // rows transitioned to PRUNED + tombstoned
}

message PinRequest    { bytes snapshot_ref = 1; string reason = 2; }
message PinResponse   {}
message UnpinRequest  { bytes snapshot_ref = 1; }
message UnpinResponse {}

message TriggerGcRequest  { bool compact_aggressively = 1; } // threshold 0.9 vs 0.5
message TriggerGcResponse { bool started = 1; }              // false if a cycle is running

message StatsRequest {
  optional string experiment_id = 1;  // set ⇒ tree section scoped to one experiment;
                                      // unset ⇒ aggregated across all experiments
}
message StatsResponse {
  // tree
  uint64 nodes_total = 1;
  map<string, uint64> nodes_by_status = 2;   // "frontier","expanded","pruned","goal"
  uint64 max_depth = 3;
  double best_progress_score = 4;
  uint64 logical_counter = 5;
  uint64 experiments_total = 18;             // distinct experiment_ids with a live root
  // store
  uint64 unique_pages = 6;
  uint64 physical_page_bytes = 7;
  uint64 logical_page_bytes = 8;             // Σ over live manifests of flattened size
  double dedup_ratio = 9;                    // logical / physical
  uint64 manifests_total = 10;
  uint64 packs_total = 11;
  uint64 disk_used_bytes = 12;
  uint64 disk_free_bytes = 13;
  // gc
  uint64 gc_cycles_total = 14;
  uint64 gc_last_freed_bytes = 15;
  uint64 gc_last_finished_at = 16;           // logical counter; 0 = never
  uint64 tombstoned_subtrees = 17;
}
```

### 1.7 Error model

Standard gRPC codes only: `INVALID_ARGUMENT` (malformed bytes, wrong lengths, version
mismatch), `NOT_FOUND`, `ALREADY_EXISTS` (CreateNode key reuse with different content),
`FAILED_PRECONDITION` (missing pages/parents, pruned parent, root conflicts, metadata
CAS mismatch), `RESOURCE_EXHAUSTED` (disk watermark exceeded — commits refused above
95% NVMe utilization), `UNAVAILABLE` (starting up / fsck running). Structured detail
messages (`MissingPages`, `MissingNodes`, `CurrentGeneration`) ride in
`google.rpc.Status.details`.

---

## 2. Manifest container format (`.spm`, version 1)

The manifest is the **snapshot reference's preimage**. It is built by the client
(hypervisor worker, via `snapstore-client`) or by the server from parts — either way the
byte layout is identical and canonical: there is exactly one valid encoding for a given
snapshot (sorted, deduplicated entries; fixed field order), so identical states always
produce identical refs.

```
Header (96 bytes):
  off  size  field
  0    8     magic              = b"SPSMAN01"
  8    2     version u16        = 1
  10   2     flags u16          bit0 DELTA      (1 = delta manifest, parent set)
                                bit1 DEV_ZSTD   (1 = device blob zstd-compressed)
                                bits 2–15 reserved, MUST be 0
  12   4     header_len u32     = 96  (offset where entries begin)
  16   32    parent_manifest_hash   (all-zero iff DELTA flag clear)
  48   8     guest_ram_bytes u64    (e.g. 134217728; MUST be multiple of 4096)
  56   8     page_size u64          = 4096 (future-proofing; v1 readers reject ≠4096)
  64   8     entry_count u64
  72   8     device_blob_len u64    (compressed length if DEV_ZSTD)
  80   8     device_blob_raw_len u64 (uncompressed length; == blob_len if not DEV_ZSTD)
  88   4     device_blob_format u32 (opaque tag minted by determinism-hypervisor;
                                     identifies its device-state schema version)
  92   4     reserved u32 = 0

Page entry table (entry_count × 40 bytes, at offset 96):
  0    8     page_index u64     (guest page number)
  8    32    page_hash          (BLAKE3-256 of the 4096-byte page content)
  -- entries MUST be sorted ascending by page_index, strictly unique.
  -- FULL manifest: entry_count == guest_ram_bytes/4096; page_index runs 0..N-1
  --   contiguously (validated). The root snapshot is always FULL.
  -- DELTA manifest: entries are exactly the pages whose content differs from the
  --   flattened parent. Pages absent here are inherited from the parent chain.

Device blob (device_blob_len bytes, at offset 96 + entry_count*40):
  opaque bytes from the hypervisor (vCPU regs, device state, virtual-time state).

Footer (32 bytes, at end):
  BLAKE3-256 over bytes [0 .. filesize-32).  snapshot_ref == footer.
```

Validation on `PutSnapshot` (all failures → `INVALID_ARGUMENT` unless noted):
magic/version/flags; `header_len==96`; sort/uniqueness/contiguity rules; footer matches
recomputed hash; `parent_manifest_hash` resolves to a stored manifest with identical
`guest_ram_bytes` (`FAILED_PRECONDITION` if missing); every `page_hash` present and
durable (`FAILED_PRECONDITION` + `MissingPages`); DEV_ZSTD blob decompresses to
`device_blob_raw_len` (decompression result is discarded — storage keeps the container
byte-identical).

**Flattening** (server-side, for ResolvePages Mode A): walk chain root-ward collecting
entry tables; child entries shadow parent entries with equal `page_index`; result must
cover every index `0..guest_ram_bytes/4096` (a gap = corruption, P0). Chain depth:
the server accepts any depth and never rewrites a manifest (implicit conversion to FULL
would change the content-derived ref). Instead, the **hypervisor worker** is responsible
for emitting a FULL manifest every `max_delta_chain` (default 64) generations
(INTEGRATION.md §2.1), and the flatten cache (ARCHITECTURE.md §7.3) keeps deep chains
cheap in practice. The server exports a `snapstore_flatten_depth` histogram so cadence
violations are visible.

### Rust sketch (`snapstore-manifest`)

```rust
pub struct Manifest {
    pub version: u16,
    pub delta: bool,
    pub parent: Option<SnapshotRef>,
    pub guest_ram_bytes: u64,
    pub entries: Vec<ManifestEntry>,      // sorted, unique by page_index
    pub device_blob: DeviceBlob,          // { format: u32, zstd: bool, bytes: Vec<u8>, raw_len: u64 }
}
pub struct ManifestEntry { pub page_index: u64, pub page_hash: PageHash }

impl Manifest {
    pub fn encode(&self) -> Vec<u8>;                       // canonical bytes + footer
    pub fn decode(buf: &[u8]) -> Result<Self, ManifestError>; // full validation
    pub fn snapshot_ref(buf: &[u8]) -> SnapshotRef;        // blake3(buf[..len-32])
}
pub fn flatten(chain: &[&Manifest]) -> Result<Vec<ManifestEntry>, FlattenError>; // child-first
```

---

## 3. Input-log container format (version 1)

The **inner** payload is the canonical input-log schema owned by
`determinism-hypervisor` (MAP.md: the platform's most stability-critical format).
snapshot-store wraps it in a thin hashed container and never parses the payload.

```
  off  size  field
  0    4     magic = b"SILG"
  4    2     container_version u16 = 1
  6    2     flags u16 = 0 (reserved)
  8    4     inner_format_version u32  (the hypervisor's log schema version; surfaced
                                        in DB column input_logs.inner_version)
  12   4     reserved u32 = 0
  16   8     payload_len u64
  24   N     payload (opaque hypervisor log bytes)
  24+N 32    footer: BLAKE3-256 over bytes [0 .. 24+N).  log_id == footer.
```

`PutInputLog` validates magic/version/lengths/footer and enforces
`24 + payload_len + 32 ≤ input_log_max_bytes` (4 MiB default). Idempotent by content.
`GetInputLog`/`GetPath(include_input_logs)` return the container byte-identical;
callers re-verify the footer (the `snapstore-client` lib does this automatically).

The 4 MiB figure is **the platform's per-segment input-log cap** — defined here, cited
by `determinism-hypervisor`'s DHILOG section (anything it can produce or verify inline
fits this cap). Spliced `.dilog` containers (replay-renderer's multi-segment replay
artifacts) are **exempt**: they live in `control-plane`'s artifact registry and never
enter this store.

---

## 4. Local fast path

Co-located hypervisor workers move tens of MiB per step; TCP gRPC double-copies and
frames every page. The fast path keeps **control on gRPC-over-UDS** (cheap, typed) and
moves **page payloads over a dedicated page channel**: `SOCK_SEQPACKET` Unix socket with
`memfd` file descriptors passed via `SCM_RIGHTS`. One copy into the memfd on the sender,
zero further copies until the pack write / guest RAM write.

Socket: `/run/snapstore/pages.sock`, mode 0660, group `snapstore`. One connection per
worker (the server handles each connection on its own task). Every message =
one SEQPACKET datagram containing a fixed header (+ inline array) with 0 or 1 fds
attached. Max datagram 64 KiB ⇒ header batches are capped; the *page data* always rides
in the memfd, never inline.

```rust
// All structs repr(C), little-endian, packed as written.
#[repr(C)]
struct PcHdr {
    magic:   u32,  // 0x50434831 "PCH1"
    msg:     u16,  // message type, below
    flags:   u16,  // 0
    seq:     u64,  // sender-chosen; echoed in the reply
    count:   u32,  // number of array elements following the header
    reserved:u32,
}
// msg values:
//   1 = PUT_BATCH      (client→server, fd = memfd with count*4096 bytes)
//   2 = PUT_BATCH_OK   (server→client, no fd)
//   3 = GET_BATCH      (client→server, header followed by count*40-byte GetReq, no fd)
//   4 = GET_BATCH_DATA (server→client, fd = memfd with count*4096 bytes)
//   5 = ERROR          (server→client, followed by ErrBody)

// PUT_BATCH: fd's memfd holds `count` pages back-to-back (count ≤ 8192 ⇒ ≤ 32 MiB).
//   Server: mmap fd readonly, batch-hash, dedup, enqueue novel pages (the pack writer
//   copies out of the mapping; the memfd is closed after the batch is queued+indexed).
// PUT_BATCH_OK body: count × 32-byte page hashes, in page order — BUT since 8192×32 =
//   256 KiB exceeds the datagram cap, PUT_BATCH_OK carries only:
#[repr(C)]
struct PutOkBody { pages_new: u32, pages_deduped: u32, batch_blake3: [u8; 32] }
//   batch_blake3 = BLAKE3 over the concatenated per-page hashes (order = memfd order);
//   the client computes per-page hashes itself anyway (it builds the manifest) and uses
//   batch_blake3 to cross-check agreement with the server. Mismatch = P0 determinism bug.

// GET_BATCH array element:
#[repr(C)]
struct GetReq { page_hash: [u8; 32], dst_slot: u64 }   // count ≤ 1500 per datagram
//   (64 KiB datagram cap ⇒ ≤ 1637 elements; use 1500.) Client sends multiple GET_BATCH
//   datagrams for larger sets; seq orders them.
// GET_BATCH_DATA: server creates memfd sized count*4096; page for request[i] is written
//   at offset i*4096 (dst_slot is echoed metadata for the client's scatter logic, the
//   server does not interpret it). Client mmaps and scatters into guest RAM.

#[repr(C)]
struct ErrBody { code: u32 /* 1 NOT_FOUND, 2 INVALID, 3 OVERLOAD */, detail_len: u32 /* utf8 follows */ }
```

Rules:
- Hashing authority is the **server** for storage (it never trusts client hashes), and
  the cross-check hash makes silent disagreement loud.
- The page channel is optional: every operation has a pure-gRPC equivalent
  (`PutPages`/`ResolvePages` with payloads). `snapstore-client` auto-selects: page
  channel if the socket exists and connects, else UDS gRPC, else TCP.
- Typical worker commit = 1 `PUT_BATCH` (dirty pages) + gRPC `PutInputLog` (the sealed
  segment DHILOG) + gRPC `PutSnapshot`; `CreateNode` is the orchestrator's call — see
  INTEGRATION.md §2.
- Typical worker restore = gRPC `ResolvePages(hashes_only=true)` (or cached manifest) →
  `GET_BATCH`s for pages it doesn't already hold.
- Why not shared-memory rings: fd-passing of memfds gets within a copy of ring
  performance with none of the lifetime/cleanup hazards (a dead worker's memfds are
  reclaimed by the kernel automatically). Decision recorded; rings revisit only if
  profiling shows the syscall path dominating (IMPLEMENTATION-PLAN.md risk R6).

---

## 5. Versioning & evolution

- Proto package is `determinism.snapstore.v1`; breaking changes mint `v2` side-by-side.
  The proto file is destined for the shared schema set in `control-plane` (MAP.md);
  until then this repo's copy is canonical and the orchestrator/hypervisor vendor it.
- Binary containers: bump the header `version` field; readers reject unknown versions
  (never best-effort parse). `STORE_VERSION` gates the whole data root.
- `device_blob_format` and `inner_format_version` are pass-through hypervisor versions —
  snapshot-store stores and surfaces them but never branches on them.
