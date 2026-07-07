# M0 Measurement + M1 Release Builds

## M0 — Attribute the ~200ms/frame before changing anything

Add (or use existing) tracing spans / ad-hoc timing so one Play frame is
broken into:

1. bridge → worker `Run{frame_budget=1}` RPC wall time, split into:
   - guest execution time to the FRAME_MARK exit (emulator cost);
   - boundary/agenda overhead in `run_segment_*` (`crates/dh-vmm/src/runctl.rs`);
   - `hash_final_stop` link cost (`StateHashChain::push_final_link`,
     `crates/dh-vmm/src/hash.rs:130`) — instrument this one explicitly;
   - epoch link cost when a 50M-instruction epoch boundary is crossed.
2. bridge → worker `GetFramebuffer` RPC wall time (pixel copy + transport).
3. bridge-side PNG encode + `/ws/frames` publish.

Also record from the same run:

- **instructions per SNES frame** for the reference workload (delta of
  `RunResponse.icount` across frames). This decides how often epoch links
  interleave with frames and whether M4 is needed:
  - if a frame is ~1M instr → an epoch every ~50 frames → epoch links are
    ~1ms/frame amortized in release → M4 unnecessary;
  - if a frame is ~10M+ instr → an epoch every ≤5 frames → ~10ms+/frame
    amortized → M4 (or `epoch_len` change) is on the critical path.
- frames-per-second end to end, as the baseline number.

Cheapest harness: a small `#[ignore]`d dh-worker integration test (pattern:
`crates/dh-worker/tests/linux_worker_api.rs`) that restores the M9 READY
snapshot, loops `Run{frame_budget=1}` + `GetFramebuffer` 600 times, and
prints stage timings. Run it against a debug and a release worker build.

Known host datum (2026-07-06, this box): `b3sum --num-threads 1` over
128 MiB ≈ 50–60ms. Treat that as the floor for one full-memory link.

## M1 — Stop running debug builds in the operator stack

### Runbook fix

`docs/ops/rom-bridge-o73-ready-snapshot.md` (worker launch, line ~151)
uses:

```sh
nohup setsid cargo run -p dh-worker --bin dh-workerd -- serve ...
```

Change to build once and run the release artifact:

```sh
cargo build --release -p dh-worker --bin dh-workerd
nohup setsid target/release/dh-workerd serve ...
```

Sweep the same document (and `docs/ops/`, `deploy/` docs in
rom-operator-bridge, and any snapshot-store serving runbook) for every
`cargo run` / `target/debug` launch of a long-lived service:

- `dh-workerd`
- `snapstore-server` (confirmed running from `target/debug` today; the
  same runbook launches it via `cargo run` at
  `docs/ops/rom-bridge-o73-ready-snapshot.md` line ~123 — fix both launch
  commands in the same pass, and file a bead in snapshot-store if its own
  docs repeat the pattern)
- `dh-m9-ready-handoff` (one-shot; release still preferred for the
  snapshot write path but not perf-critical)

Consider adding a guard so this cannot silently regress: the serve
runbooks should `--version`-check or log the build profile at startup
(e.g. emit `cfg!(debug_assertions)` in `GetWorkerInfo` / startup log and
have the private validation reference assert it is a release build).

### Rollout

1. Build release `dh-workerd` and `snapshot-store` `snapstore-server` in
   the operator worker checkout.
2. Stop the running debug processes; relaunch per the corrected runbook
   against the same operator-private runtime root (same UDS paths, same
   snapstore data root). No snapshot regeneration is needed — build
   profile is not part of machine identity.
3. Restart a bridge session and re-run the M0 measurement.

### Acceptance

- `ps` shows both daemons running from `target/release/`.
- M0 harness numbers re-collected on release builds; the per-frame budget
  table updated in this plan directory (append a `05-measurements.md`).
- Expected outcome: wall time for the 240s scenario drops several-fold but
  remains above 20s — the residual should be dominated by the per-frame
  `push_final_link` (~50ms) plus two RPCs, which motivates M2.
