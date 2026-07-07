# determinism-hypervisor — API

Three contracts live here, in order of stability-criticality:

1. **§3 The input log (`DHILOG`)** — the platform's most stability-critical schema.
2. **§4 The device blob (`DHSNAP`)** and **§5 snapshot manifest interchange** with
   snapshot-store.
3. **§2 The gRPC surface** (`hypervisor.proto`, tonic/prost).

Conventions: all binary formats are **little-endian**, fixed-layout (no varints), with
explicit `version` fields. All hashes are **BLAKE3-256 (32 bytes)** unless stated.
`icount` values in all formats are **relative to the segment's base snapshot**.

---

## 1. Versioning & compatibility rules

- `DHILOG` and `DHSNAP` carry `u16` format versions. Readers MUST reject a major
  version they don't know (`version` high byte) and MUST skip unknown **AUX** record
  kinds; unknown **canonical** record kinds are a hard error (they would change
  execution).
- `hypervisor.proto` is `package determinism.hypervisor.v1;` — breaking changes mean a
  new package version. The proto file is the temporary source of truth at
  `proto/hypervisor.proto` until `control-plane` exists (MAP.md convention).
- Golden-bytes tests pin every format: a checked-in fixture file per version must parse
  to a checked-in debug representation, and re-serialize byte-identically.

---

## 2. gRPC surface (`proto/hypervisor.proto`)

```proto
syntax = "proto3";
package determinism.hypervisor.v1;

// Served by dh-workerd on 0.0.0.0:7400 (TCP) and /run/dh/grpc.sock (UDS).
service HypervisorWorker {
  // ---- slot lifecycle ----
  rpc CreateVm        (CreateVmRequest)        returns (CreateVmResponse);
  rpc RestoreSnapshot (RestoreSnapshotRequest) returns (RestoreSnapshotResponse);
  rpc Fork            (ForkRequest)            returns (ForkResponse);
  rpc DestroyVm       (DestroyVmRequest)       returns (DestroyVmResponse);

  // ---- execution ----
  rpc InjectInputs    (InjectInputsRequest)    returns (InjectInputsResponse);
  rpc Run             (RunRequest)             returns (RunResponse);
  rpc Pause           (PauseRequest)           returns (PauseResponse);   // see §2.9
  rpc TakeSnapshot    (TakeSnapshotRequest)    returns (TakeSnapshotResponse);
  // Phase 8, optional (guest-sdk Ms6); not part of the v1 exploration loop. §2.10.
  rpc Quiesce         (QuiesceRequest)         returns (QuiesceResponse);

  // ---- introspection (slot must be Paused) ----
  rpc ReadGuestMemory (ReadGuestMemoryRequest) returns (ReadGuestMemoryResponse);
  rpc GetFramebuffer  (GetFramebufferRequest)  returns (GetFramebufferResponse);
  rpc StreamGuestEvents (StreamGuestEventsRequest) returns (stream GuestEvent);

  // ---- verification & rendering ----
  rpc VerifyReplay    (VerifyReplayRequest)    returns (stream VerifyReplayProgress);
  // Required by replay-renderer (Phase 7); not part of the v1 exploration loop. §2.7.
  rpc RunWithFrameCapture (RunWithFrameCaptureRequest) returns (stream FrameCaptureEvent);

  // ---- worker / health ----
  rpc GetWorkerInfo   (GetWorkerInfoRequest)   returns (GetWorkerInfoResponse);
  rpc ListSlots       (ListSlotsRequest)       returns (ListSlotsResponse);
  rpc WatchSlots      (WatchSlotsRequest)      returns (stream SlotEvent);
}
```

### 2.1 Common types

```proto
message SnapshotRef   { bytes hash = 1; }                  // 32 bytes, BLAKE3 of manifest
message StateHash     { bytes hash = 1; }                  // 32 bytes, chain value (ARCHITECTURE §8.5)
message Lease         { uint64 slot_id = 1; bytes token = 2; }  // token: 16 random bytes

message MachineConfig {
  uint32 version          = 1;   // config schema version, currently 1
  uint64 mem_bytes        = 2;   // multiple of 2 MiB
  uint32 vcpus            = 3;   // MUST be 1 in v1
  uint32 clock_num        = 4;   // virtual ns per instruction, rational (default 1)
  uint32 clock_den        = 5;   // (default 1)
  bytes  base_image_hash  = 6;   // BLAKE3 of the base disk image (must be in image cache)
  BootSpec boot           = 7;
  uint64 epoch_len        = 8;   // instructions per epoch (default 50_000_000)
  HashEpochs hash_epochs  = 9;   // EPOCHS_ON (default) | FINAL_ONLY
  uint32 skid_margin      = 10;  // default 8192 (landing only; does not affect results)
  repeated CpuidLeaf cpuid_table = 11;  // sorted by (function,index), unique; canonical MCFG preimage
  repeated uint32 device_set     = 12;  // u16 device ids, sorted unique; canonical MCFG preimage
}
message CpuidLeaf {
  uint32 function = 1;
  uint32 index    = 2;
  uint32 flags    = 3;
  uint32 eax      = 4;
  uint32 ebx      = 5;
  uint32 ecx      = 6;
  uint32 edx      = 7;
}
message BootSpec {
  oneof kind {
    ElfBoot     elf     = 1;     // unikernel / freestanding ELF
    BzImageBoot bzimage = 2;     // minimal Linux
  }
}
message ElfBoot     { bytes kernel_hash = 1; bytes cmdline = 2; } // kernel_hash is a 32-byte BLAKE3 cache key
message BzImageBoot { bytes kernel_hash = 1; bytes initramfs_hash = 2; // both are 32-byte BLAKE3 cache keys
                      bytes cmdline = 3; }  // APPEND-ONLY extras; the worker forces the
                                            //   canonical deterministic baseline and accepts
                                            //   only whitelisted extra flags (ARCH §2.3)
enum HashEpochs { HASH_EPOCHS_UNSPECIFIED = 0; EPOCHS_ON = 1; FINAL_ONLY = 2; }

// ---- capture engine (ARCHITECTURE §6.10) ----
// This service owns the per-step feature/framebuffer capture path (MAP.md dataflow
// step 4) and is explicitly FEATURE-MAP-AGNOSTIC: it never parses feature maps. The
// orchestrator compiles the experiment's feature map (reference-workload schema) into
// this flat extraction list once at experiment start; ranges address regions in the
// guest-sdk region manifest by name (resolved at CHANNEL_INIT and re-resolved after
// every restore — ARCHITECTURE §6.6).
message CaptureSpec {
  repeated ExtractRange ranges = 1;  // packed into feature_bytes in request order
  bool framebuffer = 2;              // also return the lz4-compressed framebuffer
}
message ExtractRange {
  string region         = 1;  // guest-sdk region-manifest name
  uint32 layout_version = 2;  // must match the manifest entry, else FAILED_PRECONDITION
  uint64 offset         = 3;  // bytes into the (logically concatenated) region
  uint32 len            = 4;
}
message FbInfo {
  uint32 width = 1; uint32 height = 2; uint32 stride = 3;
  PixelFormat format = 4;
  uint32 frame_counter = 5;          // pv-pad FRAME_COUNTER at capture
}
```

