# Review Overview

- Branch: `ralph/iteration-112-detchannelhost-detdevice-adapter-plan-restore-seam`
- Base: `main`
- Date: 2026-06-15
- Reviewer: Local Subagent
- Overall verdict: APPROVE

This branch adds a thin `DetChannelDevice` adapter around `DetChannelHost`, exports it from `dh-devices`, and proves that EVTC can now participate in the existing `MmioBus` snapshot/restore path without changing `restore_engine` logic. The adapter keeps the live detchannel ABI on PIO, serves only MAGIC/VERSION through the bus convention, delegates EVTC snapshot bytes to the host, and uses a restore-time factory to provide a fresh `FaultPlan` for each restore.

Bead `determinism-hypervisor-abe` does not define a separate acceptance field. The description asks for a `DetDevice` adapter or implementation plus a plan-supplying restore seam so restore can drive EVTC through the unchanged device loop. That is covered by the new adapter tests in `crates/dh-devices/src/detchannel.rs` and the KVM joint test in `crates/dh-worker/tests/restore_engine.rs`. I did not find production runtime construction that should also be updated in this branch; service/runtime lifecycle is still builder-seamed or unimplemented, and the downstream M6 composition bead remains open.

## Stats

- Files changed: 7
- Lines added/removed: +336/-10
- Commits: 1 (`96db9c6 ralph: iteration 112 checkpoint - detchannel device adapter`)

## Verification

- Ran `cargo test -p dh-devices`: passed.
- Ran `cargo test -p dh-worker --test restore_engine -- --nocapture`: passed, including the new KVM EVTC reattach test.
- Ran `cargo check -p dh-worker --tests`: passed.
- Ran `git diff --check main...HEAD`: passed.
