# Big Change Planning with Beads

## Agent Instructions

You are an expert software architect creating a comprehensive task breakdown for a change to an existing codebase. This task graph will be executed by AI agents working in parallel, coordinated through MCP Agent Mail with file reservations to prevent conflicts.

<quality_expectations>
Create a thorough, production-ready task graph. Include all necessary analysis, preparation, implementation, testing, and documentation tasks. Go beyond the basics - consider edge cases, error handling, security considerations, backwards compatibility, and integration points. Each task should be specific enough for an agent to execute independently without ambiguity.
</quality_expectations>

<critical_constraint>
You must NOT implement any of the changes yourself. Your ONLY output is a bash shell script containing `bd create` and `bd dep add` commands. Do NOT use `bd add` - the correct command is `bd create`. Do not write code. Do not create files other than the shell script. Do not modify existing files. Read and analyze the codebase, then produce the script.
</critical_constraint>

## Change Information

### Change Type
NEW_FEATURE

### Description
M9 - minimal-Linux guest path (bzImage boot).

Scheduling note: the hypervisor's own plan lists this last, but `reference-workload`'s image pipeline (pinned kernel + static-musl + deterministic cpio) and the agent-as-PID-1 design assume a Linux guest. The nanokernel guest was enough for Phases 1-2 gates; in-VM workload bring-up needs Linux now. Re-run the Phase 1 determinism gate and the Phase 2 fork gate against the Linux guest before building on it.

This comes from `~/.agents/projects/determinism/phases/phase-3-workload-in-the-box.md`: in Phase 3, `determinism-hypervisor` M9 is pulled forward because `guest-sdk` Ms3/Ms4/Ms5 and `reference-workload` M4/M5 depend on a Linux guest path. The existing repo already models `BootSpec::BzImage` in config/proto/image resolution, but `crates/dh-worker/src/service.rs` still rejects `ResolvedBoot::BzImage` as unimplemented and the runnable Phase 1/2 gates are still nanokernel-only.

### Links to Relevant Documentation
- `~/.agents/projects/determinism/`
- `~/.agents/projects/determinism/phases/phase-3-workload-in-the-box.md`
- `~/.agents/projects/determinism/phases/README.md`
- `~/.agents/projects/determinism/docs/determinism-hypervisor/IMPLEMENTATION-PLAN.md` (M9 - Minimal-Linux guest path)
- `~/.agents/projects/determinism/docs/determinism-hypervisor/INTEGRATION.md` (guest-sdk contract summary)
- `~/.agents/projects/determinism/docs/reference-workload/ARCHITECTURE.md` (guest image build pipeline)
- `~/.agents/projects/determinism/docs/reference-workload/API.md` (WorkloadImage manifest and `linux-direct` boot contract)
- `docs/phase-1-exit-gate.md`
- `docs/phase-2-exit-gate.md`
- `docs/ops/test-partitioning.md`
- `docs/decisions/tsc-alignment.md`