### 2.2 Slot lifecycle

```proto
message CreateVmRequest {            // cold boot from base image (rare: root creation)
  MachineConfig config   = 1;
  bytes entropy_seed     = 2;        // 32 bytes; DHILOG header seed for the boot segment
}
message CreateVmResponse {
  Lease lease            = 1;
  uint64 icount          = 2;        // 0
}

message RestoreSnapshotRequest {
  SnapshotRef snapshot   = 1;        // resolved via snapshot-store by the worker
  bytes entropy_seed     = 2;        // 32 bytes; empty ⇒ continue snapshot's PRNG stream
}
message RestoreSnapshotResponse {
  Lease lease            = 1;
  MachineConfig config   = 2;        // recovered from the manifest's machine config
  StateHash state_hash   = 3;        // chain value at the restored boundary
  uint32 frame_counter   = 4;        // ABSOLUTE pv-pad FRAME_COUNTER at the restored
                                     //   boundary (DHSNAP PADD) — the base callers use
                                     //   to schedule at_frame = frame_counter + offset (§2.3)
}

message ForkRequest {
  Lease parent           = 1;        // parent must be Paused; becomes Frozen
  uint32 count           = 2;        // 1..=remaining free slots
  repeated bytes entropy_seeds = 3;  // empty ⇒ every child continues the fork-point PRNG;
                                     //   otherwise count × 32 bytes. An all-zero child seed
                                     //   continues; a non-zero seed starts that child segment.
}
message ForkResponse {
  repeated Lease children = 1;       // tier-A CoW forks (ARCHITECTURE §8.4)
}

message DestroyVmRequest  { Lease lease = 1; }
message DestroyVmResponse { }        // freeing the last child unfreezes/frees the parent
```

### 2.3 Input injection

```proto
message InjectInputsRequest {
  Lease lease                     = 1;
  repeated ScheduledEvent events  = 2;   // must all be in the future (>= current icount)
}
message ScheduledEvent {
  oneof at {                             // when it lands (converted to icount, ARCH §3.3)
    uint64 at_icount  = 1;               // absolute, segment-relative
    uint64 at_vns     = 2;               // virtual ns → icount via clock rational
    uint32 at_frame   = 3;               // the ABSOLUTE guest FRAME_COUNTER value (pv-pad,
                                         //   ARCH §6.4) — NEVER segment-relative. The counter
                                         //   persists across snapshot/restore (DHSNAP PADD) and
                                         //   is strictly increasing along a lineage; each
                                         //   segment's FRAME_MARK table maps absolute F →
                                         //   segment-relative icount. Callers schedule
                                         //   at_frame = frame_counter + offset, reading the
                                         //   base from RestoreSnapshotResponse /
                                         //   TakeSnapshotResponse (§2.2/§2.5).
  }
  oneof event {
    PadSet      pad_set   = 4;           // the demo path: controller bitmask
    DeviceEvent dev_event = 5;           // generic device event
    NetRx       net_rx    = 6;           // loopback packet delivery
  }
}
message PadSet      { uint32 port = 1; uint32 buttons = 2; }   // port 0..3; bitmask
message DeviceEvent { uint32 device_id = 1; uint32 event_type = 2; bytes payload = 3; } // ≤ 4 KiB
message NetRx       { bytes frame = 1; }                       // ≤ 2048 bytes
message InjectInputsResponse { uint32 scheduled = 1; }
```

### 2.4 Run / Pause

