# Review — ralph iteration 50: counting_semantics acceptance + single-step MMIO-write trap fix

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-50-counting-semantics-test`
- **Diff under review:** `git diff main...HEAD`
  - `crates/dh-vmm/src/boundary.rs` (+18/-8) — re-arm `guest_debug` after handled exits in `land_at` near phase + `step_one_entry`
  - `tests/determinism/tests/counting_semantics.rs` (+367, new) — single-step attribution test + land-across-MMIO-write regression
  - `.agents/docs/determinism-hypervisor/ARCHITECTURE.md` (+11/-6) — HLT → measured set; §3.2 re-arm rule

## Verdict: APPROVE

This is correct, well-reasoned, and — critically for this engine — I could not break it with the adversarial experiments the prompt pointed me at. The two sharpest theoretical hazards (write-spanning-step overshoot at a target adjacent to the MMIO write, and non-deterministic landing on a zero-retirement icount plateau) both turn out to be **structurally impossible** on the kvm-intel class, and I proved that with live experiments (240 cold-boot landings on the plateau, all bit-identical; the post-write target lands exactly, never overshoots). The margin-8 question raised in the prompt is a real *latent* fragility in the test's *choice of margins*, but it is NOT a flake here and not a code defect — see 01.

## What I verified (live, on the lab box, kernel 6.8, kvm-intel)

| Check | Result |
|---|---|
| Both new tests, default (parallel) cargo test | pass |
| `landing_across_an_mmio_write_does_not_free_run` x30 | 0 failures |
| Full `counting_semantics` binary (both tests) x20 parallel | 0 failures |
| Plateau target `land_at(12)` x240 cold boots under parallel load | **all rip=0x100047** (1 distinct) |
| Post-write target `land_at(13)` | always rip=0x100058, **never overshoots** |
| `dh-vmm --lib` (73 live KVM tests, 1 binary, default parallel) x3 | pass, stable |
| Full workspace `cargo test` (parallel) | all green |
| `cargo clippy --workspace --all-targets` (x86_64) | clean |
| `cargo clippy ... --target aarch64-unknown-linux-gnu` | clean |
| Working tree after experiments | clean, no stashes, instrumentation reverted |

## Severity summary

- **Critical:** 0
- **Important:** 0
- **Suggestions:** 4 (see 02 / 04)

The Important-tier concerns the prompt asked me to chase (margin-8 overshoot flake; plateau determinism) were *disproven by experiment* and are downgraded to a suggestion (document the implicit contract; consider raising the test margins for robustness against future guest edits).
