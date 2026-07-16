# Package 04 — `uyhu` (P2): isolate feature-only capture cost

Bead: `determinism-hypervisor-uyhu`
Filed: `.agents/requests/phase4-oom-fix-and-capture-engine-proving/04a-item5-resolution.md`
(2026-07-08, "Cost" section).

## What this is — and is not

The capture-proof session measured a capture-attributable TakeSnapshot delta of
**p50 ≈ 1.9 ms** (with-vs-without capture, `seal_input_log=false`, 100 iters,
2-core lab host). That number is a noisy **upper bound**: every iteration also
lz4-compresses the full 229,376-byte framebuffer, while the packed feature
payload is only 591 bytes. It sits just above state-scorer M4's **1.5 ms p50
budget**, so `uyhu` asks for a measurement that separates feature-byte
extraction cost from framebuffer-lz4 cost, written up as evidence.

**This is measurement + documentation, not optimization.** No production code
changes. If — and only if — the isolated feature-only cost genuinely exceeds
the 1.5 ms p50 budget, file a follow-up request/bead for optimization; do not
optimize here.

## Where the seams are (grounded at HEAD)

- Engine: `capture_at_boundary`, `crates/dh-worker/src/service.rs:3363`.
  Two separable cost components, cleanly split by the `CaptureSpec` fields:
  - **feature ranges**: manifest resolve + `channel.read_region` per range +
    packing (`capture.ranges`, framebuffer flag off);
  - **framebuffer**: `read_framebuffer_region_from_bus` +
    `lz4_flex::compress_prepend_size` (`capture.framebuffer = true`).
- Existing measurement to extend:
  `crates/dh-worker/tests/capture_engine_real_image.rs`, cost block at
  ~lines 546–660 (`COST_ITERS = 100`, `Instant::now()` around TakeSnapshot
  with/without capture, p50/p95 percentiles, printout "NOT a gate").
- Bench harness precedent: `crates/dh-worker/benches/perf_gates.rs`
  (criterion, `harness = false`, x86_64-gated, skips cleanly without
  `/dev/kvm`). Fine as a home if a criterion trend instrument is wanted, but
  the simpler and more consistent option is extending the existing ignored
  test's cost block — prefer that unless there is a reason not to.
- Environment: the real-image test needs the reference host with `DH_M9_*`
  pointing at the **dist bundle** `reference-workload/dist/workload-image-0.1.0/`
  (decompressed `initramfs.cpio.zst` carrying `usr/bin/refwork-harness`); the
  module doc explicitly rejects the old `~/.cache/dh-m9/.../initramfs.cpio`
  contract fixture. Staging + invocation in the test's module doc; see
  `.agents/requests/phase4-oom-fix-and-capture-engine-proving/` and its
  plan's `01-entry-and-staging.md`, and 00-overview's "Environment
  Requirements".

## Design (keep it small)

Extend the existing cost block in `capture_engine_real_image.rs` from two
variants to **three**, same methodology (paused boundary, TakeSnapshot,
`seal_input_log=false`, ≥100 iters, p50+p95):

1. no capture (baseline) — exists;
2. **features-only**: the compiled 12-range spec with `framebuffer: false` —
   NEW;
3. full capture (features + framebuffer lz4) — exists.

Report deltas (2−1) = feature-only cost and (3−1) = full cost, and (3−2) =
fb-lz4-attributable cost. Print all three the way the current block does
("NOT a gate; scorer M4's 1.5 ms p50 budget is downstream").

Optional-but-cheap corroboration if the numbers look odd: a direct
`Instant::now()` pair around the `capture.ranges` loop vs the framebuffer
branch inside a test-local copy of the flow — but do not instrument production
`service.rs` for this; the with/without deltas are the method of record.

## Deliverables

1. The measurement change (test-side only) committed. Run the standard review
   pass (house `/review` workflow) on the measurement change before
   committing, matching package 05 Case B's routing.
2. A short note at
   `.agents/docs/determinism-hypervisor/capture-cost-isolation.md` (new file in
   the existing docs dir alongside `API.md`/`INTEGRATION.md`) containing:
   method, host, iteration count, the three p50/p95 rows, the two derived
   deltas, the verdict against the 1.5 ms scorer-M4 budget, and the explicit
   caveat that TakeSnapshot machinery still dominates the absolute numbers
   (this is a delta method, not a microbenchmark).