```proto
message RunRequest {
  Lease lease = 1;
  oneof until {
    uint64 icount_budget   = 2;   // run this many MORE instructions
    uint64 vns_budget      = 3;   // run this much MORE virtual time
    uint32 frame_budget    = 8;   // run until the frame-boundary exit (the pv-pad
                                  //   FRAME_COUNTER MMIO write, ARCH §6.6) of the Nth
                                  //   FrameMark since run start, then pause. The
                                  //   platform's ONLY frame-quantized stop condition;
                                  //   deterministic via the frame table. "Run N frames"
                                  //   in any caller doc means frame_budget = N — never
                                  //   vns arithmetic: virtual time stays a pure function
                                  //   of icount (ARCH §4). Reports BUDGET_REACHED.
    NextSdkEvent next_sdk_event = 4;  // stop at the next matching SDK event
    GoalCondition goal     = 5;   // stop when predicate holds (polled, see below)
  }
  uint64 hard_icount_cap   = 6;   // safety net for goal/sdk/frame waits; 0 ⇒ worker default (10e9)
  CaptureSpec capture      = 7;   // optional: extract features/framebuffer at the stop
                                  //   boundary (capture engine, ARCHITECTURE §6.10)
}
message NextSdkEvent {
  optional uint32 stream = 1;     // detchannel EventKind filter (guest-sdk API.md §3.1);
                                  // unset ⇒ stop at the next SDK event of ANY kind.
                                  // e.g. set to FRAME_MARK's kind for frame-grid stepping;
                                  // non-matching events are forwarded but don't stop the run.
}
message GoalCondition {
  repeated MemPredicate all_of = 1;     // AND of predicates
  uint64 poll_period           = 2;     // instructions between polls (default 1_000_000).
                                        // Polling happens at deterministic agenda points,
                                        // so the stop boundary is reproducible.
}
message MemPredicate {
  uint64 gpa    = 1;
  uint32 width  = 2;                    // 1|2|4|8
  uint64 mask   = 3;                    // applied before compare (0 ⇒ no mask)
  enum Op { OP_UNSPECIFIED = 0; EQ = 1; NE = 2; GE = 3; LE = 4; }
  Op op         = 4;
  uint64 value  = 5;
}

message RunResponse {
  StopReason reason      = 1;
  uint64 icount          = 2;          // boundary where we stopped
  uint64 vns             = 3;
  StateHash state_hash   = 4;          // chain value at the stop boundary
  uint64 frames_elapsed  = 5;          // FRAME_MARK count during this Run
                                       //   (== frame_budget when a frame_budget run
                                       //   stops with BUDGET_REACHED)
  GuestEvent sdk_event   = 6;          // set when reason == NEXT_SDK_EVENT
  // Capture engine output, set iff request.capture was present. Capture is
  // read-only at the pause boundary and never perturbs execution, the DHILOG,
  // or the state hash (ARCHITECTURE §6.10 C5).
  bytes feature_bytes    = 7;          // ranges packed in request order
  bytes fb_lz4           = 8;          // lz4-compressed framebuffer pixels
  FbInfo fb_info         = 9;
}
enum StopReason {
  STOP_UNSPECIFIED = 0; BUDGET_REACHED = 1; GOAL_SATISFIED = 2; NEXT_SDK_EVENT = 3;
  HARD_CAP = 4; PAUSED = 5;            // external Pause rolled forward to epoch boundary
  GUEST_HALTED = 6;                    // guest executed terminal HLT / triple fault
  FAULTED = 7;                         // guest-contract violation; slot needs Destroy/Restore
}

message PauseRequest  { Lease lease = 1; }
message PauseResponse { uint64 icount = 1; uint64 vns = 2; StateHash state_hash = 3; }
// Semantics: "pause soon at a deterministic point" — the engine stops at the NEXT EPOCH
// BOUNDARY (≤ epoch_len instructions away), never mid-grid. See ARCHITECTURE §3.3.
```

### 2.5 Snapshots

```proto
message TakeSnapshotRequest {
  Lease lease = 1;
  bool seal_input_log = 2;     // true (default): close + store the segment's DHILOG in
                               // snapshot-store, reference it in the response
  CaptureSpec capture = 3;     // optional: extract features/framebuffer at the same
                               //   boundary (capture engine, ARCHITECTURE §6.10)
}
message TakeSnapshotResponse {
  SnapshotRef snapshot   = 1;  // issued by snapshot-store (PutSnapshot)
  bytes input_log_id     = 2;  // snapshot-store log_id of the sealed DHILOG (32 bytes)
  uint64 icount          = 3;
  uint64 vns             = 4;
  StateHash state_hash   = 5;
  uint32 dirty_pages     = 6;  // delta size vs parent (observability)
  bytes machine_config_hash          = 7;  // BLAKE3 of canonical MachineConfig
  DeterminismClass determinism_class = 8;  // this host's tuple (§2.8)
  // The store's .spm manifest carries NO metadata section (§5.1): the orchestrator
  // persists state_hash / machine_config_hash / determinism_class from this response
  // into the lineage node's attrs at commit time, and input_log_id into the node row
  // (a native NodeMeta column). Restore-time gating reads node attrs — see §5.2.
  bytes feature_bytes    = 9;  // capture engine output, iff request.capture set
  bytes fb_lz4           = 10;
  FbInfo fb_info         = 11;
  uint32 frame_counter   = 12; // ABSOLUTE pv-pad FRAME_COUNTER at the snapshot boundary
                               //   (always set, capture or not) — the at_frame base for
                               //   the child segment (§2.3)
}
```

### 2.6 Introspection

```proto
message ReadGuestMemoryRequest {
  Lease lease = 1;
  repeated GpaRange ranges = 2;        // raw-GPA mode; total ≤ 16 MiB per call
  // By-name mode (debug/ad-hoc convenience; the exploration capture path uses
  // CaptureSpec instead): resolved through the guest-sdk region manifest,
  // delegating to detguest-host's Channel::read_region (guest-sdk API.md §2).
  repeated RegionRange region_ranges = 3;
}
message GpaRange    { uint64 gpa = 1; uint64 len = 2; }
message RegionRange { string region = 1; uint32 layout_version = 2;
                      uint64 offset = 3; uint64 len = 4; }
message ReadGuestMemoryResponse {
  repeated bytes chunks = 1;           // 1:1 with ranges, then region_ranges
  uint64 icount = 2;                   // boundary the read is consistent with
}

message GetFramebufferRequest { Lease lease = 1; }
message GetFramebufferResponse {
  uint32 width = 1; uint32 height = 2; uint32 stride = 3;
  PixelFormat format = 4;
  uint32 frame_counter = 5;            // pv-pad FRAME_COUNTER (ARCHITECTURE §6.4)
  uint64 icount = 6;
  bytes pixels = 7;                    // stride*height bytes
}
enum PixelFormat { PF_UNSPECIFIED = 0; XRGB8888 = 1; RGB565 = 2; }

message StreamGuestEventsRequest {
  Lease lease = 1;
  repeated uint32 streams = 2;         // detchannel EventKind values
                                       // (guest-sdk API.md §3.1); empty ⇒ all
}
message GuestEvent {
  uint32 stream = 1;                   // the record's detchannel EventKind
  uint64 icount = 2;                   // doorbell icount (deterministic)
  uint64 vns    = 3;
  bytes payload = 4;                   // record payload; framing owned by guest-sdk
}
```

