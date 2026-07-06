# Tests, Acceptance, Tracking, Closeout

## End-to-end acceptance (the number that matters)

The operator scenario from the bug report: load the SNES ROM through the
bridge, click Start then Play, and reach the same game point that takes
~20s in zsnes.

- After M1 (release builds): document the new wall time (expect a
  several-fold improvement; not yet 20s).
- After M2+M3 + bridge plan B2: **wall time ≈ 20s (real-time), sustained
  ~60fps in `/ws/frames`, input-to-effect latency ≤ ~2 frames.**
- Chain fidelity: unchanged hash semantics — verified by the
  capture-neutrality CI test (02) and a replay-verification pass over a
  played session's DHILOG.

## Determinism gates (must stay green throughout)

- Existing Phase-1/2 gates: `dh-cli gate`, run-twice-compare, replay
  suites — untouched behavior for the non-streaming paths.
- New (02): capture-neutrality — streaming vs plain run identical chains.
- New (03/M3): live-injected inputs replay bit-for-bit from DHILOG.
- New (03/M4, if built): soak comparing shadow-hash chains against
  in-place chains (`paranoid_hash`-style audit), plus dirty-tracking
  completeness soak (risk R8).

## Perf regression guard

Add an `#[ignore]`d perf smoke (the M0 harness) with a generous threshold
(e.g. ≥45 fps streamed on release builds on the reference host) so a
future change that reintroduces per-frame hashing or debug-path work is
caught before an operator feels it.

## Suggested beads

Create with `bd create` before implementation; dependency edges child ←
parent:

1. `Measure per-frame time attribution for bridge play path` (M0,
   analysis, p0)
2. `Fix ops runbooks to launch dh-workerd/snapstore release builds` (M1,
   impl, p0) — depends on nothing; do immediately
3. `Add build-profile visibility to worker startup/GetWorkerInfo` (M1,
   impl, p2)
4. `Implement RunWithFrameCapture streaming RPC per API.md §2.7` (M2,
   impl, p0) ← depends on 1
5. `Capture-neutrality CI test for RunWithFrameCapture` (M2, testing, p0)
   ← 4
6. `Accept InjectInputs at streaming-run frame-holds` (M3, impl, p1) ← 4
7. `API.md amendment: frame-hold input semantics` (M3, docs, p1) ← 6
8. `Epoch-hash shadow/async pipeline (contingent on M0 data)` (M4, impl,
   p2) ← 1, 4

## Privacy closeout

Before any handoff/PR: run the redaction gate with the operator-private
forbid file (per rom-operator-bridge `docs/redaction.md`). This plan
directory intentionally names no private paths, refs, tokens, or socket
locations; keep it that way in beads and commit messages. Measurement
tables in `05-measurements.md` must contain timings only — no capture
ids, no snapshot refs, no runtime roots.

## Coordination

The bridge-side consumer work is planned in
`rom-operator-bridge/.agents/plans/play-60fps-streaming-frames/`. Its B1
milestone (single captured Run per frame) can land before anything here
except M1; its B2 milestone consumes this plan's M2/M3 RPCs.