### Affected Areas
- `crates/dh-vmm/src/boot.rs`: current loader is ELF-only, builds identity page tables and nanokernel BootInfo, applies the MSR filter, and enters long mode directly. M9 needs a deterministic Linux direct-boot path for bzImage + initramfs, including setup header validation, boot params/zero page, e820 memory map, cmdline placement, initramfs placement, entry state, and canonical handling of unsupported protocol features. M9 must compose the Linux cmdline as the hypervisor-owned deterministic baseline `console=ttyS0 nokaslr norandmaps random.trust_cpu=off tsc=unstable clocksource=dh-pvclock nohz=off highres=off init=/init` plus only whitelisted `BzImageBoot.cmdline` extras such as `quiet` and `loglevel=<n>`. Validate or canonicalize extras in `config`/`proto_map`/worker before `MachineConfig` is hashed or snapshotted, and test that the MCFG/hash preimage matches the actual boot cmdline.
- `crates/dh-vmm/src/{kvm,cpuid,msr,inject,runctl,hash,recording}.rs`: Linux early boot will exercise broader CPU/MSR/interrupt/time surfaces than the nanokernel. Preserve no in-kernel irqchip, no PIT, no kvmclock, deterministic CPUID masking, default-deny MSR behavior, guest-only instruction counting, state hashing, and run-control semantics while adding any required lAPIC stub or early-boot compatibility. If Linux requires lAPIC compatibility, scope it as a deterministic persisted device rather than a boot-only shim: enumerate the early Linux APIC/MSR/MMIO surface under default-deny MSR policy; implement only the minimal deterministic xAPIC/lAPIC behavior needed; add a `dh-devices` model, `LAPC` DHSNAP section version bump, restore support, state-hash preimage coverage, replay/VerifyReplay coverage, and golden fixtures. Keep `KVM_CREATE_IRQCHIP`, PIT, IOAPIC, kvmclock, and TSC-deadline disabled.
- `crates/dh-devices/src/{bus,ctx,clock,pad,blk,serial,detchannel}.rs`: the Linux guest must see only the deterministic pv device contract. The reference workload expects serial console, block-device access to the read-only game image, pv-clock, pv-pad, pv-entropy, and detchannel readiness before the READY beacon. Add an explicit contract-resolution task before implementation: the cited workload/guest contracts say `kind: virtio-blk` and `/dev/vdb`, while this repo currently exposes deterministic `pv-blk` at `0xD000_4000`. Choose and document one path before implementation: either update/reference the guest image to use a deterministic `pv-blk` Linux driver that presents the game image at `/dev/vdb`, or implement a deterministic virtio-blk subset with full snapshot/hash/replay state. Do not treat existing `PvBlk` as satisfying `virtio-blk` without a compatibility spec and tests.
- `crates/dh-worker/src/image_resolver.rs`: already resolves `ResolvedBoot::BzImage { kernel, initramfs, cmdline }` from content-addressed blobs; confirm size caps, hash verification, and cache error surfaces are sufficient for bzImage/initramfs artifacts.
- `crates/dh-worker/src/service.rs`: `boot_slot` currently returns `UNIMPLEMENTED` for BzImage. `CreateVm` must call the Linux boot path for `ResolvedBoot::BzImage`; `RestoreSnapshot`, `Fork`, `VerifyReplay`, and replay restore paths must not re-run boot, but must rebuild the same deterministic bus/image assets and restore the DHSNAP/MCFG state produced by the Linux root snapshot. Add tests proving a Linux snapshot restores/replays without invoking boot-time initialization a second time.
- `crates/dh-worker/src/proto_map.rs`, `crates/dh-proto/src/lib.rs`, and `proto/hypervisor.proto`: `BootSpec::BzImage` is represented in wire/config mapping. Add conformance and compatibility checks if the task graph discovers missing schema or API coverage.
- `tools/dh-cli/src/{cli,boot,run,gate}.rs` and `tools/dh-cli/tests/`: `dh-cli boot`, `dh-cli run`, and `dh-cli gate` currently assume ELF/nanokernel inputs. Add Linux guest boot/run/gate entry points or flags so operators can run the Phase 1 determinism gate against a bzImage+initramfs workload image. Do this without making `dh-cli` depend on `dh-worker`'s private `image_resolver` module: either accept explicit local `--bzimage`, `--initramfs`, and base-image paths for direct VMM tests, move shared workload/image-cache resolution into a non-worker crate, or drive worker-backed workload-image gates through the existing gRPC ops path. Add tests that pin the chosen resolver seam.
- `tests/determinism/tests/{common,regression,timer_determinism,if0_deferral,landing_precision,m1_acceptance}`: Phase 1 gates currently cold-boot nanokernel guests. Add Linux variants that prove boot-to-boundary determinism, timer/interrupt determinism where applicable, and exact landing/counting behavior against the Linux guest contract.
- `crates/dh-worker/tests/{m5_record_replay,m5_frame_scheduling,m5_net_loopback,m7_fork_verify,replay_engine,restore_engine}`: Phase 2 fork/replay and M4/M5 regression coverage currently use nanokernel fixtures. Add Linux-gated variants or a representative Linux fixture path for snapshot, restore, fork, replay, VerifyReplay, frame scheduling, and record/replay continuity.
- `tests/nanokernel/`: keep as the existing characterization/control baseline. The task graph should not remove nanokernel gates; it should add Linux gates and compare failures against the known nanokernel baseline where useful. Add a separate nanokernel-preservation bead whose acceptance requires existing nanokernel gates to remain green independently of Linux gates, `dh-cli gate` to still default to the nanokernel path, and existing golden fixtures/corpora to remain unchanged unless a dedicated bead updates them.
- `docs/phase-1-exit-gate.md`, `docs/phase-2-exit-gate.md`, `docs/ops/test-partitioning.md`, `docs/ops/github-runner.md`, `docs/decisions/*`, `docs/upstream-divergences.md`, `.github/workflows/ci.yaml`, and `.github/workflows/nightly-drift.yaml`: document Linux guest rerun commands, hardware/operator requirements, accepted divergences from planning docs, and final gate evidence. Docs/ops beads are incomplete unless they update Phase 1 and Phase 2 exit gates with fresh dated Linux and nanokernel evidence; `docs/ops/test-partitioning.md` with exact Linux gate commands and CI/nightly/operator classification; `docs/ops/github-runner.md` with new pinned tools if any; workflows if Linux gates become required or nightly; and `docs/upstream-divergences.md` for accepted drift from the cited `~/.agents/projects/determinism` docs.
- There is no `docs/specs/` or `docs/adr/` directory in this repo. The nearest local architectural context is `docs/decisions/`, `docs/phase-*-exit-gate.md`, `docs/upstream-divergences.md`, and the prior prompts in `docs/prompts/`.