### 2.7 Verification

```proto
message VerifyReplayRequest {
  SnapshotRef base       = 1;
  oneof log {
    bytes input_log      = 2;          // inline DHILOG bytes — ≤ 4 MiB per segment
                                       // (the cap is snapshot-store's
                                       // input_log_max_bytes, the single source for
                                       // this number; spliced .dilog containers are
                                       // exempt — they live in control-plane's
                                       // artifact registry, not the store, and are
                                       // verified segment-by-segment anyway)
    bytes input_log_id   = 3;          // fetch from snapshot-store
  }
  optional bool bisect_on_divergence = 4; // absent => true; false returns coarse evidence only
}
message VerifyReplayProgress {
  oneof msg {
    EpochOk    epoch_ok   = 1;         // streamed per epoch
    VerifyDone done       = 2;
    Divergence divergence = 3;         // terminal; P0
  }
}
message EpochOk   { uint64 epoch_index = 1; uint64 icount = 2; }
message VerifyDone {
  uint64 total_icount = 1;
  StateHash end_state_hash = 2;        // matched the log's end_state_hash
}
message Divergence {
  uint64 first_bad_epoch   = 1;
  uint64 icount_lo         = 2;        // bisected range when checkpoint evidence exists;
  uint64 icount_hi         = 3;        // coarse evidence point/range when bisection is disabled
  uint64 rip_expected      = 4;
  uint64 rip_actual        = 5;
  bytes  reg_diff          = 6;        // postcard-encoded Vec<RegDiff{name, expected, actual}>;
                                       // empty unless backed by bisection checkpoint evidence
  repeated uint64 diff_page_idx = 7;   // first ≤ 64 differing flattened logical page indices
  string suspected_cause   = 8;        // decoder hint, e.g. "RDTSC at divergent RIP"
}

// ---- RunWithFrameCapture (required by replay-renderer, Phase 7) ----
// Server-streaming frame extraction during a run: one CapturedFrame per FRAME_MARK
// observed, then a terminal RunResponse. Capture-neutral (normative): the capture
// MUST NOT perturb execution, the DHILOG, or the state hash — a capture run and a
// no-capture run of the same (snapshot, inputs) produce identical refs and epoch
// hashes (CI-tested). Backpressure: if the stream consumer stalls, the worker holds
// the vCPU paused at the FRAME_MARK boundary; it never drops frames.
message RunWithFrameCaptureRequest {
  Lease lease = 1;
  oneof until {
    uint64 icount_budget = 2;
    uint64 vns_budget    = 3;
  }
  uint64 hard_icount_cap = 4;          // 0 ⇒ worker default. Currently UNUSED:
                                       //   the budget until arms are taken literally
                                       //   (mirroring Run); the field is reserved
                                       //   for a future event-driven until arm.
}
message FrameCaptureEvent {
  oneof msg {
    CapturedFrame frame = 1;           // streamed per FRAME_MARK
    RunResponse   done  = 2;           // terminal
  }
}
message CapturedFrame {
  uint32 frame_index = 1;              // the FRAME_MARK's frame index (absolute
                                       //   FRAME_COUNTER value, §2.3)
  uint64 icount      = 2;              // the FRAME_COUNTER frame-boundary exit
  bytes  fb_lz4      = 3;              // lz4-compressed framebuffer pixels
  FbInfo fb_info     = 4;
}
```

Amendment (play-60fps, M2/M3 — scope extension of the Phase-7 contract):

- **Interactive play is an approved consumer.** The RPC was specified for
  the offline replay-renderer; the rom-operator-bridge play path now uses
  it live. Capture-neutrality and the never-drop backpressure rule are
  unchanged and CI-tested (`crates/dh-worker/tests/frame_capture_stream.rs`).
  Chain links happen only on the epoch grid plus one final-stop link —
  never per frame.
- **Stopping a streaming run: cancel, not Pause.** Cancelling the gRPC
  stream ends the run AT the next FRAME_MARK boundary (≤1 frame of
  latency) as a Pause-equivalent stop: the slot lands `PAUSED_S` at that
  deterministic icount with the normal final hash link, and the DHILOG
  reflects a normal segment stop. `Pause` remains the exploration/audit
  primitive and is grid-quantized (§2.4): it rolls forward to the NEXT
  EPOCH boundary — up to ~one epoch (≈50 frames / ~1s at ~1M
  instructions/frame) of extra play. Interactive Stop MUST use stream
  cancellation; clients must not be offered both paths with silently
  different latencies.
- **Stalled-consumer watchdog.** A consumer that keeps the stream open
  but stops reading holds the vCPU at the FRAME_MARK boundary; after a
  worker-constant deadline (30s) the run ends at the held boundary
  exactly as a cancel would (terminal `RunResponse` reason `PAUSED`).
  The distinct cause is surfaced in
  `dh_worker_frame_stream_terminations_total{reason="watchdog"}`.
- **"Run until I say stop"** is a large `icount_budget` plus cancel — no
  `frame_budget` arm is added unless operating experience demands it.
