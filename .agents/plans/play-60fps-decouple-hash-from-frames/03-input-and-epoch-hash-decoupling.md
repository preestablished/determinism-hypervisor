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
  frame index are merged into the active run's scheduled-frame-input set
  (the vmm segment driver already takes `scheduled_frame_inputs` —
  `run_segment_with_scheduled_inputs_frames_and_epochs`);
- delivery happens exactly as if the events had been scheduled before the
  run started: same injection windows (§3.4), same DHILOG records —
  replay of the DHILOG reproduces the run bit-for-bit. Determinism is
  preserved because inputs are *recorded*, not because they are
  pre-declared.

Concurrency shape in the worker: the slot runtime thread owns the run;
`InjectInputs` currently rejects a running slot. Add a small mailbox the
frame callback drains at each FRAME_MARK hold (the only place guest state
is at rest), so the gRPC handler never touches the vmm concurrently.

### API/spec impact

API.md amendment: define `InjectInputs` semantics for a slot in a
streaming-run frame-hold (accepted; applied from the next frame boundary;
rejected with FAILED_PRECONDITION if the target frame is not in the
future). CI determinism test: a run with live-injected inputs replays
identically from its DHILOG.

### Latency accounting

Input arrives → applied at the next FRAME_MARK hold → visible ≤2 frames
(~33ms) later. Comparable to standalone emulator + display latency.

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
