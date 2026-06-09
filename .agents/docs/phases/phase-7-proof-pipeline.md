# Phase 7 — Proof Pipeline (watchable, verifiable replays)

## Outcome

Any node in the exploration tree can be turned into (a) a machine-checked proof —
the assembled root→node segment chain re-executes bit-identically, segment by segment — and (b) human-watchable
artifacts: an MP4 with a speedrun-style input HUD, GIF clips, contact sheets, all
registered in the control-plane artifact registry and browsable in the observatory.
Divergences (should they ever occur) are automatically bisected to the exact input-log
offset. The Phase 5 first-boss trajectory becomes a video.

## Entry requirements

- Phase 2 exit gate (input-log format frozen, verification mode exists) — the hard
  dependency for M1–M2.
- Phase 6 control-plane M3 (job queue) for M5+; Phase 5's recorded tree for the
  integration gate.
- Spark NVENC preflight from Phase 0.

## Work, by repo (ordered)

**`replay-renderer` — the spine (M1–M3 can start during Phase 6):**

1. M1 — assembly library + `.dilog` v2 portable container (rules R1–R6: version and
   machine/clock uniformity, content-addressed ref adjacency — segment_i's base ref
   equals segment_{i−1}'s verified child ref — sealed-blob integrity, intra-segment
   monotonicity, digest availability).
2. M2 — verification + bisection against a **mock** hypervisor. *Depends on M1.*
3. M3 — frame pipeline + encoders on the Spark (NVENC primary, libx264 fallback,
   integer NN upscale, input-HUD overlay). *Parallel with M2 after M1.*
4. M4 — `reexec-agent` against the real hypervisor + snapshot-store on the Intel
   box. *Depends on M2 and the hypervisor work below.*
5. M5 — `replayd` job model + cross-host pipeline (LZ4-chunked frame stream Intel →
   Spark). *Depends on M3 + M4.*
6. M6 — platform integration (control-plane queue intake, artifact registration,
   observatory divergence events). *Depends on M5 + control-plane M3/M5.*

**`determinism-hypervisor` — the H-requirements (blocking replay M4):**

1. M8 — verification-mode hardening + bisection support.
2. Implement replay-renderer's documented behavioral requirements H1–H8:
   `RunWithFrameCapture` streaming RPC, state-digest-at-pause, exact-virtual-time
   stop, capture-neutrality (capturing frames must not perturb execution). These sit
   outside the hypervisor's original M-list — schedule them explicitly here.

**`control-plane` — render path:**

1. M5 — render pipeline + artifacts end-to-end (SubmitRenderJob proxy, artifact
   registry, ranged HTTP download). *Depends on its Phase 6 M3.*

**`observatory` — close the loop for humans:**

1. M7 — replay browser + proxy actions (inline playback, timeline-JSON scrubber
   annotations). *Depends on replay-renderer M6 + control-plane M5.*

## Cross-repo ordering

```
replay M1 ─► M2 ──────────► M4 ─► M5 ─► M6 ─► observatory M7
        └──► M3 ───────────────────┘      ▲
hypervisor M8 + H1–H8 ────► (blocks M4)   │
control-plane M5 ─────────────────────────┘
```

## Exit gate

1. **Verified replay of the Phase 5 trajectory:** the first-boss node renders to an
   MP4 with input HUD; its re-execution digest matches the recorded hash; the `.dilog`
   + image re-verify from scratch on a clean checkout (the third-party
   re-verification drill).
2. **Injected-divergence test:** corrupt one event in a copy of a log; the bisect job
   reports exactly that offset within its run budget.
3. Capture-neutrality proven: a run with frame capture on and off produces identical
   state digests at every checkpoint.
4. Artifacts (mp4, `.dilog`, timeline JSON, contact sheet) registered with checksums
   in control-plane and playable/downloadable through the observatory replay browser.

## Parallelism notes

Three tracks: replay-renderer (spine), hypervisor H-requirements (do these first —
they gate the spine's M4), control-plane M5. Observatory M7 is the convergence point
at the end. M1–M3 of the spine need nothing from Phases 5–6 and should have started
during Phase 6 idle time.