- **`InjectInputs` during a streaming frame-hold (M3).** A slot whose
  streaming run is holding at a FRAME_MARK accepts `InjectInputs` for
  events scheduled `at_frame(N)` with N strictly greater than the last
  streamed frame index: they are merged into the active run's
  frame-input set and applied at the matching FRAME_MARK exactly as if
  scheduled before the run started — same injection windows (§3.4), same
  canonical DHILOG records, so DHILOG replay reproduces the run
  bit-for-bit. Events targeting the held or a past frame fail
  INVALID_ARGUMENT (the §2.9 past-target convention); icount-scheduled
  events fail FAILED_PRECONDITION while a streaming run is active (the
  run's landing agenda is fixed at run start). An accepted event the run
  never reaches re-queues on the paused slot rather than being dropped.
  Input-to-effect latency: applied at the next FRAME_MARK hold, visible
  ≤2 frames (~33ms) later.
- **Slot occupancy.** A play session pins one slot's actor thread (and
  its pinned core) for the session's duration; a worker shared with the
  exploration orchestrator loses that slot from its pool while the
  session lasts. The operator bridge uses a dedicated worker today.
- **Observability.** Streaming runs record `/metrics` (§2.8):
  `dh_worker_frames_streamed_total`,
  `dh_worker_frame_emit_duration_milliseconds`,
  `dh_worker_frame_hold_duration_milliseconds`,
  `dh_worker_frame_holds_in_progress`, and
  `dh_worker_frame_stream_terminations_total{reason=
  budget|hard_cap|paused|halted|cancel|watchdog|fault|other}`.

### 2.8 Worker / health

```proto
message GetWorkerInfoRequest {}
message GetWorkerInfoResponse {
  string worker_id = 1;                // stable host id
  uint32 slots_total = 2;
  uint32 slots_free  = 3;
  DeterminismClass class = 4;
  string version = 5;                  // dh-workerd semver
  string build_profile = 6;            // "release" | "debug"; long-lived
                                       //   operator workers MUST be "release"
}
message DeterminismClass {             // ARCH §7.4; embedded in manifests too
  string cpu_model    = 1;             // e.g. family/model/stepping string
  string microcode    = 2;
  string host_kernel  = 3;
  string vmm_version  = 4;             // dh-vmm crate version
}

message ListSlotsRequest {}
message ListSlotsResponse { repeated SlotInfo slots = 1; }
message SlotInfo {
  uint64 slot_id = 1;
  SlotState state = 2;
  uint64 icount = 3;
  SnapshotRef base = 4;                // base snapshot of current segment (if any)
  uint32 live_children = 5;            // for Frozen parents
}
enum SlotState { SLOT_UNSPECIFIED = 0; EMPTY = 1; PAUSED_S = 2; RUNNING = 3;
                 FROZEN = 4; FAULTED_S = 5; }
// PAUSED_S (like FAULTED_S): proto enum values use C++ scoping — siblings of
// the package, not the enum — so SlotState values must not collide with
// StopReason's PAUSED/FAULTED. protoc rejects a bare PAUSED here.
message WatchSlotsRequest {}
message SlotEvent { SlotInfo slot = 1; }   // emitted on every state transition
```

Plus `/healthz` and `/metrics` on HTTP 7401 (MAP.md convention) — not gRPC.

### 2.9 Error model

gRPC status codes: `FAILED_PRECONDITION` (wrong slot state / stale lease),
`NOT_FOUND` (unknown snapshot ref / image hash), `RESOURCE_EXHAUSTED` (no free slots),
`INVALID_ARGUMENT` (schema violations, past-icount injection), `DATA_LOSS` —
**reserved exclusively for determinism violations** (verification divergence,
boundary-engine overshoot); callers must treat `DATA_LOSS` as P0 and page a human.
Error details carry a `dh.ErrorDetail` proto with `slot_id`, `icount`, and a
machine-readable `code`.

### 2.10 Quiesce (Phase 8, optional)

The host-facing initiator for guest-sdk's quiesce protocol (its ARCHITECTURE §6 —
the in-guest relay, token echo, and ring-C `Quiesce` command encoding are owned
there and cited, never restated). Not part of the v1 exploration loop: snapshots
never need quiesce (boundary pauses suffice); this exists for semantically clean
pause points and lands with guest-sdk Ms6 in Phase 8.

```proto
message QuiesceRequest {
  Lease lease            = 1;
  uint64 token           = 2;   // host-chosen; echoed by QuiesceReady / QUIESCE_ACK
                                //   (guest-sdk ARCHITECTURE §6)
  QuiesceMode mode       = 3;
  uint64 hard_icount_cap = 4;   // 0 ⇒ worker default
}
enum QuiesceMode { QUIESCE_MODE_UNSPECIFIED = 0; COOP = 1; FORCED = 2; }
message QuiesceResponse {       // slot is Paused at the ack boundary
  uint64 icount = 1; uint64 vns = 2; StateHash state_hash = 3;
}
```

Semantics: slot must be Paused. The worker pushes the `Quiesce{token, mode}` command
onto detchannel ring C at the pause boundary (a canonical `DEV_EVENT/RING_PUSH`
record, §3.3 — host channel mutations are always inputs), then runs until the guest's
`QuiesceReady{token}` event (or `QUIESCE_ACK` detcall on the FORCED path) and pauses
at that guest-initiated boundary — deterministic like any SDK-event stop. If
`hard_icount_cap` trips before the ack, the run pauses at the cap boundary and the
RPC fails `FAILED_PRECONDITION` (quiesce not achieved; the slot remains usable).

---

## 3. The input log: `DHILOG` v1 (byte-level, normative)

