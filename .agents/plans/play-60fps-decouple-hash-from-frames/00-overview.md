# Play at 60fps: Decouple Frame Delivery From State-Hash Links

## Problem

Interactive play through rom-operator-bridge reaches a game point in ~240s
that a standalone SNES emulator (zsnes) reaches in ~20s — roughly 12x too
slow, i.e. ~5 fps instead of ~60 fps, ~200ms per rendered frame.

The target is real-time 60fps play with the state-hash chain kept at FULL
fidelity (no relaxation of the chain definition). The user-approved design
direction is: separate (a) frame generation, (b) the frame sent to the
operator, and (c) the full-memory blake3 hash link, so (c) stops gating (a)
and (b).

## Measured / Confirmed Root Causes

Ranked by expected impact:

1. **Debug builds in production.** The live `dh-workerd` and
   `snapstore-server` processes run from `target/debug/`. The ops runbook
   `docs/ops/rom-bridge-o73-ready-snapshot.md` (line ~151) launches the
   worker with `cargo run -p dh-worker --bin dh-workerd -- serve ...` —
   no `--release`. The workspace has no `[profile.dev]` opt-level override,
   so the entire host-side hot path (boundary engine, page walks, lz4, PNG
   inputs, gRPC) runs unoptimized.

2. **A full-guest-RAM blake3 hash link per displayed frame.** The bridge
   Play loop issues one `Run{frame_budget=1}` per frame. Every Run stop
   pushes a segment-final chain link (`RunOptions::hash_final_stop`
   defaults to `true`, `crates/dh-vmm/src/runctl.rs`), and
   `StateHashChain::push_final_link` (`crates/dh-vmm/src/hash.rs:130`)
   walks EVERY page of guest RAM (M9 machine: 128 MiB = 32,768 pages, each
   `read_slice`-copied then hashed). Measured on this host: single-threaded
   blake3 over 128 MiB is ~50ms at full release speed (`b3sum
   --num-threads 1`). At one link per frame this alone caps the system at
   ~15–20 fps in release, far worse in debug. `hash_epochs` defaults to
   `EpochsOn` (epoch links every 50M instructions) on top of that.

3. **Two serialized gRPC round-trips per frame from the bridge**
   (`Run{frame_budget=1}` then `GetFramebuffer`), plus a full framebuffer
   pixel copy and PNG encode per frame. The bridge code itself flags
   folding these into one captured Run as a known optimization
   (rom-operator-bridge `service/src/backend.rs` `play_step`).

## Why This Repo

Causes 1 and 2 live here. The already-specified-but-unimplemented
`RunWithFrameCapture` server-streaming RPC (API.md §2.7, proto
`proto/hypervisor.proto` line ~373, stub returning `unimplemented` at
`crates/dh-worker/src/service.rs:4709`) is the architecture's own answer
to the decoupling: one long Run streams a `CapturedFrame` per FRAME_MARK
while chain links happen only at epoch boundaries and the final stop, and
capture is normatively **capture-neutral** (must not perturb execution,
DHILOG, or the state hash). Backpressure holds the vCPU at the FRAME_MARK
boundary — which also gives the bridge real-time pacing for free.

A sibling plan in
`rom-operator-bridge/.agents/plans/play-60fps-streaming-frames/` covers
the bridge-side consumption. That plan's Milestone B1 (single captured Run
per frame) needs nothing from this repo; its Milestone B2 (streaming)
depends on Milestone M2 here.

## Milestones

- **M0 — Measure** (01): per-stage timing attribution so every later
  milestone has a before/after number. Includes instructions-per-frame for
  the reference workload (sizes how often epochs interleave with frames).
- **M1 — Release builds in ops** (01): fix the runbook, rebuild, restart,
  re-measure. Expected: large immediate win (likely 3–6x), still short of
  60fps because of cause 2.
- **M2 — Implement `RunWithFrameCapture`** (02): the spec'd streaming RPC.
  Hash links move from per-frame to per-epoch/per-stop. This is the
  60fps-enabling change.
- **M3 — Input at frame-hold** (03): let the operator inject pad input
  while the vCPU is held at a FRAME_MARK backpressure boundary, so live
  play gets ≤1-frame input latency without chopping runs into short
  segments (which would reintroduce per-stop hash links).
- **M4 — Epoch-hash latency mitigation, if M0 numbers demand it** (03):
  keep the chain byte-identical while getting the ~50ms full-memory walk
  off the frame-delivery critical path.

## Constraints

- The state-hash chain definition and values MUST NOT change: same
  preimage, same links, same values for the same (snapshot, inputs).
  Capture-neutrality is CI-enforced (02).
- ARCH rule: nothing depends on `dh-worker`.
- Privacy: no operator-private paths, snapshot refs, lease tokens, socket
  paths, or raw worker errors in committed files, bead notes, or PR bodies.
  Refer to the operator-private runtime root generically.
- Machine-config changes (e.g. `epoch_len`) change the machine identity
  hash and therefore require regenerating the READY snapshot lineage via
  `dh-m9-ready-handoff` — treat as a last resort (03).

## Tracking

Create beads per milestone before implementation (see 04 for suggested
titles and dependency edges).
