# Requested Work

## What We Need (Behavioral)

1. **Track it.** File the internal bead for the OOM (P0/P1 — it is the
   repo's only live production defect), linking the bridge's request dir
   and `l1w`/`9bx`. An inbound request dir is not a tracker.
2. **Profile, then fix.** Reproduce the growth on a long
   `RunWithFrameCapture` (synthetic workload is fine), identify the
   per-epoch retainer(s) empirically — don't trust `01-`'s suspects
   beyond using them as starting points — and free them incrementally
   so RSS is bounded across a Run of arbitrary length. Record on bead
   `38b6` whether the fix absorbs, partially absorbs, or leaves intact
   the deferred epoch-hash M4 design, so that plan's status stays true.
3. **Regression guard — ceiling AND plateau.** A CI-runnable (or
   lab-lane) test driving a multi-minute streaming Run that fails if
   (a) worker RSS exceeds a stated bound (derived mechanically from the
   configured slot count × guest size + margin — write the derivation
   and its input sources down), **or** (b) RSS fails to plateau: RSS in
   the final third of the run exceeds the one-minute-mark RSS by more
   than a stated small percentage. The ceiling catches the loud leak;
   the plateau catches the 1–5 MB/s leak that would survive a generous
   ceiling yet kill a 4-hour Phase-5 soak.
   Determinism constraint, stated strongly because this touches the
   hash path: **replay one pre-fix recording on the post-fix build and
   show the epoch hash chain is bit-identical to the recorded chain**
   — record/replay gates alone verify within-build determinism and
   would miss a hash-value or recording-format change. If the fix
   *does* change chain values or the sealed format, that is a declared,
   versioned break with a migration story for existing snapstore/bridge
   artifacts — never a silent one.
4. **Green-light the bridge.** Resolve their
   `run-with-frame-capture-memory-leak-oom/` dir with the fix evidence
   and a concrete answer for `9bx`: the segment budget they can safely
   run (or "unbounded"), and the deployed-worker build that carries the
   fix. Coordinate the redeploy window with them (they own the
   restart procedure).
5. **Capture engine, proven on real data** *(entry condition:
   reference-workload's regenerated image exists — their `refwork-gp9`;
   if it slips, deliver items 1–4 and resolve this item as
   explicitly-waiting)*. Against the real image: compile an extraction
   list from the demo feature map over the real region manifest and
   verify **on both capture surfaces** (`Run`-with-capture and
   `TakeSnapshot`-with-capture — distinct paths in
   `dh-worker/src/service.rs`; if only one is proven, say which and
   why): (a) returned `feature_bytes` match independent
   `detguest-host` reads of the same (region, offset, len) ranges
   bit-for-bit, (b) `fb_lz4` decodes to the D7 229,376-byte frame,
   (c) the same capture spec over a restored/forked child returns
   identical bytes for unchanged state, and (d) **the negative case**:
   a mismatched `layout_version` in the extraction list is rejected,
   proven version recorded — the guard protecting scorer M1 from
   decoding a stale layout. While there, record per-capture cost
   (spec-compile + extract + pack) — not an acceptance bar, but scorer
   M4's 1.5 ms p50 budget sits downstream; one number now prevents a
   Phase-4 surprise. Record a small durable sample set (spec, bytes
   hashes, revs) — the interface evidence refwork's corpus request and
   scorer M1 build on. Division of labor: **you prove the engine;
   refwork produces and packages the corpus (including their
   exporter)** — don't build corpus tooling here. Capture under a
   *concurrent* `RunWithFrameCapture` stream is explicitly out of
   scope — unproven; nobody should assume otherwise.

## Suggested Sequencing (Yours To Overrule)

1 → 2 → 3 → 4 strictly (the bridge is running clamped in production);
5 when the image lands. If refwork's capture/corpus session starts
before the fix deploys to the lab worker, they must use
segment-bounded Runs (the bridge's `fbd38d1` pattern) — flag it to
them rather than letting a corpus run become incident #2.

## Acceptance Criteria

(ACs map to work items: AC1↔items 1–2, AC2↔item 3, AC3↔item 4,
AC4↔item 5, AC5↔item 2's `38b6` disposition.)

1. Bead filed; profile evidence (RSS-over-time before/after) in a
   timestamped `target/` evidence dir.
2. Regression guard (ceiling + plateau) in CI or a documented lab lane,
   with the bound derivation and input sources; record/replay gates
   green at the fix commit **and** the pre-fix-recording hash-chain
   bit-identity check passed (or the declared, versioned format break
   with migration story).
3. Bridge request dir resolved; `9bx` answered with a number and a
   build; bridge confirms (their `l1w` close or a handback note).
4. Item 5 either: sample capture evidence recorded (spec + hash table +
   cross-check log, revs of image/manifest/worker) — or an explicit
   waiting-on-`refwork-gp9` note in the resolution.
5. `38b6` annotated with the fix's relationship to the M4 design.

## Out Of Scope For This Request

- Round-1's items (frame caps, `linux_m5`, guest-sdk handoff, wall-clock
  backstop) — still owed under that request; this one neither supersedes
  nor blocks it.
- 60fps / the full M4 latency pipeline — only the *memory* half is
  demanded here; latency stays measured-and-deferred unless the fix
  happens to deliver it.
- Corpus packaging/labeling (reference-workload's round-2 request) and
  scorer-side consumption (future state-scorer repo).