One DHILOG describes one **segment**: execution from a base snapshot to an end
boundary. `base snapshot + DHILOG ⇒ bit-identical re-execution` — this file *is* the
platform's replayability guarantee. Stored in snapshot-store as an opaque blob
(`log_id` = snapshot-store's container hash); also passed inline over gRPC.

All integers little-endian. The file is `header || records || (implicit end)` with
`header.body_hash` sealing the records.

### 3.1 Header — fixed 256 bytes

| Offset | Size | Field | Notes |
|---:|---:|---|---|
| 0 | 6 | `magic` | ASCII `DHILOG` |
| 6 | 2 | `version` | `u16` = `0x0100` (major.minor = 1.0; major in high byte) |
| 8 | 4 | `header_len` | `u32` = 256 |
| 12 | 4 | `flags` | bit0 `SEALED` (complete, hashes valid); bit1 `HAS_AUX` (AUX records present); bit2 `EPOCH_HASHES` (AUX includes EPOCH_HASH records); others 0 |
| 16 | 32 | `base_snapshot_id` | snapshot ref this log replays from |
| 48 | 32 | `end_snapshot_id` | snapshot ref at the end boundary; zeros if no end snapshot was taken |
| 80 | 32 | `entropy_seed` | ChaCha20 seed for the segment (zeros ⇒ continue base snapshot's PRNG stream) |
| 112 | 32 | `machine_config_hash` | BLAKE3 of canonical MachineConfig encoding |
| 144 | 4 | `clock_num` | `u32` |
| 148 | 4 | `clock_den` | `u32` |
| 152 | 8 | `record_count` | `u64` |
| 160 | 8 | `end_icount` | `u64` — icount of the end boundary |
| 168 | 8 | `end_vns` | `u64` |
| 176 | 32 | `end_state_hash` | chain value at end boundary (zeros if unsealed) |
| 208 | 32 | `body_hash` | BLAKE3 of all record bytes `[256, EOF)` (zeros if unsealed) |
| 240 | 8 | `encoder_fingerprint` | `u64` detguest-wire encoder fingerprint (bead 4ld); zero ⇒ no SDK digests in this segment. Verifiers compare fingerprints before SDK_EVENT digests to detect encoder skew |
| 248 | 8 | `reserved` | zeros; readers MUST reject nonzero (reserved-means-zero rule) |

### 3.2 Record framing — fixed 24-byte record header + padded payload

| Offset | Size | Field | Notes |
|---:|---:|---|---|
| 0 | 1 | `kind` | see §3.3 |
| 1 | 1 | `rflags` | bit0 `AUX` (derived record, skippable); others 0 |
| 2 | 2 | `payload_len` | `u16`, ≤ 4096 |
| 4 | 4 | `seq` | `u32`, starts 0, +1 per record (detects truncation/splice) |
| 8 | 8 | `icount` | `u64` — boundary the record lands at / was emitted at |
| 16 | 8 | `boundary_rip` | `u64` — guest RIP at that boundary (cross-check; replayer verifies) |
| 24 | n | `payload` | |
| 24+n | pad | | zero-pad to next 8-byte multiple |

Records MUST be ordered by (`icount`, then `seq`). Canonical records at equal icount
land in `seq` order.

### 3.3 Record kinds

**Canonical (rflags.AUX = 0)** — inputs to execution; replay applies them:

| kind | Name | Payload layout |
|---:|---|---|
| `0x01` | `PAD_SET` | `port: u8, _pad: [u8;3], buttons: u32, frame_hint: u32` (12 B). `frame_hint` = the at_frame value (the absolute FRAME_COUNTER value, §2.3) if frame-scheduled, else `0xFFFF_FFFF`. Replay lands by icount; frame_hint is verified against the FRAME_MARK table. |
| `0x02` | `DEV_EVENT` | `device_id: u16, event_type: u16, data_len: u32, data: [u8; data_len]` |
| `0x03` | `NET_RX` | raw frame bytes (`payload_len` is the frame length, 1–2048; zero-length is INVALID — the device rejects empty delivery, so writer and reader forbid it) |

**`DEV_EVENT` payload encodings for the detchannel** (`device_id = 0x0001`) —
normative, frozen with DHILOG v1. Every host-side mutation of detchannel memory is an
input to execution (guest-sdk's load-bearing invariant, its ARCHITECTURE §2/§7) and is
recorded as one of these canonical records via the `ChannelWriteSink` hook (guest-sdk
API.md §2). Ring ids follow guest-sdk's `ring_desc` order: `0 = C, 1 = I, 2 = A, 3 = W`.

| `event_type` | Name | `data` layout | Records / replay applies |
|---:|---|---|---|
| `0x0001` | `RING_PUSH` | `ring_id: u8 (0=C, 1=I), _pad: [u8;3], new_prod: u32, record_bytes: [u8; data_len-8]` | A host push of a command (ring C) or input (ring I): the record bytes written and the producer index released. Replay re-writes the bytes and index at the recorded icount. |
| `0x0002` | `CONS_BUMP` | `ring_id: u8 (2=A, 3=W), _pad: [u8;3], new_cons: u32` | A consumer-index bump after draining a guest→host ring (pause-boundary or doorbell drain). Guest-visible (flow control / drop behavior depends on it); replay re-writes the index at the recorded icount. |
| `0x0003` | `PIO_ANSWER` | `port: u16, _pad: u16, value: u32` | The value returned by an `IN` detcall (ports `0xD370–0xD39F`, guest-sdk API.md §5). Replay re-answers the same `IN` exit with the recorded value. |

**AUX (rflags.AUX = 1)** — derived from execution; written during recording, *compared*
during verification, skippable by minimal replayers:

| kind | Name | Payload layout |
|---:|---|---|
| `0x40` | `ENTROPY` | `len: u32, _pad: u32, digest8: u64` (first 8 bytes of BLAKE3 of the bytes served) |
| `0x41` | `TIMER_FIRE` | `vector: u8, _pad: [u8;3], armed_deadline_vns: u64, delivered_icount: u64` (delivery may defer past the arm target per the §3.4 injectability rule — ARCHITECTURE) |
| `0x42` | `EPOCH_HASH` | `epoch_index: u64, chain_value: [u8;32]` |
| `0x43` | `SDK_EVENT` | `stream: u16, _pad: u16, len: u32, digest8: u64` (`stream` carries the detchannel EventKind, guest-sdk API.md §3.1; payloads live in the gRPC stream, not the log) |
| `0x44` | `NET_TX` | `len: u32, _pad: u32, digest8: u64` |
| `0x45` | `FRAME_MARK` | `frame_index: u32, _pad: u32` (the per-segment frame table; `frame_index` is the **absolute** FRAME_COUNTER value, so the table maps absolute F → segment-relative icount — it resolves at_frame scheduling and `frame_budget` stops, and lets replay-renderer find frame boundaries) |
| `0x46` | `BISECTION_CHECKPOINT` | `format_version: u16 (=1), flags: u16, max_covered_gap: u32, checkpoint_snapshot_ref: [u8;32], checkpoint_icount: u64, checkpoint_vns: u64` |
| `0x7F` | `END` | `stop_reason: u8` (mirrors proto StopReason), `_pad: [u8;7]`, `end_state_hash: [u8;32]` — always last record, always present in sealed logs |

### 3.4 Semantics (normative)

1. **Replay** = restore `base_snapshot_id`, seed PRNG from header, schedule every
   canonical record at its `icount`, run to `end_icount`, land the END boundary.
   Resulting chain value MUST equal `end_state_hash`.
2. **Verification** additionally recomputes every AUX record and compares: entropy
   digests, timer delivery icounts, epoch chain values, SDK digests, boundary RIPs.
   First mismatch ⇒ divergence (bisect per ARCHITECTURE dh-verify).
   Native bisection diagnostics require `BISECTION_CHECKPOINT` records. A checkpoint
   record names a full, non-mutating snapshot-store checkpoint captured at that record's
   `icount`; the record header's `boundary_rip` is the recorded RIP at the checkpoint.
   `max_covered_gap` is the maximum distance to the previous checkpoint that this
   evidence can justify. To emit a ≤1024-instruction `Divergence` range, the relevant
   checkpoint gap MUST be ≤1024. Wider gaps may only produce the wider evidence-backed
   window. Logs without checkpoint records are checkpoint-less: `bisect_on_divergence =
   true` fails with `FAILED_PRECONDITION` naming the missing artifact, while
   `bisect_on_divergence = false` returns the coarse epoch/hash divergence with
   `rip_expected = rip_actual = 0` and empty `reg_diff` / `diff_page_idx`.
   Checkpoint snapshots are diagnostic artifacts: taking one MUST NOT clear dirty-page
   state, reseed entropy, advance the segment, perturb the DHILOG except for the AUX
   record, or change the final snapshot/log lineage.
3. **Concatenation**: logs compose along snapshot lineage — if `L1.end_snapshot_id ==
   L2.base_snapshot_id`, replaying `L1` then `L2` from `L1.base_snapshot_id` is defined
   and is how replay-renderer reconstructs root→node trajectories. (Each log's icounts
   restart at 0 from its own base; there is no global icount.)
4. An unsealed log (crash artifact) is identified by `flags.SEALED == 0`; it MUST NOT
   be stored in snapshot-store or used for replay.

### 3.5 Bisection diagnostics payloads

`Divergence.reg_diff` is postcard-encoded `Vec<RegDiff>`:

```rust
struct RegDiff {
    name: String,      // canonical vCPU field path, e.g. "regs.rax" or "sregs.cr3"
    expected: Vec<u8>, // little-endian canonical field bytes from checkpoint DHSNAP VCPU
    actual: Vec<u8>,   // little-endian canonical field bytes from replay probe
}
```

The source of truth is the canonical DHSNAP `VCPU` section order, never raw padded KVM
struct memory. `diff_page_idx` is computed by flattening the recorded checkpoint
snapshot and replay probe snapshot through `ResolvePages(hashes_only = true)` and
comparing logical page hashes by page index; delta manifest entries alone are
insufficient evidence.

---

## 4. The device blob: `DHSNAP` v1

The opaque `device blob` embedded in every snapshot manifest (snapshot-store stores it
byte-identically — its bytes are part of the snapshot ref). Layout:

| Offset | Size | Field |
|---:|---:|---|
| 0 | 6 | `magic` = ASCII `DHSNAP` |
| 6 | 2 | `version` = `0x0100` |
| 8 | 4 | `section_count: u32` |
| 12 | 4 | `_pad` |
| 16 | — | sections, back to back |

Section: `tag: [u8;4]`, `sec_version: u16`, `_pad: u16`, `len: u32`, then `len` bytes,
zero-padded to 8-byte alignment. Readers MUST reject unknown tags (device state is
never optional). Defined tags and contents (each section is itself fixed-layout;
authoritative struct definitions live in `dh-snapshot::dhsnap` with golden tests):

| Tag | Contents |
|---|---|
| `MCFG` | canonical `MachineConfig` encoding — the `machine_config_hash` preimage. The store's manifest carries no machine-config metadata (§5.1), so restore recovers the config from here (§5.2) |
| `VCPU` | `kvm_regs`, `kvm_sregs2`, `kvm_fpu`, canonicalized XSAVE area (len-prefixed), `kvm_xcrs`, MSR list as `count u32 + (index u32, _pad u32, value u64)*`, `kvm_vcpu_events`, `kvm_debugregs` — raw kvm-bindings structs, byte-copied (they are `repr(C)`, stable ABI), with the XSAVE canonicalization of ARCHITECTURE §8.1 |
| `LAPC` | lapic-stub state (Rust struct, fixed-layout encode) |
| `TIME` | `cumulative_icount u64, vns u64, epoch_index u64, hash_chain [u8;32]` |
| `ENTR` | VERSIONED by `sec_version`. v1: ChaCha20 PRNG state, exactly `seed: [u8;32], stream: u64, word_pos: u128` (56 bytes) — `rand_chacha`'s exportable state (`get_seed`/`get_stream`/`get_word_pos`); restore re-seeds via `set_stream`/`set_word_pos` and MUST reproduce the next draws bit-identically (golden test, IMPLEMENTATION-PLAN M4). v2 (what the snapshot engine writes): the v1 56 bytes ‖ the pv-entropy device's guest-visible regs `buf_gpa u64, len u32, status u32` (72 bytes total) — the regs have no §4 tag of their own |
| `CLKD` | pv-clock device regs (timer deadline, vector) |
| `PADD` | pv-pad latches `[u32;4]`, irq vector, frame_counter |
| `EVTC` | detchannel attach state: channel base GPA `u64` (0 = not attached). All ring, manifest, and index state lives in guest RAM and travels with the pages (guest-sdk ARCHITECTURE §2); the host re-attaches at this GPA after restore |
| `BLKO` | pv-blk overlay: `cluster_count u32`, then `(cluster_idx u32, blake3 [u8;32])*` for clusters dirty **since the parent snapshot**, followed by the cluster bytes; plus `total_overlay_clusters u32` for integrity |
| `NETL` | pv-net registers only (36 bytes: tx_buf_gpa u64, tx_len u32, tx_status u32, rx_buf_gpa u64, rx_cap u32, rx_len u32, rx_vector u32). The original "pending-RX state (must be empty at snapshot; enforced)" is satisfied BY CONSTRUCTION: the device buffers no frames (TX is drained per exit by run control; RX delivery is immediate at record landing), so no pending state exists to serialize — iteration-85 amendment |
| `SERL` | debug-serial (empty section; serial is stateless for hashing) |

---

## 5. Snapshot manifest interchange with snapshot-store

snapshot-store owns the manifest container format and computes the **snapshot ref**
(BLAKE3 of the manifest's canonical bytes — see `../snapshot-store/`). This service is
the *producer/consumer*; the interchange is:

### 5.1 TakeSnapshot → store (producer)

1. `PUT_BATCH` over the page channel (UDS `SOCK_SEQPACKET` + memfd fd-passing,
   `/run/snapstore/pages.sock` — byte protocol in the store's API.md §4): the worker
   writes the dirty pages **back-to-back as bare 4096-byte payloads into a memfd**
   (≤ 8192 pages / 32 MiB per batch) and passes the fd. **The server hashes each page
   itself** (hashing authority) and dedups; no page indices and no client-side hashes
   travel on the wire — indices exist only in the manifest the worker builds. The
   worker computes the same per-page hashes anyway (it needs them for the manifest
   entry table) and cross-checks `PutOkBody.batch_blake3` against its own
   concatenated-hash digest; a mismatch is a P0 determinism bug. (Pure-gRPC fallback:
   `PutPages` with the same bare-page batches.)
2. `PutInputLog`: the sealed DHILOG (if `seal_input_log`), receiving `log_id`.
3. `PutSnapshot` with the client-built `.spm` container (store's byte format, its
   API.md §2). The parts this service supplies:

| `.spm` field (store's schema) | We supply |
|---|---|
| `parent_manifest_hash` + `DELTA` flag | base snapshot ref of the segment (FULL for roots and every `max_delta_chain` generations) |
| page entry table | sorted, unique `[(page_index, page_hash)]` of pages dirtied since parent |
| `device_blob` + `device_blob_format` | the DHSNAP bytes (§4), including the `MCFG`, `TIME`, and `ENTR` sections — opaque to the store |

   The `.spm` container has **no metadata section**: `state_hash`,
   `machine_config_hash`, `determinism_class`, `icount`/`vns`, and `input_log_id` are
   **not** in the manifest, and no doc may cite `manifest.meta.*`. They are returned
   in `TakeSnapshotResponse` (§2.5); the orchestrator persists
   `state_hash`/`machine_config_hash`/`determinism_class` into the lineage node's
   `attrs` at commit time and `input_log_id` into the node row (a native `NodeMeta`
   column in the store's tree schema).

4. Store returns the **snapshot ref**; the worker returns it in `TakeSnapshotResponse`.
   A returned ref is durable (store's crash-consistency guarantee).

### 5.2 RestoreSnapshot ← store (consumer)

`GetSnapshot(ref)` → `.spm` container; pages via `ResolvePages` + page-channel
`GET_BATCH` (or the materialized-file fast path for `mmap(MAP_PRIVATE)` restore —
ARCH §8.4 tier B); `device_blob` bytes → DHSNAP decode.

Restore-time checks — everything needed lives in the device blob, nothing in the
manifest:

- The worker decodes the DHSNAP `MCFG` section into `RestoreSnapshotResponse.config`
  and verifies it is a config this build can serve (unknown version / unsupported
  device set ⇒ `FAILED_PRECONDITION`). `machine_config_hash` is recomputed from the
  decoded MCFG bytes and checked against the DHILOG header on any replay.
- **Determinism-class gating is the caller's job and reads node attrs.** The worker
  has no class record to check at restore (the manifest carries none). The
  orchestrator and replay-renderer read `determinism_class` from the lineage node's
  attrs (persisted at commit per §5.1) and match it against `GetWorkerInfo().class`
  *before* dispatching a job to a worker. Verification jobs hard-require class
  equality; exploration jobs may proceed best-effort at the caller's discretion.

### 5.3 Invariants

- Pages referenced by a returned ref are immutable and GC-protected by the store while
  the lineage node lives; the worker never caches pages across `DestroyVm` except via
  the store's materialized files.
- The device blob round-trips **byte-identically** (store contract) — required because
  its bytes participate in the snapshot ref and in the state-hash definition.
- `state_hash` equality between two snapshots (as returned in `TakeSnapshotResponse`
  and persisted in node attrs) does NOT imply ref equality (different parents can
  converge); dedup-by-state is state-scorer's job, not the store's, not ours.
