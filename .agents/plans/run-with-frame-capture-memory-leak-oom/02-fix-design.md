# 02 — Fix Design: Per-Hypothesis Shapes And The Determinism Constraint

Pick the branch matching 01's confirmed hypothesis. Whatever branch you
take, the two invariants from `00-overview.md` hold: the epoch hash
chain and sealed DHILOG format stay bit-identical, and the proof is a
pre-fix recording replayed on the post-fix build.

## Branch H1: Bisection Checkpoints Were Enabled On The Deployed Worker

Two sub-problems — fix both, not just the config:

1. **The config surprise.** If the deployment enabled `every_epoch()`
   (or an interval) without the operator intending it for play
   sessions, make enablement loud and legible: log the effective
   `BisectionCheckpointConfig` at worker startup AND at each Run start
   when enabled (it multiplies per-Run memory churn and snapstore
   traffic by ~128 MiB/epoch); consider a startup warning when
   `enabled` is combined with a streaming-scale default. Fix the
   deployment config itself in coordination with ops (the play-60fps
   measurements doc records the deployment; the bridge owns restarts).
2. **The mechanism should survive being enabled.** Even intentional
   bisection checkpointing must not OOM the host:
   - `read_pages` (`crates/dh-worker/src/snapshot_engine.rs:278-288`)
     allocates guest RAM as 32k separate 4 KiB `Vec<u8>`s per
     checkpoint — replace with one contiguous pooled buffer reused
     across checkpoints (one allocation per Run, not 32k per epoch),
     or stream pages into the snapstore put without materializing the
     full set. This kills the allocator-churn amplifier regardless of
     config.
   - Audit the snapstore client path (`put_snapshot_from_pages`,
     `snapshot_engine.rs:252` onward) for buffering/queueing: if puts
     are async or retried, verify backpressure exists — a slow
     snapstore must stall the Run (or fail loudly), never queue
     full-memory snapshots in worker RAM.
   - These changes are host-side memory management only; they must not
     change what bytes go into a checkpoint or the DHILOG
     `log_bisection_checkpoint` record (`service.rs:3710-3723`). The
     `o2d` bead (bisection checkpoint aux reseal) is adjacent — read it
     before touching this path so the two changes don't collide.

## Branch H2: Allocator Retention (No Code-Level Leak)

Fix the churn at its sources, prefer pooling over allocator tuning:

- **Frame sink** (`crates/dh-worker/src/service.rs:3736-3775`): reuse
  a per-Run scratch buffer for the framebuffer region read
  (`service.rs:3155`) and the pixel copy (`service.rs:3206`), and a
  reusable lz4 output buffer. The `CapturedFrame.fb_lz4` handed to the
  stream must remain an owned Vec (it crosses the channel), but its
  allocation is right-sized (~230 KB compressed), not the churny part.
- **Per-epoch Vecs**: `device_sections` (`hash.rs:356-366`) and the
  vCPU blob (`hash.rs:177`) allocate fresh per epoch — small, but if
  the profile fingers them, thread a reusable buffer through.
- **Allocator-level backstop**, only with profile evidence: either
  switch `dh-workerd` to jemalloc (a Cargo `#[global_allocator]`
  change — decide deliberately, it changes memory behavior globally
  and needs a soak before the green light), or call `malloc_trim(0)`
  at a bounded cadence (e.g. every N epochs) from the run loop's
  worker side. Prefer the pooled-buffer fixes; use the backstop only
  if pooling leaves residual growth.
- None of these touch hash preimages or DHILOG bytes — the bit-identity
  check should pass trivially, but run it anyway (buffer-reuse bugs
  that leak stale bytes into a hash are exactly what it would catch —
  e.g. a reused framebuffer scratch must be fully overwritten or
  length-clamped per frame).

## Branch H3: Something Else

No prescription — but the same discipline: smallest fix that bounds
RSS, hash/DHILOG bit-identity proven, regression guard (03) encodes the
new steady state.

## The `38b6` / play-60fps M4 Disposition (Required Output, All Branches)

Bead `38b6` defers the M4 shadow-copy + async-hash pipeline
(`.agents/plans/play-60fps-decouple-hash-from-frames/03-input-and-epoch-hash-decoupling.md`),
which priced "+128 MiB per slot" of deliberate memory cost to buy
latency. Given the static sweep, the likely truthful annotation is:
**the OOM fix and M4 are disjoint** — the leak is not the synchronous
hash path's memory (it retains nothing), so the fix neither absorbs nor
is absorbed by M4; M4 stays deferred on latency grounds and, when
built, must budget its +128 MiB/slot against the regression guard's
ceiling (03's `shadow_cost` term exists for exactly this). But write
the annotation from what 01 actually found, not from this prediction.
If H1 confirmed and the fix pools the checkpoint buffer, note on `38b6`
that M4's shadow copy should reuse the same pool discipline.

## Quality Gates For The Fix Commit

- Full workspace test suite, 3+ consecutive runs (hash-adjacent change
  — repo standing rule), merge gated on exit code (never `;`-chained).
- The pre-fix recording bit-identity replay (produce the recording
  BEFORE the fix lands — see `03-regression-guard.md`).
- Post-fix profile rerun into the same evidence dir as 01's baseline.