### Success Criteria
The Phase 1 and Phase 2 gates from `~/.agents/projects/determinism/` pass against a Linux guest, not only against the nanokernel guest.

Concretely, the task graph should require:
- A bzImage + initramfs Linux guest boots deterministically through the direct-boot path to the guest-sdk ring-A `Ready` event (`EventKind 14`) drained at the doorbell exit. READY means `CHANNEL_INIT` completed, the agent emitted `Hello`, autostart/control reached `Start{}`, expected regions are live at pinned layout versions, and no host input was consumed before READY. Serial output or another ad hoc marker is not sufficient for the reference-workload path.
- The Phase 1 determinism gate has a named CLI/operator command that runs 100 cold Linux boots to `Ready` with no host-injected input, compares `ready_icount`, `Ready{unit, region_count, manifest_generation}`, `machine_config_hash`, and `state_hash`, then runs a fixed post-READY icount budget plus a deterministic Linux timer/IRQ subgate equivalent to the current `timer-event` gate. Pass means 100/100 zero divergence and no skipped runs.
- The Phase 2 fork/replay gate has a Linux guest mode and passes. Required evidence: Linux variants of the M5 record/replay corpus and the M7 fork/VerifyReplay acceptance; replay verifies every `EPOCH_HASH` and END `state_hash`; full acceptance runs 1000 fork children with 1000/1000 `VerifyDone` and zero `Divergence`; cross-slot rerun samples same-seed children and requires identical child snapshot refs; nightly coverage includes a 100-child Linux canary plus Linux corpus reverify. Document exact commands/env vars, and forbid `*_ALLOW_SKIP=1` for acceptance evidence.
- M9 acceptance from the hypervisor implementation plan is satisfied: two Linux boots produce equal hashes at READY, and the relevant M4/M5 regression tests pass on the Linux guest too.
- Existing nanokernel Phase 1/2 gates remain green as regression coverage. Required evidence includes `cargo test --workspace`, `cargo run -p dh-cli -- gate --runs 100`, the current Phase 1 determinism tests, the M5 record/replay corpus reverify, and the documented M7 operator commands.

### Constraints
N/A

---

## Your Task

Analyze this codebase change and create a comprehensive **Beads task graph** using the `bd` CLI. Beads provides dependency-aware, conflict-free task management for multi-agent execution.

Before creating the task graph, you MUST first analyze the affected areas of the codebase:

1. Check `docs/specs/` and `docs/adr/` for existing architectural decisions
2. Examine the directory/module structure of the affected areas listed above
3. Identify key interfaces, APIs, and integration points that must be preserved
4. Note existing test patterns and coverage in the affected areas
5. Assess risk areas where changes could break existing functionality

