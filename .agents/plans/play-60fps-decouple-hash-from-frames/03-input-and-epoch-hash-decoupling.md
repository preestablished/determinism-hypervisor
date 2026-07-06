# M3 — Live Input at Frame-Hold; M4 — Epoch-Hash Off the Critical Path

## M3 — Pad input while a streaming run is held at a FRAME_MARK

### Problem

`RunWithFrameCapture` (M2) removes per-frame Run stops, but the current
input path (`InjectInputs`, applied while the slot is paused between
Runs) then only gets a word in at streaming-run boundaries. Chopping play
into short runs to bound input latency reintroduces per-stop
`hash_final_stop` links (~50ms each at 128 MiB) — the thing M2 removed.

### Design

During a streaming run, the vCPU is regularly paused ON a FRAME_MARK
boundary (the backpressure hold — with a paced 60Hz consumer this happens
every frame). A frame-boundary hold is a deterministic icount. Accept
`InjectInputs` for a slot in this held state:

- events scheduled `at_frame(N)` for N strictly greater than the held
  frame index are merged into the active run's scheduled-frame-input set;
- delivery happens exactly as if the events had been scheduled before the
  run started: same injection windows (§3.4), same DHILOG records —
  replay of the DHILOG reproduces the run bit-for-bit. Determinism is
  preserved because inputs are *recorded*, not because they are
  pre-declared.

### Concurrency: InjectInputs must bypass the slot actor channel

Two facts the naive implementation gets wrong:

1. `InjectInputs` does NOT reject a running slot — the state machine
   allows writes on `Paused` and `Running` (`ensure_write_path`,
   `crates/dh-vmm/src/lib.rs:150`). Instead it routes through
   `with_runtime_mut` onto the slot's single-threaded `SlotActor`
   (`crates/dh-worker/src/runtime.rs:231-250`), whose mpsc queue is
   drained one closure at a time. The entire streaming run is ONE such
   closure, so an `InjectInputs` issued mid-run silently queues behind it
   and blocks until the run ends — for M2 that is the whole play session.
   A hang, not an error.
2. The vmm's `frame_inputs: &[ScheduledFrameInput]` parameter
   (`run_segment_inner`, `crates/dh-vmm/src/runctl.rs:413`) is an
   immutable slice fixed at call entry. "Merging into the active run's
   set" is NEW vmm-layer work, not an existing capability.

Design accordingly, mirroring the one pattern that already crosses this
boundary — the async-Pause flag (`pause: Arc<AtomicBool>`,
`runtime.rs:174`), which any thread can set without an actor command:

- construct a shared injection queue (e.g. `Arc<Mutex<VecDeque<
  ScheduledEvent>>>`) alongside the `SlotActor`/runtime and hand a clone
  to the gRPC layer;
- `InjectInputs` on a slot with an active streaming run validates the
  lease and target frame, pushes to the queue, and returns immediately —
  it must NOT go through `with_runtime_mut`;
- the vmm grows a way to consume it: either thread an
  `Option<&dyn Fn() -> Vec<ScheduledFrameInput>>` pull-hook into
  `run_segment_inner`'s FRAME_MARK branch, or have the worker's frame
  callback drain the queue and feed a shared, interior-mutable input set
  the FRAME_MARK branch reads. Every drained event is DHILOG-logged
  exactly as pre-scheduled ones are;
- rejection rules (past/held frame → FAILED_PRECONDITION) evaluate
  against the last streamed frame index, which the worker already tracks.

### API/spec impact

API.md amendment: define `InjectInputs` semantics for a slot in a
streaming-run frame-hold (accepted; applied from the next frame boundary;
rejected with FAILED_PRECONDITION if the target frame is not in the
future). CI determinism test: a run with live-injected inputs replays
identically from its DHILOG.

### Latency accounting

Input arrives → applied at the next FRAME_MARK hold → visible ≤2 frames
(~33ms) later. Comparable to standalone emulator + display latency.

### Stop/Pause latency during a streaming run

