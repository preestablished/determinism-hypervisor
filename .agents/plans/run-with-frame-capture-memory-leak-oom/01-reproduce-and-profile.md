# 01 — Reproduce And Profile: The Profile Is The Finding

A static sweep of the per-epoch/per-frame Run loop (2026-07-07, this
plan's prep, verified by an independent review pass) found **no code
path that retains a guest-memory-sized buffer per epoch when bisection
checkpoints are disabled** — but it DID find two smaller genuine
in-Run retainers (candidates C1/C2 below) and established that the one
exact-signature allocation is gated off. The fix is therefore NOT
"find the obvious Vec::push and drain it" — the observed 300–500 MB/s
must come from one of the ranked hypotheses below, and only measurement
can pick between them. Do the profiling first; do not write fix code
until one hypothesis is confirmed.

## Static-Sweep Results (Verified File:Line, 2026-07-07 @ main a7f2117+)

Exonerated by code reading — re-verify only if the profile contradicts:

- `crates/dh-vmm/src/hash.rs:130-156` `push_final_link`: full-memory
  walk per epoch through a single reused 4 KiB stack buffer. Heavy
  *read* (touches all guest RAM each epoch), zero heap retention.
- `crates/dh-vmm/src/dirty.rs`: `DirtyPageSet` dense bitmap ≤96 KiB;
  dirty ring fixed-size mmap. Bounded.
- DHILOG `LogWriter.buf` (`crates/dh-inputlog/src/dhilog.rs:124-125`,
  held on the rail `crates/dh-vmm/src/recording.rs:80`, sealed only at
  Run end): genuinely monotonic for the Run's duration, but every
  record kind is bounded (payload ≤4088; epoch-hash record 40 bytes; no
  record embeds page data) — tens of bytes/epoch, not 128 MiB/epoch.
- Frame stream: `FRAME_STREAM_CHANNEL_CAPACITY = 2`
  (`crates/dh-worker/src/service.rs:1491`), hook streams directly into
  the channel, no accumulating collection, no per-frame task spawn.
- `guest_mem.clone()` sites: `GuestMemoryMmap` handle clones, not deep
  copies; once per Run.
- Bisection checkpointing: the exact-signature allocation —
  `crates/dh-worker/src/snapshot_engine.rs:278-288` `read_pages`
  builds a full-guest-RAM page set (~128 MiB as 32,768 separate 4 KiB
  heap allocations) per checkpoint — is only reachable when
  `bisection_checkpoints.enabled` (dispatch `service.rs:3666`, config
  gate `service.rs:3527-3529`, default disabled `service.rs:219,236-237`).
  Moreover the stock binary has NO enablement path: `dh-workerd`'s
  `parse_args` exposes no bisection flag or env var, and
  `WorkerConfig::from_host_defaults()` hardcodes the disabled default —
  the only enablement sites are tests. Even when enabled, the page set
  is dropped synchronously after the blocking
  `put_snapshot_from_pages` call (`snapshot_engine.rs:249-252,322-342`;
  the snapstore client is a blocking call under a mutex, no async
  queue) — at most one checkpoint's pages are live at a time.

Genuine in-Run retainers the first sweep mis-classified — both grow
until Run teardown, neither plausibly reaches 300–500 MB/s alone, both
go on the profile's watch list:

- **C1 — `drained_guest_events` grows uncapped DURING a Run.** The
  retention cap (`append_guest_events_with_retention_cap`,
  `service.rs:2598-2651`, cap 1024) applies only AFTER the run returns
  (`service.rs:3836`); during the run every vCPU exit's drained events
  accumulate in `drained_guest_events.extend(events)`
  (`service.rs:3646`), each event carrying an owned payload Vec up to
  ~4 KiB. Rate-limited by vmexit pacing, but unbounded in time.
- **C2 — `DebugSerial.out` is never drained in production.**
  `crates/dh-devices/src/serial.rs:39` `out: Vec<u8>` grows on every
  guest THR write; `take_output()` has zero non-test callers despite
  the module doc claiming the run loop drains it. A fresh `DebugSerial`
  lives per Run and is freed only at teardown — a serial-chatty guest
  makes this a real monotonic retainer. (The doc-vs-reality discrepancy
  gets its own follow-up bead regardless of the OOM outcome.)

## Hypotheses, Ranked

**H2 — allocator retention of transient allocations (no code-level
leak).** glibc malloc keeps freed memory in per-thread arenas; a
multi-threaded tokio worker doing per-epoch/per-frame allocations can
grow RSS while "leaking" nothing. Known churn: the frame sink
(`service.rs:3736-3775`: framebuffer region Vec `service.rs:3155`,
pixel copy `service.rs:3206`, lz4 output `service.rs:3771` — the D7
frame is 229,376 bytes, so ~3 × 229 KB × 60 fps ≈ 40 MB/s) plus
per-epoch device-section and vCPU-blob Vecs. Caveat that keeps this
honest: identical-size alloc/free cycles normally *plateau* at a
high-water mark plus fragmentation — pure H2 explains a plateau better
than compounding growth, and 40 MB/s of churn does not directly explain
300–500 MB/s. Treat H2 as the likely *amplifier/mechanism* and expect
the profile to also identify a *driver* (cross-thread arena migration,
fragmentation from mixed sizes, or an H3 source).

**H3 — something the static sweep missed** (kernel-side growth such as
KVM dirty-ring/memslot behavior under the long-run pattern, a
dependency's internal buffer, tonic/h2 stream buffers, or C1/C2 above
at unexpectedly high guest event/serial rates). If the mechanism is
confirmed but the *rate* stays unexplained, this is where the hunt
widens — the finding must explain both the rate (~300–500 MB/s) and
the trigger (long Run vs. many short Runs).

**H1 — bisection checkpoints were somehow enabled on the deployed
worker.** The rate would match (128 MiB/epoch × ~3 epochs/s ≈
384 MB/s of *churn*), but two facts demote it: the stock binary cannot
enable it (see sweep above), and even enabled, the pages are dropped
per epoch — monotonic growth would additionally require retention
this sweep did not find (audit the snapstore client's internals if H1
somehow confirms). Check it anyway because it is cheap: confirm the
deployed build `4285b45` is stock (the deployment is recorded in
`.agents/plans/play-60fps-decouple-hash-from-frames/05-measurements.md`;
ops runbook `docs/ops/rom-bridge-o73-ready-snapshot.md`). Five
minutes, then move on.

**Routing rule:** confirmed-mechanism-but-unexplained-rate routes to
H3 widening BEFORE any fix; a compound finding (e.g. H2 mechanism with
a specific churn driver) takes the fix branch of the *driver* in
`02-fix-design.md`, plus the H2 pooling fixes.

## Repro Harness

1. **Workload:** prefer the staged M9 artifacts — see
   `docs/phase-2-exit-gate.md` for the `DH_M9_*` env-var invocation
   pattern and the beads memory notes for artifact staging
   (`~/.cache/dh-m9/reference-workload/` + the dist emulator image);
   the incident ran the real emulator. A synthetic guest (tight loop
   dirtying pages + FRAME_COUNTER increments) is acceptable IF it
   reproduces the growth — and is preferable for the 03 regression
   guard if it does.
2. **Drive:** one `RunWithFrameCapture` with a paced ~60 Hz consumer,
   like the bridge. Do not write a gRPC client from scratch — start
   from the existing drivers in `crates/dh-worker/tests/`
   (`frame_capture_stream.rs`, `play_perf_smoke.rs`). Budget: derive
   from measured MIPS × target seconds — at the incident's implied
   ~150 MIPS (epoch_len 50M × ~3 epochs/s), a 3-minute run needs
   ≥3 × 10^10 instructions; size to the profiling window, not a round
   number. Run BOTH configurations: bisection disabled (default) and
   `BisectionCheckpointConfig::every_epoch()` (test-only config, wire
   it through the in-process service) — the pair discriminates H1's
   mechanism from H2/H3 in one experiment.
3. **Also run a plain long `Run`** (no frame capture) at the same
   budget: it isolates the epoch path from the frame path.

## Instrumentation

- RSS over time: sample `/proc/<pid>/status` `VmRSS` (and `VmHWM`,
  `RssAnon`) every 1 s for the worker process; write CSV into the
  evidence dir.
- Allocator attribution, in escalating order of effort:
  `malloc_info(3)` / `mallinfo2` snapshots at epoch boundaries (cheap,
  distinguishes "arena holds freed memory" from "live allocation");
  then `heaptrack` or valgrind massif on a debug-friendly build if live
  allocation is growing (attributes the call stack).
- One targeted experiment for H2: at a fixed point mid-run, call
  `malloc_trim(0)` (behind a temporary debug hook) — if RSS collapses,
  the H2 *mechanism* is confirmed (apply the routing rule above for
  the rate question).
- Watch C1/C2 directly: log `drained_guest_events.len()` (and summed
  payload bytes) and the serial `out.len()` at epoch boundaries during
  the repro — two counters, cheap, definitive.
- Record everything in a timestamped `target/oom-profile-<date>/`
  evidence dir: configs, build SHA, CSVs, plots or summarized
  rates, and the confirmed hypothesis. This dir is AC1 evidence —
  before/after (post-fix rerun) both live here.

## Exit Criteria For This Step

- Growth reproduced (or a documented failure to reproduce with all
  hypotheses tested — in which case STOP and escalate: append a
  findings note to
  `.agents/requests/run-with-frame-capture-memory-leak-oom/` as the
  channel back to the bridge, update the bead, and do not fix blind).
- One hypothesis (or compound) confirmed with rate arithmetic that
  matches the incident (~300–500 MB/s at the incident's config, or the
  equivalent scaled rate for the repro's guest size/MIPS).
- **A pre-fix recording produced and stashed** (the repro run itself
  can produce it): the sealed DHILOG plus its epoch chain values, into
  the evidence dir. `02`/`03`'s cross-build bit-identity check depends
  on this existing BEFORE any fix lands — it cannot be produced
  retroactively.
- The internal bead (P1, filed before this work started per
  `00-overview.md`) updated with the confirmed root cause.
