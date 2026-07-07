# Current State (Evidence-Based)

Repo `main` at `a7f2117` (the OOM request filing; the last engineering
commits are `bdd476b`/`4285b45`), clean tree, assessed 2026-07-07.
Beads: 4 issues, 0 in progress (`veu` P1 docs, `38b6` P2 deferred
epoch-hash pipeline, `umay` P3, `i71k` P4). **No bead exists for the
OOM incident.**

## The Incident (Sources: The Bridge's Filing, Commit `fbd38d1`, And Bridge Bead `l1w`)

- First live streaming session, 2026-07-07 ~03:29Z, bridge `fb2a7fc`
  against deployed worker `4285b45`: one long `RunWithFrameCapture`
  (large icount budget, ~60 Hz pacing) → kernel OOM-killer fired from a
  `dh-slot-0` thread at ~6.84M resident pages (~26 GB anon RSS);
  collateral: an unrelated k8s pod and snapstore; restarted per the
  `rom-bridge-o73` runbook at 03:42 (these three details are from
  bridge bead `l1w`, not the filing itself).
- Growth ~300–500 MB/s — ~20–35× the ~14 MB/s frame stream (the
  filing's "three orders of magnitude" was bad arithmetic; don't
  repeat it). The stream channel is exonerated on solid grounds
  regardless: capacity-2 backpressure
  (`FRAME_STREAM_CHANNEL_CAPACITY=2`, `dh-worker/src/service.rs`), and
  ~14 MB/s over the ~75 s incident ≈ 1 GB — nowhere near 26 GB.
- Consistent with a ~128 MiB (guest-memory-sized) buffer retained per
  epoch within a Run, freed only at teardown; masked historically by
  per-frame `Run{frame_budget=1}` teardowns.
- Bridge containment (`fbd38d1`): `PLAY_STREAM_SEGMENT_ICOUNT_BUDGET`
  ≈200M instructions (~4 epochs) + seamless segment reopen; cost ~50 ms
  hash-link stall per boundary. Bridge beads: `l1w` (incident record,
  closes on their eqb validation passing on the fixed stack), `9bx`
  (raise segment budget — **waits on your green light**).

## Suspect Code Paths (Start Here, Verify By Profiling)

- `crates/dh-vmm/src/runctl.rs:317` `run_segment_with_epochs` →
  `crates/dh-vmm/src/hash.rs:130` `push_final_link` — the synchronous
  per-epoch hash path. Caveat: as written it streams pages through a
  reusable 4 KiB buffer and retains 32 bytes/epoch — a *latency*
  suspect more than a 128 MiB/epoch retainer; if profiling exonerates
  it for memory, that's expected.
- `crates/dh-vmm/src/recording.rs` — the recording `Vec` accumulates
  for the whole Run, sealing at teardown; but per-epoch records are
  tens of bytes, so alone it can't explain 300–500 MB/s either.
- In short: the named suspects don't obviously account for the
  signature — **the profile is the finding**, not these guesses.
- Probably innocent: `crates/dh-vmm/src/dirty.rs` (`DirtyPageSet` is a
  bounded clearing bitmap; `DirtyRing` cursor doesn't rewind).
- Planned-work overlap: bead `38b6` / the play-60fps M4 shadow-copy +
  drain-per-epoch design
  (`.agents/plans/play-60fps-decouple-hash-from-frames/03-input-and-epoch-hash-decoupling.md`)
  — its own risk note priced "+128 MiB per slot." The incremental-free
  fix and that design touch the same drain points; decide deliberately
  whether the fix *is* a slice of M4 or a separate minimal patch, and
  record the decision on `38b6`.

## Capture Engine: Built, Never Exercised On Real Data

- Proto surface: `proto/hypervisor.proto` `CaptureSpec`/`ExtractRange`
  (~:95–104), `feature_bytes` (~:238–239), `fb_lz4` (~:276–277); served
  by `crates/dh-worker/src/service.rs`. Landed as Phase 3's
  pulled-forward capture-engine item ("consume the guest-sdk region
  manifest at channel init; accept a compiled extraction list...").
- guest-sdk Ms4 (region publication — the joint half) is **done and
  independently verified** (2026-07-02), so real RAM extraction has no
  guest-side gap. What has never happened: a compiled extraction list
  run against a *real workload image's* manifest with the returned
  `feature_bytes` cross-checked against independent `detguest-host`
  reads of the same ranges.
- Consumers queued behind this proof: reference-workload's round-2
  corpus request (packaging), then state-scorer M1 golden tests
  (`phase-4-scoring-and-inputs.md` entry + scorer chain).

## Dependencies

- The leak fix has none — profiling and the fix run on the staged
  fixtures and synthetic workloads in-repo.
- The capture proof consumes reference-workload's regenerated image
  (`refwork-gp9`, their round-1 request) — sequence it after that
  lands; the leak fix must land first anyway (a capture run is a long
  Run).
- Round-1's items are independent and untouched by this request.
