# Phase 3 — A Workload in the Box (the game runs)

## Outcome

The demo workload — the deterministic emulator running an operator-supplied game
image — boots inside the deterministic VM under the guest agent's supervision. Its
emulated RAM and framebuffer are published as named memory regions the host reads
directly. A scripted input log plays the game's first room **in-VM**, and the full
determinism validation suite (double-run + snapshot/restore continuity) is green.
After this phase, the platform has something real to explore.

## Entry requirements

- Phase 2 exit gate (fork/replay proven, worker daemon live).
- reference-workload M2 done (emulator plays first room host-side) — carried in from
  Phases 1–2's parallel track.
- An operator-supplied, legally-obtained game image (the repo ships no game content).

## Work, by repo (ordered)

**`determinism-hypervisor` — pulled-forward dependency:**

1. M9 — minimal-Linux guest path (bzImage boot). **Scheduling note:** the
   hypervisor's own plan lists this last, but `reference-workload`'s image pipeline
   (pinned kernel + static-musl + deterministic cpio) and the agent-as-PID-1 design
   assume a Linux guest. The nanokernel guest was enough for Phases 1–2 gates; in-VM
   workload bring-up needs Linux now. Re-run the Phase 1 determinism gate and the
   Phase 2 fork gate against the Linux guest before building on it.
2. **Capture engine** (the C-requirements in the hypervisor's API doc): consume the
   guest-sdk region manifest at channel init; accept a compiled extraction list
   (region, layout_version, offset, len) on Run/TakeSnapshot; return packed
   `feature_bytes` + `fb_lz4`. *Joint with guest-sdk Ms4 — this is how the host reads
   RAM features without a feature-map dependency in the hypervisor.*

**`guest-sdk` — the in-guest chain:**

1. Milestone 3 — `detguest-sdk` end-to-end events from a real workload. *Depends on
   hypervisor M9 (Linux guest).*
2. Milestone 4 — **memory publication usable by the platform** ⭐ (mlock + pagemap
   GVA→GPA translation, seqlock manifest, kernel-config pinning: no compaction/
   migration/KSM/THP/swap). *Depends on Ms3.*
3. Milestone 5 — `inject_point` + input-log round trip + determinism proof (the
   bit-identical `determinism_replay` CI gate). *Depends on Ms4.*

**`reference-workload` — the workload chain:**

1. M3 — harness + protocol against a mock agent. *No platform dependency; start at
   phase open.*
2. M4 — guest image + real agent, in-VM bring-up. *Joint with guest-sdk Ms4; depends
   on hypervisor M9, guest-sdk Ms3, and its own M3.*
3. M5 — full determinism validation suite: boot→N frames with a fixed log twice →
   per-frame RAM+framebuffer hashes identical; snapshot mid-game → restore →
   continue → identical to uninterrupted run; 20× zero-flake. *Depends on M4.*

**`snapshot-store` — close the storage loop:**

1. M7 — GC: mark-and-sweep + subtree pruning end-to-end. Needed before exploration
   (Phase 5) can run long; this is the last quiet window to land it.

**Opportunistic parallel track (zero platform deps, keeps Phase 5 short):**

- `exploration-orchestrator` M1–M4 (pure core, fakes, scheduler, end-to-end runner
  on a synthetic grid-world). See Phase 5.

## Cross-repo ordering

```
hypervisor M9 (Linux guest) ──► guest-sdk Ms3 ──► Ms4 ⭐ ──► Ms5
                                          │         ║ joint
reference-workload M3 ────────────────────┴──► refwork M4 ──► refwork M5
snapshot-store M7 (GC)                                   (independent)
orchestrator M1–M4 on fakes                              (independent)
```

## Exit gate

1. reference-workload M5 green: the determinism suite passes 20 consecutive runs
   with zero flakes, including mid-game snapshot/restore continuity.
2. guest-sdk Ms4 acceptance: emulator RAM region readable from the host and stable
   across 100× snapshot/restore; Ms5 `determinism_replay` CI gate green.
3. MAP.md build-order step 2 milestone: a scripted input log plays the game's first
   room **in-VM**, driven entirely through the worker daemon's gRPC API
   (RestoreSnapshot → InjectInputs → Run → GetFramebuffer shows the room).
4. snapshot-store GC property tests green (safety: never collects pages reachable
   from a live manifest; completeness: collects everything else).

## Parallelism notes

The guest-sdk and reference-workload chains converge at the joint M4/Ms4 milestone —
coordinate those two closely (same agent or tight pairing). Hypervisor M9 is the
phase's entry bottleneck; start it first. Orchestrator-on-fakes is free parallelism
for whoever is idle.
