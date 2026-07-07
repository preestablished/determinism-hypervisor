# Plan: Fix The Per-Run Memory Accumulation Behind The RunWithFrameCapture OOM

Plan for `.agents/requests/run-with-frame-capture-memory-leak-oom/`
(the bridge's incident filing, 2026-07-07) as adopted into executable
shape by `.agents/requests/phase4-oom-fix-and-capture-engine-proving/`
items 1–4. Capture-engine proving (phase4 item 5) is **out of scope
here** — it has its own entry condition (`refwork-gp9`) and belongs to
the phase4 request; this plan covers only the leak.

## The Incident In One Paragraph

First live streaming Play session (2026-07-07 ~03:29Z): one long
`RunWithFrameCapture` (large icount budget, bridge pacing ~60 Hz) grew
`dh-workerd` anon RSS at ~300–500 MB/s to ~26 GB and the kernel OOM
killer fired from a `dh-slot-0` thread, taking an unrelated k8s pod
first. The frame-stream channel is exonerated (capacity-2 backpressure,
`FRAME_STREAM_CHANNEL_CAPACITY` in `crates/dh-worker/src/service.rs:1491`;
~14 MB/s of frames over ~75 s ≈ 1 GB, nowhere near 26 GB). The signature
fits something retained per epoch/exit inside one long Run and freed only
at Run teardown — invisible in the old `Run{frame_budget=1}` era, where
every Run ended after ~1 frame. Production containment: the bridge clamps
segments to ~200M instructions and reopens the stream (`fbd38d1`), at a
cost of one ~50 ms hash-link stall per boundary; bridge bead `9bx` waits
on our green light to raise it.

## Ground Rules For The Implementer

1. **The profile is the finding.** `01-reproduce-and-profile.md` carries a
   ranked candidate list from static analysis, but do not fix anything
   until a repro shows RSS growth and instrumentation attributes it.
   Several previously-named suspects are already exonerated by reading the
   code (see 01); the remaining candidates have per-item rates that only a
   measurement can confirm.
2. **The hash chain is untouchable.** Any fix must leave the epoch hash
   chain and the sealed DHILOG format bit-identical for the same
   execution. The proof obligation is explicit: replay one pre-fix
   recording on the post-fix build and show the chain values match the
   recorded chain (within-build record/replay gates would miss a
   format/value change). If a fix *requires* a format change, that is a
   declared, versioned break with a migration story — stop and surface it,
   never land it silently.
3. **Track it in beads.** The internal bead is FILED:
   `determinism-hypervisor-9f3x` (P1, links the bridge request dir and
   bridge beads `l1w`/`9bx`). Claim it (`bd update determinism-hypervisor-9f3x --claim`)
   before writing code and keep root-cause/evidence notes on it. The request dir
   is not a tracker. Also annotate bead `38b6` (deferred epoch-hash M4
   pipeline) with the fix's relationship to that design — absorbed,
   partially absorbed, or untouched (see `02-fix-design.md`).
4. **Determinism verification discipline** (standing repo lesson): never
   chain `cargo test ; git merge` — gate merges on test exit codes; and
   hash-sensitive changes get 3+ consecutive full workspace runs, not one.

## Files In This Plan

| File | Contents |
|---|---|
| `01-reproduce-and-profile.md` | Repro harness, instrumentation, ranked candidate retainers with code refs |
| `02-fix-design.md` | Fix shapes per candidate, determinism constraints, `38b6`/M4 disposition |
| `03-regression-guard.md` | Bounded-RSS ceiling + plateau test, bound derivation, CI/lab-lane placement |
| `04-bridge-green-light-and-closeout.md` | `9bx` answer with a number, request-dir resolutions, deploy coordination |

## Suggested Sequencing

01 → 02 → 03 → 04 strictly. The bridge is contained in production, so
correctness beats speed; but do not start 03's bound derivation until 02
has landed (the bound depends on what steady-state RSS looks like
post-fix).

## Acceptance Criteria (Mirrors phase4 AC1–AC3, AC5)

1. Bead filed; RSS-over-time profile evidence, before and after the fix,
   in a timestamped `target/` evidence dir (path recorded on the bead).
2. RSS bounded across a multi-minute streaming Run: regression guard
   (ceiling AND plateau) landed in CI or a documented lab lane, with the
   bound derivation and its input sources written down.
3. Pre-fix-recording hash-chain bit-identity check passed on the post-fix
   build (or a declared, versioned break with migration story — expected
   outcome is "passed").
4. Bridge request dir resolved with fix evidence and a concrete `9bx`
   answer: the safe segment budget (a number or "unbounded") and the
   deployed-worker build carrying the fix.
5. `38b6` annotated with the fix's relationship to the M4 design.
