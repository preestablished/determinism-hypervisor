# 01 — Reproduce And Profile: The Profile Is The Finding

A static sweep of the per-epoch/per-frame Run loop (2026-07-07, this
plan's prep) found **no code path that retains a guest-memory-sized
buffer per epoch when bisection checkpoints are disabled**. That
negative result is load-bearing: it means the fix is NOT "find the
obvious Vec::push and drain it" — the observed 300–500 MB/s must come
from one of the ranked hypotheses below, and only measurement can pick
between them. Do the profiling first; do not write fix code until one
hypothesis is confirmed.

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
  (`crates/dh-worker/src/service.rs:1491`), hook streams directly, no
  accumulating collection. Guest events capped by
  `append_guest_events_with_retention_cap` (`service.rs:2598-2651`).
- `guest_mem.clone()` sites: `GuestMemoryMmap` handle clones, not deep
  copies; once per Run.

The one exact-signature allocation:

- `crates/dh-worker/src/snapshot_engine.rs:278-288` `read_pages`:
  allocates a full-guest-RAM `Vec<(u64, Vec<u8>)>` (~128 MiB for a
  128 MiB guest, as 32,768 separate 4 KiB heap allocations) per
  bisection checkpoint, then ships to snapstore and drops. Gated: the
  `epoch_sink` dispatch (`service.rs:3666`) only reaches it when
  `bisection_checkpoints.enabled` (`service.rs:3527-3529`; default
  disabled, `service.rs:219,236-237`).

## Hypotheses, Ranked

**H1 — bisection checkpoints were actually enabled on the deployed
worker.** 128 MiB/epoch × ~3 epochs/s ≈ 384 MB/s — an exact match to
the observed 300–500 MB/s, and the OOM collateral included snapstore
(consistent with per-epoch snapshot uploads hammering it). The default
is disabled, but the *deployed* worker (`4285b45` release build, ops
deployment recorded in the play-60fps measurements) may set it — check
the deployment's `WorkerConfig` construction, flags/env, and the ops
runbook config before anything else. Even if enabled, the pages Vec is
dropped after `put_snapshot_from_pages` (`snapshot_engine.rs:252`)
worker-side — so H1 alone predicts massive allocator churn (32k × 4 KiB
allocs per epoch), which glibc plausibly never returns to the OS
(→ compounds with H2), and/or retention inside the snapstore client
(audit its buffering/queueing if H1 confirms).

**H2 — allocator retention of transient allocations (no code-level
leak).** glibc malloc keeps freed memory in per-thread arenas; a
multi-threaded tokio worker doing large (>128 KiB → mmap'd, but
144-byte-to-4 KiB → arena'd) allocations per epoch/frame can grow RSS
monotonically while "leaking" nothing. Per-frame churn from the frame
sink (`service.rs:3736-3775`: framebuffer region Vec `service.rs:3155`,
pixel copy `service.rs:3206`, lz4 output `service.rs:3771` — the D7
frame is 229,376 bytes, so ~3 × 229 KB × 60 fps ≈ 40 MB/s of churn)
plus per-epoch device-section Vecs. 40 MB/s of churn does not directly
explain 300–500 MB/s of growth, so H2 *alone* is an incomplete account
— treat it as the amplifier and H1 (or H3) as the driver.

**H3 — something the static sweep missed** (kernel-side growth such as
KVM dirty-ring/memslot behavior under the long-run pattern, a
dependency's internal buffer, tonic/h2 stream buffers). If H1 and H2
both fail to reproduce the signature, widen the profile: the finding
must explain the *rate* (~300–500 MB/s) and the *trigger* (long Run vs.
many short Runs).

## Repro Harness

1. **Workload:** prefer the staged M9 artifacts
   (`~/.cache/dh-m9/reference-workload/` + the dist emulator image per
   the repo's staging notes) since the incident ran the real emulator;
   a synthetic guest (tight loop dirtying pages + FRAME_COUNTER
   increments via pv-pad/detchannel) is acceptable IF it reproduces the
   growth — and is preferable for the 03 regression guard if it does.
2. **Drive:** one `RunWithFrameCapture` with a large icount budget
   (≥6 × 10^9 instructions ≈ minutes of run at plausible MIPS),
   consuming frames at a paced ~60 Hz like the bridge does. Run BOTH
   configurations: bisection disabled (default) and
   `BisectionCheckpointConfig::every_epoch()` — the pair
   discriminates H1 from H2/H3 in one experiment.
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
  H2 confirmed as the retention mechanism.
- Record everything in a timestamped `target/oom-profile-<date>/`
  evidence dir: configs, build SHA, CSVs, plots or summarized
  rates, and the confirmed hypothesis. This dir is AC1 evidence —
  before/after (post-fix rerun) both live here.

## Exit Criteria For This Step

- Growth reproduced (or a documented failure to reproduce with all
  three hypotheses tested — in which case STOP and take the findings
  back to the bridge; do not fix blind).
- One hypothesis confirmed with rate arithmetic that matches the
  incident (~300–500 MB/s at the incident's config, or the equivalent
  scaled rate for the repro's guest size/MIPS).
- The internal bead (P1, filed before this work started per
  `00-overview.md`) updated with the confirmed root cause.
