# Request: Fix The Per-Run Memory Leak, Then Prove The Capture Engine On Real Data

## Who Is Asking

The phases track, round 2 (2026-07-07). Two predecessors in this repo
stand unexecuted — the round-1 request
(`phase3-frame-cap-retune-and-run-wallclock-backstop/`) and the bridge's
incident filing (`run-with-frame-capture-memory-leak-oom/`, only a
`00-overview.md`, no bead, no acceptance criteria). This request adopts
the OOM incident into an executable shape and names the repo's Phase-4
deliverable behind it. It does not touch round-1's scope (frame caps,
`linux_m5`, guest-sdk handoff, wall-clock backstop) — that stays round-1's.

## Standing Relative To Round 1 — Read This First

Round-1 (`phase3-frame-cap-retune-and-run-wallclock-backstop/`) is also
unexecuted, and the two do not block each other. Ordering guidance so
nobody has to guess: **if one agent holds both, do round-1 item 3 first
or in the same session** — the guest-sdk handoff is mostly verification,
and it unblocks another repo's two P0 beads (Phase 3 exit gate 2) that
have waited since 2026-06-18 — then this request's items 1–4; the OOM is
the worse defect but it is *contained* in production (the bridge's
segment clamp), not bleeding. If two agents, parallelize freely — the
scopes are disjoint.

## Why determinism-hypervisor, Why Now

1. **The leak is the repo's only live production defect.** First live
   streaming Play session (2026-07-07 ~03:29Z): `dh-workerd` grew
   ~300–500 MB/s to ~26 GB anon RSS inside one long `RunWithFrameCapture`
   and was OOM-killed (taking an unrelated pod and snapstore with it).
   The stream channel is exonerated (backpressured, capacity 2); the
   shape fits a **full-guest-memory-sized buffer (~128 MiB) retained per
   epoch, freed only at Run teardown** — invisible in the old
   `Run{frame_budget=1}` era. The bridge contained it by clamping
   segments to ~200M instructions (their `fbd38d1`, ~50 ms stall per
   segment boundary) and is holding bead `9bx` for your green light.
   Notably, the incident is currently **untracked in this repo's own
   beads** — the incoming request dir is its only record.
2. **The likely fix is work you already planned.** The incident filing
   itself flags it: this is the deferred play-60fps M4 (bead `38b6`,
   epoch-hash shadow/async pipeline,
   `.agents/plans/play-60fps-decouple-hash-from-frames/03-input-and-epoch-hash-decoupling.md`
   — which called out the "+128 MiB per slot" memory-cost risk) **seen
   from the memory side instead of the latency side**. Candidate
   retainers: the `run_segment_with_epochs` → `push_final_link` path
   (`crates/dh-vmm/src/runctl.rs:317`, `hash.rs:130`) and the
   whole-Run recording buffer (`crates/dh-vmm/src/recording.rs`);
   the dirty-tracking structures are bounded and probably innocent.
3. **Phase 4's entry runs through your capture engine.** The entry gate
   needs "real RAM/framebuffer captures from the in-VM emulator" — the
   mechanism is your Phase-3 capture engine (`CaptureSpec`/`ExtractRange`
   → packed `feature_bytes` + `fb_lz4`; proto and `dh-worker` service
   surface exist). It has never been exercised end-to-end against a real
   workload image. reference-workload's round-2 corpus request produces
   and packages the corpus; **you prove the engine that takes the
   captures**. The two items travel together because a capture session
   benefits directly from the fix and must not become the second OOM
   incident — though bounded-segment runs (the bridge's containment
   pattern) remain a workable interim for anyone capturing before the
   fix deploys.

## The Ask In One Paragraph

File the internal bead and fix the leak — profile one long
`RunWithFrameCapture` for monotonic RSS, free the per-epoch retainers
incrementally, add a bounded-RSS regression guard over a multi-minute
streaming Run, resolve `38b6`'s relationship to the fix (absorbed,
partially absorbed, or still deferred — say which), and hand the bridge
its segment-budget green light with a number; then, once
reference-workload's regenerated image exists, run the capture engine
end-to-end against it — a compiled extraction list over the real region
manifest returning packed `feature_bytes` cross-checked against
`detguest-host` reads, plus `fb_lz4` frames — and record the sample
evidence the scorer-corpus work will consume; close both this request
and the bridge's OOM request dir with resolutions.

## Files In This Request

| File | Contents |
|---|---|
| `01-current-state.md` | Evidence: the incident, the suspect code paths, capture-engine inventory |
| `02-requested-work.md` | The ask, sequencing, acceptance criteria, out of scope |
| `03-verification-offer.md` | Bridge + refwork verification choreography |
