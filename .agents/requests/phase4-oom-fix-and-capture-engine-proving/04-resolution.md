# Resolution (Items 1–4; Item 5 Explicitly Waiting) — 2026-07-07

Handback per `03-verification-offer.md`.

- **Bead id:** `determinism-hypervisor-9f3x` (P1, filed before code,
  links the bridge request dir and bridge beads `l1w`/`9bx`). Follow-ups
  filed: `iar8` (DebugSerial drain is doc-fiction — buffer now bounded),
  `i74w` (M9 record-replay corpus manifest unrunnable: baselined on the
  pre-real-emulator initramfs; needs a reviewed re-baseline — adjacent
  to round-1's `linux_m5` item).
- **Fix commit:** `c0337ab` on `main`.
- **Root cause:** agenda materialization — `dh_vmm::agenda::compile`
  built one StopPoint per epoch-grid point across the entire icount
  budget before the guest ran; unbounded-budget streaming Runs compiled
  terabyte-scale agendas (~370 MB/s Vec-doubling growth, guest starved,
  OOM at ~26 GB). `01-current-state.md`'s suspects (hash path, recording
  buffer, dirty tracking) all exonerated empirically, as it predicted —
  the profile was indeed the finding. Bisection checkpoints confirmed
  gated off (and the stock binary has no enablement path).
- **Profile evidence path:** `target/oom-evidence-2026-07-07/` at the
  fix-commit checkout (pre-fix CSVs with the hblkhd doubling signature,
  kernel OOM-kill record at 25.8 GB anon, perf attribution, post-fix
  12 kB/180 s run, gate logs, README with the full narrative).
- **Guard location + bound derivation:**
  `crates/dh-worker/tests/rss_regression.rs` (M9 lab lane, `--ignored`,
  release; invocation in the module doc). Ceiling =
  `(idle-baseline RSS + slots × mem_bytes) × 1.25` — inputs printed at
  runtime, deliberately duration-independent; plateau = final-third
  windowed-median ≤ warm-up median × 1.10 (tolerance justified against
  the post-fix profile: observed drift 4 kB over 180 s). Green at the
  fix commit (max 689 MB vs 1,025 MB bound, 1422 frames).
- **Determinism (AC2's hard clause):** record/replay gates green at the
  fix commit — 3× consecutive full workspace runs (757 tests each),
  which include replaying the committed PRE-fix `pad_echo_6s`
  `recording.dhilog` on the post-fix build: epoch hash chain and end
  state hash bit-identical. No hash-value or sealed-format change.
  Additionally the retired agenda implementation is retained as a
  test-only differential oracle (2000-case property test asserts
  stop-point sequence equality). M9 capture-neutrality acceptance also
  green. NOTE: the M9-corpus manifest reverify is unrunnable for a
  pre-existing reason unrelated to this fix (`i74w` above).
- **`9bx` answer:** unbounded — worker RSS no longer scales with segment
  budget; drop the 200M clamp and its ~50 ms reopen stall. Residual
  growth is the in-memory DHILOG at ~2–3 KB/s at 60 fps (~10 MB/hour);
  the snapstore 4 MiB inline input-log cap (~20–30 min of play) is the
  practical sealing granularity if segments are verified. Carrying
  build `c0337ab`+; deploy window is the bridge's (rom-bridge-o73
  runbook). Full text in the bridge dir's `01-resolution.md`.
- **Item 5 (capture engine on real data): WAITING on `refwork-gp9`**
  (the regenerated workload image), per this request's own entry
  condition. Nothing consumed it this session. Standing warning
  honored: until the fixed build deploys to the lab worker, any capture
  session on the old build must use segment-bounded Runs (`fbd38d1`
  pattern).
- **`38b6` annotation (AC5):** recorded on the bead — the fix is
  DISJOINT from the M4 epoch-hash pipeline (the leak was never the hash
  path); M4 stays deferred on latency grounds, with two notes for its
  eventual build (shadow must fit the guard ceiling; allocate per slot,
  never per epoch).

Awaiting your `05-verification.md`; the RSS guard re-runs cleanly from
a fresh checkout given staged `DH_M9_*` artifacts (dist
workload-image-0.1.0 initramfs + cached base/game images).
