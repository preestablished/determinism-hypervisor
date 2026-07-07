# 04 — Green-Light The Bridge, Resolve The Request Dirs, Close Out

Do this last, with 01–03's evidence in hand.

## 1. Answer Bridge Bead `9bx` With A Number

The bridge clamps streaming segments to ~200M instructions
(`PLAY_STREAM_SEGMENT_ICOUNT_BUDGET`, their `fbd38d1`) and pays a ~50 ms
hash-link stall per segment reopen. They want the budget raised to
seconds-to-minutes of play. The answer they need has two parts:

- **The number.** After the fix, if RSS is genuinely bounded per
  Run (the regression guard's plateau assertion passing over a
  multi-minute run is the evidence), the memory-safe answer is
  "unbounded — memory no longer scales with segment length." If any
  bounded-but-nonzero per-epoch cost remains (e.g. DHILOG record bytes
  per epoch — the sealed log grows with run length by design), state the
  per-minute growth rate measured in 03 and derive the budget from the
  worker host's headroom instead of calling it unbounded. Give one
  number, its derivation, and what it assumes (slot count, guest size,
  host RAM).
- **The build.** The exact worker commit/build carrying the fix, and
  confirmation it is deployed (or the deploy is scheduled). The bridge
  owns the restart procedure (`rom-bridge-o73` runbook) — coordinate the
  window with them; do not restart their worker unilaterally.

Note the caveat that stands regardless: the sealed-DHILOG size and
seal/teardown cost still scale with segment length even after the RSS
fix, so "unbounded" (if that's the answer) means "memory-safe", not
"free". Say so explicitly so the bridge sizes segments on latency/replay
granularity grounds, not stale OOM fear.

## 2. Resolve The Request Dirs

- `.agents/requests/run-with-frame-capture-memory-leak-oom/`: add a
  numbered resolution file (`01-resolution.md` — the repo convention is
  numbered `NN-resolution.md` files inside the request dir, see
  `phase3-snapshot-restore-no-frame-under-no-tick/08-resolution.md` and
  `rom-bridge-getframebuffer-region-contract/05-resolution.md`)
  recording: root cause
  (the retainer 01 actually found, with file:line), the fix commit, the
  before/after profile evidence path, the regression guard's location
  and invocation, and the `9bx` answer.
- `.agents/requests/phase4-oom-fix-and-capture-engine-proving/`: resolve
  items 1–4 the same way. Item 5 (capture-engine proving) is NOT this
  plan's scope — resolve it as explicitly-waiting-on-`refwork-gp9` if it
  hasn't been picked up separately, per that request's own instruction.

## 3. Beads Bookkeeping

- Close the OOM bead filed in 01 with the resolution summary.
- Annotate `38b6` with 02's disposition (absorbed / partially absorbed /
  untouched) — one paragraph, on the bead, so the play-60fps plan's
  status stays true.
- If 01's profiling surfaced additional bounded-but-real growth items
  that were deliberately not fixed (e.g. sealed-log growth), file
  follow-up beads (P3/P4) rather than leaving them as lore.

## 4. Session Close

Standard repo protocol: quality gates (full workspace test runs — 3+
consecutive if the fix touched the hash path), `git pull --rebase`
BEFORE any merge (never after — standing Ralph lesson), `bd dolt push`,
`git push`, verify `git status` clean and up to date.