3. Bead disposition:
   - feature-only delta p50 ≤ 1.5 ms (expected — payload is 591 bytes vs a
     229 KB compress):
     `bd close determinism-hypervisor-uyhu -r "<numbers>, evidence at .agents/docs/determinism-hypervisor/capture-cost-isolation.md, commit <sha>"`.
   - feature-only delta p50 > 1.5 ms: keep `uyhu` open or close-and-supersede
     per bead conventions, and file a follow-up optimization request (new
     bead, P1, linking the note). Do not optimize in this package.

## Acceptance

- The extended ignored test runs green on the reference host and prints the
  three-variant table:

  ```bash
  # staging env per the test's module doc, then:
  cargo test -p dh-worker --test capture_engine_real_image --release -- --ignored --nocapture
  ```

- The note exists with real measured numbers (no placeholders).
- Hygiene gates still green (test-only + docs change):

  ```bash
  cargo test --workspace --all-targets
  cargo clippy --workspace --all-targets -- -D warnings
  members=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[].name')
  cargo fmt --check $(printf -- '--package %s ' $members)
  ```

- No production source touched → no determinism-suite obligation; state so.

## Failure guidance

- **Staged artifacts missing/stale on the host**: re-stage per the module
  doc / `01-entry-and-staging.md`; if the dist image itself is gone, this
  package is blocked — annotate the bead with the missing prerequisite, do not
  substitute a synthetic image (the bead is specifically about real-image
  cost).
- **High variance swamping the 591-byte signal** (features-only delta within
  noise of zero): that is itself a valid finding — report "feature-only cost
  below measurement noise floor of X µs at N iters", which comfortably clears
  the 1.5 ms budget. Consider raising iters to 500 and pinning cores
  (`taskset`) as the M8 CI lane does, before concluding.
- **Numbers exceed budget**: do not tune, do not cache, do not touch
  `capture_at_boundary`. File the follow-up and hand off.
- **Reference host unavailable**: same handling as package 05's
  host-unavailable branch and 00-overview's "Where To Execute" — this test
  runs nowhere else; document the block in the plan dir, annotate the bead,
  and stop. Do not close the bead from a host that cannot run the real image.

---

## EXECUTED — 2026-07-16, infra-control, HEAD b4358a7 + this measurement change

- Extended the cost block in `capture_engine_real_image.rs` to three variants
  (no-capture / features-only / full), interleaved per iteration, signed p50
  deltas (review finding: `saturating_sub` clamped noise-inverted deltas to 0
  — observed live on the first, unpinned run), percentile helper now takes a
  sorted slice. Test-side only; no production source touched, determinism
  obligation vacuous.
- Review pass: 8-angle /code-review (medium) on the diff; 2 findings applied
  (signed deltas, pct-by-slice), 2 declined with reasons (timing-block helper
  extraction — async-closure churn in a proof test; comment restating payload
  sizes — matches module-doc convention). No correctness findings survived
  against the measurement itself.
- Three accepted pinned runs (`taskset -c 2-5`); primary quiet-host run:
  features-only delta **+50 µs p50**, full **+512 µs**, fb-lz4 **+462 µs**
  (baseline p50 36,917 µs). Loaded-host replicates are directionally
  consistent (+242 µs / noise-inverted) but non-probative against a 1.5 ms
  budget (their median noise is ±~4 ms). **Verdict rests on the quiet
  primary run: feature-only cost clears scorer M4's 1.5 ms p50 budget by
  ~30× — close `uyhu`, no optimization follow-up.**
- Evidence note: `.agents/docs/determinism-hypervisor/capture-cost-isolation.md`.
- Runner-reservation caveat recorded there (no passwordless sudo; no
  queued/in-progress runs during windows).
- Second review round (user-requested, two subagents, 2026-07-16): verdict
  overclaim fixed (loaded runs no longer presented as budget bounds), the
  cost loop now rotates the intra-iteration variant order (fixed order
  biased deltas downward; recorded runs predate rotation and carry that
  caveat in the note), and a rotated-order validation run came back green
  (numbers noise-dominated by concurrent load — recorded as validation
  only). Process note: the review pass used the 8-angle `/code-review`
  harness rather than the `/review` skill named in Deliverables — same
  intent (independent multi-angle review with findings dispositioned);
  deviation recorded here.
- Post-commit CI corroboration: push run 29519130362 green at `bdd60c3`
  (both hosted lanes + kvm-intel), covering the f06050c measurement change.