`PauseRequest` is grid-quantized by design: the engine rolls forward to
the NEXT EPOCH boundary before reporting Paused (API.md §2.4,
`runctl.rs` pause handling). Mid-stream, that is up to ~one epoch of
extra play after the operator hits Stop (~50 frames / ~1s at ~1M
instr/frame). Interactive Stop therefore goes through gRPC **stream
cancellation** (or the 02 watchdog path), which lands at the current
frame boundary — ≤1 frame of latency. Do not expose both paths to the
bridge with silently different latencies: the bridge uses cancel, Pause
remains the exploration/audit primitive, and the API.md amendment states
both numbers.

## M4 — Epoch-hash latency (only if M0 numbers demand it)

Epoch links (`EpochsOn`, every 50M instructions) remain synchronous: the
vCPU is paused while `push_final_link`-style full-memory hashing runs
(~50ms release at 128 MiB). Whether this matters depends on
instructions-per-frame (M0):

- ~1M instr/frame → one ~50ms stall every ~50 frames → occasional 3-frame
  hiccup. Likely acceptable for play; frames are never dropped (stream
  just bunches). Ship M2+M3 and stop here.
- ~10M+ instr/frame → a stall every few frames → 60fps unreachable.
  Pick one of the mitigations below.

### Option A (preferred): shadow-copy + async hash — chain byte-identical

At the epoch boundary, instead of hashing guest RAM in place:

1. maintain a host-private shadow copy of guest RAM plus a dirty-page set
   since the last epoch (KVM dirty-ring/dirty-log — the M4 snapshot-codec
   direction already reserved in `crates/dh-snapshot`);
2. at the boundary, copy only the dirty pages into the shadow (SNES
   workload dirties a small fraction of 128 MiB per epoch), capture the
   canonical vCPU blob + device sections (cheap), resume the vCPU;
3. a hasher thread computes the FULL-memory walk over the shadow and
   appends the link. Preimage, order, and values are identical to today.

**The hash stays a full-memory walk.** ARCHITECTURE.md §8.5's wording
("for each page dirtied since previous hash point") describes an eventual
dirty-delta preimage that Phase-1 code deliberately does not implement
(`hash.rs` module doc: full walk; "M4 extends, never replaces"). Option A
uses dirty tracking only to maintain the shadow COPY cheaply — the hasher
still walks every shadow page. Switching the preimage to dirty-pages-only
would change every chain value and violate this plan's core constraint;
an implementer reading §8.5 literally could make that mistake, so say it
in the code too.

**Dirty-ring contention with the snapshot engine.** The per-vCPU dirty
ring has a free-running, never-rewinding harvest cursor
(`crates/dh-vmm/src/dirty.rs`), and `snapshot_engine.rs` already harvests
it on every `TakeSnapshot` (`harvest_at_boundary`). If the epoch hasher
harvests the same ring independently, a snapshot taken between two epochs
silently steals dirty entries and the shadow under-copies (wrong chain —
the failure mode is corruption, not slowness). Either (a) single harvest
point fanning out to both consumers, or (b) a dedicated dirty accumulator
for the shadow that every harvest feeds and only the epoch hasher resets.
Decide before implementation; (b) is simpler to reason about.

Ordering constraint: the chain is sequential; the hasher must finish link
N before link N+1's inputs are consumed — with one epoch of headroom this
is a pipeline depth of 1. Failure semantics: a Run must not report its
terminal `state_hash` until pending links are drained (the terminal
RunResponse already needs the chain value, which provides the sync
point). Snapshot/fork/verify paths must also drain first.

Risks: dirty-tracking completeness (existing risk R8 / `paranoid_hash`
audit mode covers verification — run soaks comparing shadow-hash chains
against in-place chains), memory cost (+128 MiB per slot), and the
copy cost at the boundary (bounded by dirty-page count; measure).

Sequencing: if M4 is built at all, build it after M3 stabilizes — M3
changes when a captured run's segment boundary is reached, which is where
M4's drain points live; landing M4 first risks redesigning the drain
logic twice.

### Option B (config change, operationally heavier): raise `epoch_len`

`epoch_len` is a `MachineConfig` preimage member — changing it changes
machine identity and requires regenerating the READY snapshot lineage
(`dh-m9-ready-handoff`) and re-blessing downstream refs. It also coarsens
replay/bisection granularity for exploration. Use only if Option A is
rejected; document the chosen value against M0 measurements.

### Non-options

Disabling `hash_final_stop`/`hash_epochs` for play sessions was
considered and rejected by the operator: the chain must stay
full-fidelity for every run.