Use your analysis to make each bead specific - reference actual file paths, module names, and patterns you observed.

Then generate a shell script that creates the complete task graph.

**IMPORTANT: Your ONLY deliverable is a bash shell script with `bd create` commands. Not an implementation plan. Not a design document. Not a code review. A runnable `.sh` script.**

---

## Output Format

Generate a shell script that creates the full task graph. The script should:

1. **Initialize Beads** (if not already initialized)
2. **Create all beads** with appropriate priorities
3. **Establish dependencies** between beads
4. **Add labels** for phase grouping
5. Include mechanically checkable acceptance criteria for every bead

Every `bd create` must include `--type`, `--priority`, `--labels`, `--description`, and `--acceptance`; include `--notes` when file reservations, coordination constraints, or external docs matter. Acceptance must be mechanically checkable: exact commands, required env vars, expected pass counts, hash/ref equality conditions, zero-divergence requirements, and whether the gate is CI, nightly, or operator-run. Do not encode acceptance only in the title.

### Example Output

```bash
#!/bin/bash
# Project: determinism-hypervisor
# Change: M9 minimal-Linux guest path
# Generated: 2026-06-18

set -e

# Initialize beads if needed
if [ ! -d ".beads" ]; then
    bd init
fi

echo "Creating change beads..."

# ========================================
# Phase 1: Analysis & Preparation
# ========================================

ANALYZE_BOOT=$(bd create "Analyze current ELF-only boot path and Linux direct-boot requirements" \
  --type task \
  --priority 0 \
  --labels analysis \
  --description "Read crates/dh-vmm/src/boot.rs, crates/dh-worker/src/service.rs, and the cited M9/reference-workload docs. Document the exact bzImage setup-header, boot params, cmdline, initramfs, and entry-state requirements this repo must implement." \
  --acceptance "Notes identify the current ELF-only entry points, the Linux direct-boot state to add, the canonical cmdline baseline, and all files that later implementation beads must reserve." \
  --notes "Reservations: crates/dh-vmm/src/boot.rs, crates/dh-worker/src/service.rs, docs/upstream-divergences.md" \
  --silent)

RECONCILE_CONTRACT=$(bd create "Reconcile Linux block-device and READY contracts with sibling docs" \
  --type decision \
  --priority 0 \
  --labels analysis \
  --description "Resolve reference-workload's linux-direct /dev/vdb and guest-sdk Ready EventKind 14 contracts against this repo's pv-blk, detchannel, worker, and proto surfaces before implementation begins." \
  --acceptance "Decision records whether M9 uses a deterministic pv-blk Linux driver or implements a deterministic virtio-blk subset, and pins Ready EventKind 14 as the only accepted Linux readiness point." \
  --notes "Reservations: docs/upstream-divergences.md, docs/decisions/*, crates/dh-devices/src/blk.rs, crates/dh-devices/src/detchannel.rs" \
  --silent)

CHAR_NANOKERNEL=$(bd create "Preserve existing nanokernel Phase 1/2 gates as M9 characterization baseline" \
  --type task \
  --priority 0 \
  --labels prep,testing \
  --description "Pin the current nanokernel gates before Linux work so regressions are separable from new Linux failures." \
  --acceptance "cargo test --workspace and cargo run -p dh-cli -- gate --runs 100 remain green; dh-cli gate still defaults to the nanokernel path; no existing golden fixture or corpus changes without a dedicated bead." \
  --notes "Reservations: tools/dh-cli/src/gate.rs, tests/determinism/**, tests/nanokernel/**" \
  --silent)
bd dep add $CHAR_NANOKERNEL $ANALYZE_BOOT

# ========================================
# Phase 2: Core Implementation
# ========================================

IMPL_BZIMAGE=$(bd create "Implement deterministic bzImage setup-header parsing and boot params layout" \
  --type feature \
  --priority 0 \
  --labels impl \
  --description "Add the first Linux direct-boot slice in crates/dh-vmm/src/boot.rs: setup-header validation, boot params/zero page, e820 map, cmdline placement, initramfs placement, and explicit rejection of unsupported protocol features." \
  --acceptance "Host-runnable parser/layout tests cover valid and malformed bzImage headers; MCFG/hash preimage tests prove the canonical cmdline bytes equal the actual boot cmdline bytes." \
  --notes "Reservations: crates/dh-vmm/src/boot.rs, crates/dh-vmm/src/config.rs" \
  --silent)
bd dep add $IMPL_BZIMAGE $ANALYZE_BOOT
bd dep add $IMPL_BZIMAGE $RECONCILE_CONTRACT

echo "Bead graph created."
```

