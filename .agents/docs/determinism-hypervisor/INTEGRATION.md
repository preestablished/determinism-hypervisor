# determinism-hypervisor — Integration

How every sister service touches this one. Control flows through
`exploration-orchestrator` (and later `control-plane`); bulk data flows: pages ↔
snapshot-store on-box, while feature bytes + lz4 framebuffer return **inline** on
`Run`/`TakeSnapshot` responses to the orchestrator, which forwards them to
state-scorer in `ScoreBatch` (MAP.md dataflow step 4). This service never initiates
work — it is a passive pool of slots driven by RPCs.

| Peer | Direction | Transport | What |
|---|---|---|---|
| snapshot-store | worker → store | UDS gRPC `/run/snapstore/grpc.sock` + page channel `/run/snapstore/pages.sock` | page-channel `PUT_BATCH` / PutSnapshot / PutInputLog on TakeSnapshot; GetSnapshot / `GET_BATCH` / materialized-file on Restore. On-box only. |
| exploration-orchestrator | orch → worker | gRPC TCP :7400 (or UDS if co-located) | Fork / RestoreSnapshot / InjectInputs / Run / TakeSnapshot / ReadGuestMemory / GetFramebuffer / DestroyVm / ListSlots / WatchSlots — the orchestrator's worker-driver module composes this lease API (its docs own the composition; §2 below is the canonical usage). Sole issuer of leases during exploration. Capture: `CaptureSpec` on Run/TakeSnapshot returns `feature_bytes` + `fb_lz4` inline (ARCH §6.10). |
| state-scorer | none | — | **Never talks to this service.** The scorer receives `feature_bytes`/`fb_lz4` inline in `ScoreBatch` from the orchestrator; no scorer-pull path exists (the scorer's own non-goals exclude touching guest memory). |
| replay-renderer | renderer → worker | gRPC TCP :7400 | VerifyReplay per segment for proof; `RunWithFrameCapture` (Phase 7) — or RestoreSnapshot + per-frame Run stops — for frame extraction (re-exec on Intel, encode on Spark). |
| guest-sdk | guest → worker (in-VM) | detchannel + pv devices (ARCHITECTURE §6) | detchannel events out (rings A/W); pad/entropy/clock/blk contracts in. Surfaced to the outside via StreamGuestEvents. |
| input-synthesizer | none | — | Never talks to this service. Bursts go synthesizer → orchestrator → InjectInputs. |
| control-plane / observatory | scrape / later | HTTP :7401 | /healthz, /metrics; structured logs shipped by the host agent. v1 has no direct gRPC to either (scrape-only; this service is not an event producer in v1). Image-cache blob fetch from control-plane arrives in Phase 6 (ARCH §9). |

---

## 1. Slot leasing protocol (orchestrator contract)

- `Fork`/`RestoreSnapshot`/`CreateVm` return a `Lease{slot_id, token}`. Every mutating
  RPC echoes it; a stale token gets `FAILED_PRECONDITION`. Leases have no timeout in
  v1 (trusted single orchestrator); `DestroyVm` releases.
- Jobs are pure functions of `(snapshot_ref, burst, seed)` — on worker failure the
  orchestrator may blindly re-run the job on any other slot/host (same determinism
  class) and will get the identical child snapshot ref.
- A `Frozen` parent (live CoW children) cannot run; the orchestrator's scheduler must
  `DestroyVm` children promptly after `TakeSnapshot` to recycle slots. `WatchSlots`
  exists so the scheduler can maintain an accurate free-slot view without polling.

## 2. Sequence: one exploration step (MAP.md dataflow, steps 2–5) — canonical usage

The orchestrator's **worker-driver module** composes this service's lease API; there
is no job-level RPC (`RunBurst`-style composites do not exist — the composition below
*is* the contract). One expansion of frontier node `N` with burst `B` on one slot.
(`K` bursts = `Fork count=K` and `K` parallel copies of the right half.)

```
exploration-      input-        dh-workerd          snapshot-store      state-scorer
orchestrator      synthesizer   (Intel box)         (Intel box)         (DGX Spark)
     │                │              │                    │                  │
     │ ProposeBursts(ctx)            │                    │                  │
     │───────────────>│              │                    │                  │
     │<─ K bursts ────│              │                    │                  │
     │                               │                    │                  │
     │ RestoreSnapshot(ref_N, seed)  │                    │                  │
     │──────────────────────────────>│  GetSnapshot(ref_N)│                  │
     │                               │───────────────────>│                  │
     │                               │<── manifest + mmap file (tier B) ─────│
     │                               │  [or: Fork(lease_N, K) if N is hot    │
     │                               │   on this worker — tier A CoW]        │
     │<── Lease{slot,token}, hash ───│                    │                  │
     │                               │                    │                  │
     │ InjectInputs(lease, B as PadSet@at_frame…)         │                  │
     │──────────────────────────────>│ (agenda built)     │                  │
     │ Run(lease, frame_budget=B.frames)                  │                  │
     │──────────────────────────────>│                    │                  │
     │                               │ boundary engine:   │                  │
     │                               │  land→inject→run…  │                  │
     │<── RunResponse{icount, vns, state_hash, frames} ───│                  │
     │                               │                    │                  │
     │ TakeSnapshot(lease, capture=CaptureSpec{ranges, framebuffer})         │
     │──────────────────────────────>│ PUT_BATCH(bare dirty pages, memfd)    │
     │                               │───────────────────>│ (server hashes,  │
     │                               │ PutInputLog(DHILOG)│  dedups)         │
     │                               │───────────────────>│                  │
     │                               │ PutSnapshot(.spm container)           │
     │                               │───────────────────>│                  │
     │                               │ capture engine: read manifest regions,│
     │                               │  pack feature_bytes, lz4 framebuffer  │
     │<── {ref_child, log_id, state_hash, machine_config_hash,               │
     │     determinism_class, feature_bytes, fb_lz4, fb_info} ───            │
     │ DestroyVm(lease) ────────────>│  (slot freed immediately)             │
     │                                                    │                  │
     │ ScoreBatch{items: [{feature_bytes, fb_lz4, state_hash, …}], …}        │
     │─────────────────────────────────────────────────────────────────────>│
     │<── ScoreResults{progress, novelty, duplicate, stage, prune, …} ───────│
     │                                                                       │
     │ commit/discard: CreateNode(ref_child, log_id, scores,                 │
     │   attrs{state_hash, machine_config_hash, determinism_class})          │
     │   → snapshot-store                                 │                  │
```

Notes:
- **Bursts are frame-quantized.** The run step is `Run(frame_budget = burst frames)`
  (API.md §2.4) — the platform's only frame-quantized stop condition. The pause lands
  on the frame-boundary exit of the last frame, so the capture is never torn, and
  every `at_frame`-scheduled PAD_SET inside the burst has been consumed before
  TakeSnapshot (whose empty-agenda precondition therefore holds, ARCH §8.1). "Run N
  frames" is always `frame_budget = N`, never vns arithmetic — virtual time stays a
  pure function of icount (ARCH §4). `at_frame` values are **absolute** FRAME_COUNTER
  values: the orchestrator schedules `parent frame_counter + offset`, reading the base
  from `RestoreSnapshotResponse`/`TakeSnapshotResponse` (API.md §2.2/§2.5).
- **The scorer never appears between the worker and the orchestrator.** Features and
  framebuffer come back inline with `TakeSnapshotResponse` (or `RunResponse`, if the
  orchestrator puts the `CaptureSpec` on the final `Run` instead — same boundary,
  same result); the orchestrator forwards them inline in `ScoreBatch`. Consequence:
  the slot can be destroyed **immediately** after TakeSnapshot — no lease is held
  across a cross-host scoring round trip, which is what makes slot recycling cheap.
- The orchestrator compiled `CaptureSpec.ranges` from the experiment's feature map
  once at experiment start; this service stays feature-map-agnostic (ARCH §6.10).
- Discarded children cost nothing in the store beyond already-deduped pages: the
  orchestrator simply never creates a node, and the store GC reclaims unreferenced
  delta pages later.

## 3. Sequence: replay verification (the proof pipeline)

Triggered by control-plane when a goal node is found; executed by replay-renderer.

**This per-segment model is the canonical verification model for the platform**
(replay-renderer's `.dilog` v2 container cites it): each lineage edge is verified
independently as `VerifyReplay(base = snapshot_{i-1}, log_i)` against that segment's
own `end_state_hash`, and the whole-trajectory proof is **root-anchored induction** —
segment *i*'s `base_snapshot_id` must equal the verified output ref of segment *i−1*
(content-addressed refs make the induction sound: equal ref ⇒ bit-identical state).
There is no single-root flat-digest pipeline; the chained state hash (ARCH §8.5) and
DHILOG v1 stay frozen as-is.

```
replay-renderer        snapshot-store            dh-workerd (Intel)
     │                       │                        │
     │ GetPath(goal_node)    │                        │
     │──────────────────────>│                        │
     │<─ [(node_i, snapshot_ref_i, log_id_i)] root→goal
     │                       │                        │
     │ GetInputLog(log_id_i) ∀i                       │
     │──────────────────────>│                        │
     │<─ DHILOG_1 … DHILOG_n │                        │
     │  verify lineage stitching: DHILOG_i.end_snapshot_id == DHILOG_{i+1}.base_snapshot_id
     │                       │                        │
     │ for i in 1..=n:       │                        │
     │   VerifyReplay{base=snapshot_ref_{i-1}, input_log_id=log_id_i}
     │────────────────────────────────────────────────>│
     │                       │<── GetSnapshot(base) ───│
     │                       │    re-execute; recompute epoch hashes,
     │                       │    entropy digests, boundary RIPs
     │<── stream EpochOk(0..m) ────────────────────────│
     │<── VerifyDone{end_state_hash} ──────────────────│
     │   check end_state_hash == DHILOG_i.end_state_hash
     │                       │                        │
     │ ── on any Divergence{icount_lo..hi, reg_diff, …} ──> P0:
     │    file with full diagnostics; the trajectory is NOT certified; halt pipeline.
     │                       │                        │
     │ frame extraction pass (after all segments verify):
     │   RestoreSnapshot(snapshot_ref_{i-1}) ─────────>│
     │   RunWithFrameCapture{icount_budget = DHILOG_i.end_icount} ──>│
     │<── stream CapturedFrame{frame_index, fb_lz4} per FRAME_MARK ──> Spark encode
     │   (alternative, per-frame stepping: loop
     │    Run{until: next_sdk_event{stream: FRAME_MARK kind}} + GetFramebuffer;
     │    the FRAME_MARK table in each DHILOG also gives the frame→icount grid up
     │    front, so Run{icount_budget} directly between frames works too.
     │    RunWithFrameCapture is capture-neutral: it adds nothing to the DHILOG and
     │    leaves all hashes identical — API.md §2.7.)
```

The verification re-execution **is the proof** (MAP.md principle 4): a certified
trajectory means every segment re-executed bit-identically on the determinism class
recorded in its lineage nodes' attrs (persisted at commit from
`TakeSnapshotResponse` — API.md §5.1; verification jobs hard-require class equality,
checked by the caller against `GetWorkerInfo().class` per §5.2).

## 4. Divergence bisection (inside VerifyReplay, dh-verify)

```
1. Replay full segment comparing EPOCH_HASH records → first bad epoch e.
2. Binary search inside (e-1, e]: restore base, run to midpoint icount, compare chain
   value against a fresh reference replay's value at the same icount (reference values
   are recomputed on demand; determinism up to the divergence point makes prefix
   re-runs exact). Each probe is one re-execution of ≤ epoch_len instructions from the
   nearest verified boundary.
3. Narrow to ≤ 1024 instructions, then run the reference and suspect in lockstep
   single-step from icount_lo, comparing KVM_GET_REGS each step → first diverging
   instruction; decode bytes at RIP (suspected_cause: RDTSC/RDRAND/unfiltered MSR/...).
4. Emit Divergence{} with reg and page diffs. Mark slot FAULTED_S; never reuse it
   without DestroyVm (host-state suspicion).
```

## 5. guest-sdk contract summary (the channel is guest-sdk's; these are our obligations)

The guest↔host channel is **guest-sdk's detchannel** — one 2 MiB guest-RAM page
(header + region manifest + four SPSC rings) plus the PIO detcall ABI at
`0xD370–0xD39F`. Its layout, framing, event kinds, and register semantics are
specified in `../guest-sdk/ARCHITECTURE.md` §2 and `API.md` §3–§5 and are cited, not
restated. What each side must do:

**Hypervisor-side obligations (this service, ARCH §6.6):**
- Implement the PIO detcall handler (`KVM_EXIT_IO`) for the full window: IDENT,
  CHANNEL_INIT, DOORBELL, INJECT, QUIESCE_ACK.
- At CHANNEL_INIT: validate + attach (`detguest_host::Channel::attach`), record the
  channel GPA (DHSNAP `EVTC`), read the region manifest — and re-read it after every
  restore (it is guest RAM; no event replay needed).
- Record **every** host-side channel mutation (ring-C/I push, consumer-index bump,
  detcall `IN` answer) as a canonical DHILOG `DEV_EVENT` record (encodings: API.md
  §3.3), touching channel memory only while the vCPU is paused.
- Enforce the FRAME_MARK consistency rule (ARCH §6.6) and maintain the per-segment
  frame→icount table.

**In-guest obligations (guest-sdk / the workload):**
- Discover pv devices at fixed GPAs (ARCHITECTURE §2.2 map) via `BootInfo`/cmdline;
  initialize the detchannel via the CHANNEL_INIT detcalls before the `Hello`/READY
  beacon (the deterministic READY point is defined in guest-sdk's contract).
- Signal each emulated video frame per the FRAME_MARK consistency rule: framebuffer
  fully written → `FrameMark` record on ring W → pv-pad `FRAME_COUNTER` MMIO write
  (the frame-boundary exit; no doorbell — guest-sdk API.md §1.6).
- Read pad input **only** from the pv-pad MMIO latch, once per frame (the SDK's
  `poll_input` wraps the latch read); never cache across frames. Ring I carries no
  pad data.
- Use pv-clock exclusively for time; never RDTSC/RDTSCP/RDRAND/RDSEED (the kernel
  image is built with these paths compiled out; CR4.TSD set).
- All disk writes go to pv-blk (land in the overlay); the base image is immutable.
- A guest that violates the contract doesn't break the platform — it breaks its own
  replays, which verification mode catches and attributes (suspected_cause).

## 6. Failure handling expectations per peer

| Failure | Behavior |
|---|---|
| snapshot-store unreachable | TakeSnapshot/Restore fail `UNAVAILABLE`; slots keep running; worker retries nothing itself (callers own retry policy — jobs are pure). |
| Capture against an unknown region / stale `layout_version` | `FAILED_PRECONDITION` (capture engine C2, ARCH §6.10). The orchestrator's compiled extraction list is out of date vs the guest image — re-compile from the feature map, never retry blindly. |
| Worker crash | All slots die with it (slots are process-local). Orchestrator detects via WatchSlots stream break + gRPC errors, reschedules in-flight jobs elsewhere; nothing durable is lost (durable state lives in snapshot-store only). |
| Divergence (`DATA_LOSS`) anywhere | P0. The worker quarantines the slot, increments `dh_determinism_failures_total` (alert threshold: > 0), and attaches full Divergence diagnostics. Humans get paged; the experiment may continue on other workers at orchestrator discretion. |
| Guest triple fault / terminal HLT | `RunResponse{reason: GUEST_HALTED}` — a legitimate, deterministic outcome (the scorer will score it terribly). Not an error. |
