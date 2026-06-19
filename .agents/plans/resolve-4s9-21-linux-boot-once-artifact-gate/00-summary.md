# Resolve determinism-hypervisor-4s9.21

Plan name: `resolve-4s9-21-linux-boot-once-artifact-gate`

Target bead: `determinism-hypervisor-4s9.21` - Ensure Linux restore fork and replay never rerun boot initialization.

## Current State

The implementation for this bead is already merged to `main` in merge commit `e32a55c`.

The remaining blocker is the final artifact-backed M9 acceptance run. The current shell did not have the required `DH_M9_*` artifact variables, so the ignored Linux tests were only compiled and exercised in skip mode.

The bead should not need new code if both artifact-backed tests pass. If either test fails with real artifacts, use the triage plan in `03-failure-triage.md` and implement the smallest repair that preserves the existing contract.

## Acceptance Target

The bead can be closed only when both exact commands pass with real artifacts and `DH_M9_ALLOW_SKIP=0`:

```bash
DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test restore_engine linux_boot_once --release -- --ignored --nocapture
DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test replay_engine linux_boot_once --release -- --ignored --nocapture
```

The tests prove:

- `CreateVm` is the only BzImage loader call.
- `RestoreSnapshot` does not invoke the Linux loader.
- Tier-A `Fork` does not invoke the Linux loader.
- `VerifyReplay` restores and replays the Linux READY segment without invoking the Linux loader.
- Restored/forked/replayed READY state preserves `machine_config_hash`, `state_hash`, EVTC, and BLKO snapshot sections.

## Plan Files

- `01-artifact-prerequisites.md` defines the required host, KVM, and artifact environment.
- `02-acceptance-runbook.md` gives the exact command sequence for the happy path.
- `03-failure-triage.md` describes how to debug and repair failures if the artifact-backed gates fail.
- `04-code-seams.md` names the source files and invariants a repair is allowed to touch.
- `05-bead-closeout.md` defines how to update beads and push once the acceptance evidence is captured.
