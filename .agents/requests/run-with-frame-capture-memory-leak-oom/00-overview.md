# Request: dh-workerd Leaks Memory For The Duration Of A Single Run (OOM Under Streaming Play)

Filed by rom-operator-bridge, 2026-07-07, after the first live exercise of
the streaming Play stack (bridge `fb2a7fc` consuming `RunWithFrameCapture`
from the worker deployment recorded at `4285b45`).

## What Happened

- ~03:03 UTC: operator worker started (release build, preflight OK).
- ~03:27–03:28 UTC: the bridge opened one `RunWithFrameCapture` with a
  large icount budget and consumed frames at a paced ~60 Hz.
- 03:29:02 UTC: the kernel OOM killer fired, invoked from a `dh-slot-0`
  thread. The oom-kill task dump shows `dh-workerd` at ~6.84M resident
  pages (~26 GB anon RSS) on a host where the guest VM is configured with
  a small fraction of that. Collateral: the kernel first killed an
  unrelated k8s pod before the worker itself died.
- Everything the worker's own log captured is startup + "serving"; there
  was no panic — the process was killed.

## Why We Think It Is Per-Run Accumulation, Not The Stream Channel

- The frame stream path is correctly backpressured (`FRAME_STREAM_CHANNEL_CAPACITY = 2`,
  `try_send` + hold guard + 30s stall watchdog + cancel-on-Closed), so
  frames do not queue unbounded worker-side.
- The bridge read at ~60 fps (~230 KB lz4/frame ≈ 14 MB/s) — three orders
  of magnitude below the observed growth (~26 GB over ≲60–90 s of Run
  wall time ≈ 300–500 MB/s).
- The growth rate is consistent with retaining a full-guest-memory-sized
  buffer per epoch inside one long Run (128 MiB × ~3 epochs/s at a
  plausible effective guest MIPS). Bisection checkpoints were disabled
  (default), so the prime suspect is per-epoch hash-link/dirty-tracking
  buffers that are only freed at Run teardown.
- Under the old per-frame `Run{frame_budget=1}` loop each Run ended after
  ~1 frame, so whatever accumulates per-Run never had time to grow — this
  reproduces specifically under a long streaming Run.

## The Ask

1. Profile one long `RunWithFrameCapture` (or plain long `Run`) for
   monotonic RSS growth; identify what is retained per epoch/frame until
   Run end and free it incrementally.
2. Add a regression guard: RSS (or an internal buffer-count metric) must
   stay bounded across a multi-minute streaming Run.
3. Tell the bridge when it is safe to raise its segment budget: the bridge
   now deliberately bounds each streaming segment to ~200M instructions
   (~4 default epochs) and reopens the stream on BUDGET_REACHED as an OOM
   containment measure (bridge commit `fbd38d1`). That costs a hash-link
   stall (~50 ms) every segment; we want to raise the budget back to
   seconds-to-minutes of play once the leak is fixed.

## Related

- Bridge-side incident hardening: rom-operator-bridge `fbd38d1`
  (segment-bounded streams + reopen, stop-path fixes the incident exposed).
- Your plan `.agents/plans/play-60fps-decouple-hash-from-frames/` — if the
  per-epoch hash link is the retainer, this request is probably the same
  work item seen from the memory side rather than the latency side.

No private values above: timestamps, public constants, commit hashes, and
kernel-log magnitudes only.
