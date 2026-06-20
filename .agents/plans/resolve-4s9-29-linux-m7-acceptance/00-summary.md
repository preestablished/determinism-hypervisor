# Resolve 4s9.29 Linux M7 Fork VerifyReplay Acceptance

Plan name: `resolve-4s9-29-linux-m7-acceptance`

Selected bead: `determinism-hypervisor-4s9.29` - Add Linux M7 fork VerifyReplay acceptance and nightly canary.

## Why This Bead

`4s9.29` is still marked `BLOCKED`, but its direct dependencies are now closed:

- `4s9.21` - Linux restore/fork/replay must not rerun boot initialization.
- `4s9.27` - Linux M5 record/replay corpus.
- `4s9.28` - Linux M4/M5 frame scheduling and pv-net regression coverage.

The current blocker is no longer missing prerequisite work. The blocker is that `crates/dh-worker/tests/m7_fork_verify.rs` still has only the nanokernel `pad_echo` fixture and a guard that panics when `DH_M7_ACCEPT_GUEST=linux`.

Closing `4s9.29` unblocks:

- `4s9.31` - preserve nanokernel gates and golden fixtures after M9.
- `4s9.32` - update Phase 1 and Phase 2 exit gates with Linux and nanokernel evidence.
- `4s9.33` - document Linux gate commands, runner requirements, and CI nightly classification.

## Reference Host Assumption

This plan assumes the implementation agent is running on this Linux/KVM reference host, not a generic development machine. Treat this host as the place to gather final evidence.

Expected local artifact staging:

```bash
m9_artifact_root="$HOME/.cache/dh-m9/reference-workload"
export DH_M9_BZIMAGE="$m9_artifact_root/bzImage"
export DH_M9_INITRAMFS="$m9_artifact_root/initramfs.cpio"
export DH_M9_BASE_IMAGE="$m9_artifact_root/base.img"
export DH_M9_GAME_IMAGE="$m9_artifact_root/game.img"
export DH_M9_IMAGE_CACHE="$HOME/.cache/dh-m9/image-cache"
mkdir -p "$DH_M9_IMAGE_CACHE"
```

Known current reference artifact hashes from the checked-in M9 Linux M5 corpus manifest:

```text
bzImage      595466463a37efac6822ffccf3e61d0a2230e7d223a94c0bce5eb78b2f43bee9
initramfs    87edf64db22dc85ef0c6b17fdc6e58a8f73391a6053e96f7a1056da7d08b9f57
base.img     488de202f73bd976de4e7048f4e1f39a776d86d582b7348ff53bf432b987fca8
game.img     e02849845005d9d34fa3245d98fa59116a0245ed0136b496dbd2defebdc203ac
```

Do not downgrade this bead to skip-allowed or operator-only evidence unless the reference host actually loses KVM or the staged M9 artifacts.

## Desired End State

`crates/dh-worker/tests/m7_fork_verify.rs` supports two guest modes:

- `DH_M7_ACCEPT_GUEST` unset or `nanokernel`: current `pad_echo` M7 acceptance behavior is preserved.
- `DH_M7_ACCEPT_GUEST=linux`: the harness boots the M9 Linux fixture to READY once, forks children from the READY snapshot, runs each child for a deterministic post-READY frame budget, seals each child DHILOG, and verifies each child with `VerifyReplay`.

The Linux acceptance must prove:

- 1000/1000 fork children complete `VerifyReplay` with `Done`.
- zero `Divergence` messages are observed.
- every `VerifyReplay.Done.end_state_hash` equals the live child snapshot state hash.
- every child DHILOG is a valid single edge from the READY snapshot to the child snapshot.
- the cross-slot rerun test proves same-seed children produce identical snapshot refs, state hashes, input log ids, and DHILOG payloads across sampled slots.
- nightly runs a 100-child Linux canary on the KVM reference runner.

## File Map

- `01-current-state-and-blocker.md` records the current implementation state and why the bead is still blocked.
- `02-implementation-sequence.md` gives the coding plan and file-level changes.
- `03-linux-log-and-replay-contract.md` defines the Linux child evidence contract.
- `04-validation-and-acceptance.md` lists the commands and expected proof points.
- `05-risks-and-debugging.md` captures known risks, failure modes, and triage strategy.
- `06-beads-and-handoff.md` covers Beads, git closeout, and downstream status.
- `07-review-resolution.md` will summarize subagent feedback and any resulting plan edits.
- `08-review-feasibility.md` will contain the implementation-feasibility review.
- `09-review-acceptance.md` will contain the acceptance/completeness review.
