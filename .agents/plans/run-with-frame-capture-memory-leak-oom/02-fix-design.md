# 02 — Fix Design: Per-Hypothesis Shapes And The Determinism Constraint

Pick the branch matching 01's confirmed hypothesis. Whatever branch you
take, the two invariants from `00-overview.md` hold: the epoch hash
chain and sealed DHILOG format stay bit-identical, and the proof is a
pre-fix recording replayed on the post-fix build.

## Branch H1: Bisection Checkpoints Were Somehow Enabled (Unlikely — See 01)

01's sweep found the stock binary has no enablement path, so this
branch only fires if the deployed build turns out to be patched or a
future config surface appears. The hardening below is worth doing
regardless of which hypothesis confirmed, as follow-up if not now:

1. **Make enablement loud.** Log the effective
   `BisectionCheckpointConfig` at worker startup AND at each Run start
   when enabled (it multiplies per-Run memory churn and snapstore
   traffic by ~128 MiB/epoch). If deployment config work is needed,
   coordinate with ops: this repo stages the fixed release binary per
   `docs/ops/rom-bridge-o73-ready-snapshot.md`; the bridge owns the
   restart window (see `04-bridge-green-light-and-closeout.md`).
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
  check should pass trivially, but run it anyway: it DOES cover
  `device_sections`/vCPU-blob pooling (those are hash preimages).
  **It does NOT cover the framebuffer scratch** — fb bytes never enter
  any hash or DHILOG record (`fb_lz4` is host output only), so a
  stale-bytes bug in a pooled region-read or pixel buffer would
  silently corrupt frames delivered to the bridge while every
  determinism gate stays green. The frame-sink pooling change
  therefore needs its own content check: a test driving two
  consecutive frames with different content (and a short-then-long
  region sequence) asserting each decoded `fb_lz4` matches an
  independent framebuffer read of the same frame.

## Fixes For The C1/C2 Retainers (Cheap — Do Them In Any Branch)

01's sweep found two genuine grow-until-teardown buffers. Whether or
not they are the incident's driver, bound them while you're here:

- **C1:** apply the guest-event retention cap incrementally during the
  run (trim inside or alongside the `on_exit` accumulation at
  `service.rs:3646`) instead of only after the run returns
  (`service.rs:3836`). Check first whether anything consumes the full
  in-run event list at Run end (e.g. SDK-event responses) before
  trimming — preserve observable behavior; the cap semantics should
  match what post-run trimming produces today.
- **C2:** `DebugSerial.out` (`crates/dh-devices/src/serial.rs:39`) is
  never drained in production. Either bound it (ring buffer / cap with
  drop-oldest and a dropped-bytes counter) or actually wire the drain
  the module doc claims exists. If serial output feeds no consumer
  today, the bound is the safe minimal fix; file the follow-up bead
  for the doc-vs-reality discrepancy either way. Caution: DebugSerial
  is a device — confirm its `snapshot()` section (if any) does not
  include `out` before changing its shape, so device-section hash
  preimages are untouched.

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
- The pre-fix recording bit-identity replay. The recording comes from
  01's exit criterion (produced BEFORE the fix). Mechanism: the replay
  engine already verifies recorded EPOCH_HASH chain values during
  replay (`VerifyReplay` surface / `crates/dh-worker/src/verify_replay.rs`
  path used by `m5_record_replay.rs`-style tests) — replay the pre-fix
  sealed DHILOG on the post-fix build and require zero divergence; if
  no existing harness fits, write a small comparer that reads the
  sealed DHILOG's epoch-hash records and diffs them against the
  post-fix replay's chain values, and say so in the evidence dir.
- Post-fix profile rerun into the same evidence dir as 01's baseline.