---

## Bead Creation Guidelines

### Priority Levels
- `--priority 0` = Critical (blocking other work, or high-risk changes needing early validation)
- `--priority 1` = High (important implementation work)
- `--priority 2` = Medium (standard work)
- `--priority 3` = Low (cleanup, nice-to-haves)

### Labels (Phase Grouping)
Use `--labels` to group beads by phase:
- `analysis` - Understanding current state
- `prep` - Preparation work (characterization tests, feature flags, scaffolding)
- `impl` - Core implementation
- `testing` - Test coverage
- `migration` - Data/code migration
- `docs` - Documentation updates
- `cleanup` - Post-rollout cleanup

### Dependency Rules
1. Never create cycles
2. Analysis tasks should complete before implementation begins
3. Characterization tests should exist before changing code
4. Use `bd dep add CHILD PARENT` (child depends on parent completing first)
5. Parallel work should share a common ancestor, not depend on each other

### Task Granularity
- Each bead should be completable in **under 750 lines of code changed**
- Tasks should be atomic enough for one agent to complete without coordination
- If a task requires multiple file areas, consider splitting by file area
- Do not create one broad "implement bzImage boot" bead. Split at minimum into: bzImage setup-header parser/validation; boot params/e820/cmdline/initramfs layout; Linux entry state/MSR/CPUID/lAPIC-stub work; worker `CreateVm`/image-resolver integration; Linux device/runtime compatibility; CLI gate UX; Phase 1 Linux tests; Phase 2 Linux corpus/fork/replay tests; docs/ops/CI updates.

---

## Change-Specific Considerations

### For New Features
- Start with analysis of similar existing features
- Consider feature flag for gradual rollout
- Plan for A/B testing if relevant
- Include documentation and changelog updates

### For Refactors
- Add characterization tests first (capture current behavior)
- Consider strangler fig pattern for large changes
- Plan incremental migration path
- Ensure no behavior changes unless intentional

### For Migrations
- Create rollback plan as an explicit task
- Plan data validation checkpoints
- Consider dual-write period if applicable
- Include monitoring/alerting tasks

### For Performance Changes
- Add benchmarks before and after
- Include load testing tasks
- Plan gradual rollout with monitoring
- Have rollback criteria defined

---

## File Reservation Planning

For each major work area, note the file patterns that will need exclusive reservation:

```bash
# Example reservation notes (add as bead descriptions)
# CAUTION: These files have many consumers
# Auth refactor: src/auth/**, tests/auth/** (coordinate with API team)
# Shared utils: src/lib/utils.ts (high contention - keep changes minimal)
```

This helps agents claim appropriate file surfaces when they start work.

Each bead that edits files must put `Reservations:` in `--notes` with exact file globs. Overlapping reservations must be serialized with `bd dep add` or marked same-agent/serial in notes.

---

## Verification Steps

After generating the script:

1. Validate the generated script with `bash -n <script>`.
2. Do not run the generated script as part of this planning prompt. The script text is the sole deliverable; it must not mutate `.beads/`, create issues, chmod files, or run `bd ready`. A human/operator runs it later.

---

## Completeness Checklist

Ensure your task graph includes:

- [ ] Analysis of current implementation in affected areas
- [ ] Characterization tests for existing behavior
- [ ] Feature flag or gradual rollout mechanism (if applicable)
- [ ] Core implementation broken into small units
- [ ] Unit tests for new/changed code
- [ ] Integration tests for affected workflows
- [ ] Regression testing plan
- [ ] Documentation updates
- [ ] Migration scripts (if data changes)
- [ ] Rollback plan
- [ ] Cleanup tasks for post-rollout
- [ ] Clear dependency chains with no cycles
