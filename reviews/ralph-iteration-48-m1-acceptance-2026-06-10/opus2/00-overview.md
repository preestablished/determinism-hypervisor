# M1 Acceptance (bead 40q) — Second-Reviewer Overview

- Reviewer: Claude Opus (2nd reviewer)
- Date: 2026-06-10
- Branch: ralph/iteration-48-m1-acceptance
- Scope reviewed: `git diff main...HEAD` — `tests/determinism/tests/m1_acceptance.rs`,
  `tests/determinism/Cargo.toml`, `docs/ops/cpuid-diff-infra-control.txt`, `Cargo.lock`.
  Cross-read for grounding: `dhilog.rs`, `ctx.rs`, `detchannel.rs`, `runctl.rs`,
  `hash.rs`, `clock.rs`, `pad.rs`, `entropy.rs`, `blk.rs`, `image.rs`, `device_exercise.asm`.

## Verdict: APPROVE

The test is correct, lands live, and is bit-stable across repeats. It does what the
bead asks: it boots `device_exercise` over the REAL device surface and asserts each
device value end to end. Nothing here is wrong or unsound.

My findings are about what the run-twice comparison does NOT cover (device-internal
state, drained beacon contents, IRQ queue) — these are determinism *blind spots* in
the acceptance net, not defects in the code under test. The strongest single
improvement is widening the run-twice comparison to include the device fingerprint
and the drained beacons; today the comparison leans almost entirely on the full-RAM
state hash to catch device divergence, which works for THIS guest only because every
device artifact it produces happens to live in guest RAM.

## What I ran (live, lab box, /dev/kvm rw)

- `cargo test -p determinism-tests --test m1_acceptance` — PASS, repeated 5x, all PASS.
- `cargo test --workspace` — all suites PASS (incl. the 31s and 99s live KVM suites).
- `cargo clippy --workspace --all-targets` — clean (no warnings, no errors).
- `cargo check --workspace --all-targets --target aarch64-unknown-linux-gnu` — PASS
  (the `#![cfg(target_arch = "x86_64")]` gate empties the test file on arm; the new
  gated dev-deps resolve fine and do not leak into the arm build).
- Working tree clean after the run (`git status --short` empty).

## Severity counts

- Critical: 0
- Important: 2  (run-twice blind spots: device state + beacons; IRQ queue never asserted-empty)
- Suggestions: 6
