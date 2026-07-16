# Addendum — closeout tails `uyhu` and `i74w` (2026-07-16)

Executed under `.agents/plans/quality-gate-closeout-tails/` (packages 04/05)
on `infra-control` at HEAD `b4358a7`.

## `uyhu` (04a-item5 "Cost" follow-up) — CLOSED

The capture cost was isolated per the bead: the cost block in
`capture_engine_real_image.rs` now measures three variants (no-capture /
features-only / full). Primary quiet-host run: **feature-only delta
+50 µs p50**, full-capture +512 µs, fb-lz4-attributable +462 µs
(loaded-host replicates directionally consistent but non-probative — their
median noise is ±~4 ms). The ~1.9 ms figure from the 2026-07-08 proof was
framebuffer-lz4-dominated, as suspected. **Feature-only cost clears scorer
M4's 1.5 ms p50 budget by ~30× on the quiet primary run; no optimization
follow-up filed.** Full method, tables, and caveats:
`.agents/docs/determinism-hypervisor/capture-cost-isolation.md`.

## `i74w` (04-resolution corpus item) — OPEN, gate re-confirmed at HEAD

Both failure legs reproduced empirically on 2026-07-16: manifest-matching
old-fixture artifacts are rejected pre-boot (autostart contract), and the
staged real dist image fails Run-until-READY with the epoch_len=745000
OVERSHOOT (`counted 642206698 past target 642190000`) — identical to
`jyo7`'s 2026-07-07 observation. Re-baseline remains blocked on `jyo7`
(dependency recorded); no regen attempted. Evidence in the plan's package 05
EXECUTED note and bd comments on both beads.

## `9f3x`

Untouched, per plan scope: still waiting on the rom-operator-bridge team's
redeploy confirmation (their bead `l1w`).
