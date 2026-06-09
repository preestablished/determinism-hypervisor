# Project Planning with Beads

## Agent Instructions

You are an expert software architect creating a comprehensive task breakdown. This task graph will be executed by AI agents working in parallel, coordinated through MCP Agent Mail with file reservations to prevent conflicts.

<quality_expectations>
Create a thorough, production-ready task graph. Include all necessary setup, implementation, testing, and documentation tasks. Go beyond the basics - consider edge cases, error handling, security considerations, and integration points. Each task should be specific enough for an agent to execute independently without ambiguity.
</quality_expectations>

## Project Information

### Links to Relevant Documentation

Local copies live in `.agents/docs/` (read `MAP.md` first — it includes the clean-room source boundary, which is normative):

- `.agents/docs/MAP.md` — system map for the whole determinism platform + clean-room source boundary
- `.agents/docs/determinism-hypervisor/README.md` — service overview
- `.agents/docs/determinism-hypervisor/ARCHITECTURE.md` — architecture (pv devices §6, detclock, capture §6.10)
- `.agents/docs/determinism-hypervisor/API.md` — worker gRPC API surface
- `.agents/docs/determinism-hypervisor/INTEGRATION.md` — how every sister service touches this one
- `.agents/docs/determinism-hypervisor/IMPLEMENTATION-PLAN.md` — milestone-by-milestone plan (M1–M3 are this phase)
- `.agents/docs/guest-sdk/` — the in-VM peer: detchannel + pv-device contracts hypervisor M1 must implement against (README, ARCHITECTURE, API, INTEGRATION, IMPLEMENTATION-PLAN)
- `.agents/docs/phases/phase-0-bootstrap.md` — entry-gate context (assumed already passed)
- `.agents/docs/phases/phase-1-deterministic-execution.md` — this phase's scope and exit gates
- `.agents/docs/phases/` — later phases, for roadmap context only (out of scope here)

### Project Description

**Phase 1 of `determinism-hypervisor` — Deterministic Execution (single timeline).** Boot a guest on the Intel box and run it bit-deterministically: the same boot + the same configuration, executed twice, produces an identical chained state hash at the same retired-instruction count. Three milestones on the critical path:

- **M1** — nanokernel guest + pv devices + boot from image.
- **M2** — `detclock`: retired-instruction counting + exact event landing (PMI kick at target−8192 skid margin, single-step refinement). *Depends on M1.*
- **M3** — virtual time, deterministic injection, run control (run-until icount / virtual-ns / event; pause at deterministic boundary). *Depends on M2.*

This is the riskiest phase in the program: if instruction-precise determinism on KVM can't be made to work, nothing downstream matters. Front-load it. Scope is this repo only — the snapshot-store, guest-sdk, and reference-workload Phase 1 tracks live in their own repos. No forking, no real workload yet — one timeline, perfectly repeatable.

### Technical Stack

- Rust (2021 edition), Cargo workspace of `dh-*` crates already scaffolded: `dh-types`, `dh-kvm`, `dh-vmm`, `dh-inputlog`, `dh-snapshot`, `dh-proto`, `dh-worker`, `dh-smoke`
- KVM via `/dev/kvm` ioctls on a pinned-kernel Intel Linux box (Phase 0 preflight passed: pinned kernel, perf_event access, KVM caps)
- `perf_event` retired-instruction counter with PMI for event landing
- gRPC/protobuf worker API; `determinism-proto` is a path dependency on the sibling `../control-plane` repo
- Development happens on macOS; determinism and landing-precision gates run on the Intel box

### Specific Requirements

**Exit gates (executable, per the phase doc and IMPLEMENTATION-PLAN.md):**

- **Determinism gate:** boot the nanokernel guest, run to icount N twice → identical chained state hash; repeat with an injected timer event at an exact icount → identical hashes. 100 consecutive runs, zero divergence.
- **Landing-precision gate:** events land at the requested retired-instruction count exactly (M2 acceptance: 10,000 random targets in a 100M-instruction loop, `icount == N`, RIP at instruction start, zero overshoots), including across REP-string instruction boundaries. Skid histogram exported; measured max skid < skid_margin/2.
- **M3 acceptance items the task graph must encode** (IMPLEMENTATION-PLAN.md M3): the IF=0 deferral test (timer lands while interrupts are masked; delivery defers identically across runs); the first **CI-required determinism regression test** (run nanokernel 1e9 instructions twice from cold boot with fixed seed, final state hash equal — required-for-merge from M3 onward); and the guest-TSC alignment benchmark (per-entry `KVM_SET_MSRS{IA32_TSC}` writes vs the `KVM_VCPU_TSC_CTRL` offset attribute — pick one with measured numbers **before** M4 freezes restore behavior).
- Events must land at exact instruction boundaries; pauses only at deterministic boundaries.

**Repo-state corrections the generator must account for:**

- **Crate layout:** ARCHITECTURE.md §1 is the **normative** layout — `dh-detclock`, `dh-devices`, `dh-verify`, `tools/dh-cli`, `tests/nanokernel/` — and the scaffolded workspace (`dh-types`, `dh-kvm`, `dh-smoke`, …) does not match it (those three crates appear nowhere in ARCH §1). Include an early setup task that reconciles the workspace to ARCH §1; M2's deliverable is literally the `dh-detclock` crate.
- **M0 is not actually complete.** Despite Phase 0 being nominally passed, the crates are empty stubs and M0's acceptance artifacts do not exist: the real-mode→long-mode boot stub, `dh-cli boot tests/nanokernel/hello.elf`, and the `dh-workerd --preflight` checker (ARCH §7.4 host config + full §2.1 capability table). Create M0-completion beads as explicit predecessors of M1 work.
- **`detguest-host` (decision made):** hypervisor M1's detchannel host side links guest-sdk's `detguest-host` crate via a **path dependency on the sibling `../guest-sdk` repo**, the same pattern as the existing `determinism-proto = { path = "../control-plane/..." }` dep. M1 detchannel beads therefore carry a cross-repo dependency on guest-sdk Milestone 1 (which has zero hypervisor dependency and proceeds in parallel); the channel spec is in `.agents/docs/guest-sdk/`.
- **Nanokernel build pipeline is a required task category:** every M1–M3 acceptance test runs nanokernel guest programs (`tests/nanokernel/`). The graph needs beads for the guest test-program toolchain/build pipeline, the test programs themselves (device-exercise program, 1,000-instruction counting sequence with REP MOVS/CPUID/MMIO exits, 100M-instruction landing loop, timer-arming program), and read-only base image + CoW overlay production.
- **Phase-1 state hash:** the determinism gate and the M3 CI test require a state hash, but the full "state hash chain + XSAVE canonicalization" is an M4 deliverable (excluded by the sequencing guard below). Include an explicit Phase-1 bead for a minimal final-state hash (per ARCH §8.5, scoped to what M3's run-twice-compare needs) and name the owning crate; full chain/canonicalization stays in M4.

**Infrastructure and risk constraints:**

- **CI:** determinism jobs require a KVM-capable **self-hosted runner labeled `kvm-intel`** plus the host-pinning lock file `ci/determinism-class.lock` (nightly fails on host drift); the current `.github/workflows/ci.yaml` is ubuntu-latest-only. Unit-layer tests (codecs, agenda math, vns rational math) must build and run on any host **including aarch64**. The graph needs beads for runner registration, workflow split (host-runnable vs `kvm-intel`-gated), the lock file + re-baseline procedure, and wiring the M3 determinism regression as required-for-merge.
- **Test partitioning:** every bead's acceptance criteria must state whether it is host-runnable (any machine, incl. macOS/aarch64 dev hosts) or hardware-gated (`kvm-intel` runner / Intel box only) — agents must know which gates they can verify locally.
- **M2 host-config + counter-fallback (IMPLEMENTATION-PLAN R2, ARCH §7.4):** M2 beads must carry the host-config requirements that gate skid acceptance (isolcpus/nohz_full/rcu_nocbs, NMI watchdog off — it consumes a PMU counter, THP off, `kvm_intel` module params) and document the fallback counter: switch `dh-detclock` to retired conditional branches (`BR_INST_RETIRED.COND`/`.NEAR_TAKEN`) with the `(count, RIP, RCX)` boundary tuple if INST_RETIRED semantics fail empirics.
- **Clean-room source boundary** (normative, see `MAP.md`): implementation agents use only this project's docs, public hardware/OS/KVM/Rust/crate documentation, and operator-supplied artifacts. If a requirement can't be met from the allowed source set, stop and file a documentation issue.
- **Sequencing guard:** do not start hypervisor M4 (snapshots) until the determinism gate is green — snapshotting a nondeterministic VM produces unfalsifiable bugs.

**Parallelism guidance for the graph:**

- The M1→M2→M3 spine is serial, but flag intra-milestone seams: the five pv device models (clock/pad/entropy/blk+overlay/serial) split by file area; host-runnable M3 math (agenda/scheduler, vns rational arithmetic) can start before M2 lands; harness scaffolding is independent.
- The exit-gate tooling is a measured deliverable with a named home: the 100-run determinism harness and the skid-histogram/landing harness (10,000 random targets) belong in dedicated beads (e.g. under `dh-verify` / `tools/dh-cli` per ARCH §1 — do not leave them implicit).

---

## Your Task

Analyze this project and create a comprehensive **Beads task graph** using the `bd` CLI. Beads provides dependency-aware, conflict-free task management for multi-agent execution.

---

<critical_constraint>
Your ONLY output is a bash shell script. Do NOT use `bd add` — the correct command to create a bead is `bd create`. Use `bd dep add` for dependencies. Do not implement anything yourself.
</critical_constraint>

## Output Format

Generate a shell script that creates the full task graph. The script should:

1. **Initialize Beads** (if not already initialized)
2. **Create all beads** with appropriate priorities
3. **Establish dependencies** between beads
4. **Add labels** for phase grouping

### Example Output

```bash
#!/bin/bash
# Project: determinism-hypervisor
# Generated: 2026-06-09

set -e

# Initialize beads if needed
if [ ! -d ".beads" ]; then
    bd init
fi

echo "Creating project beads..."

# ========================================
# M0 completion: workspace reconciliation + KVM smoke
# ========================================

CRATE_LAYOUT=$(bd create "Reconcile workspace to ARCHITECTURE §1 crate layout" \
  -d "Rename/restructure crates to dh-detclock, dh-devices, dh-verify, tools/dh-cli, tests/nanokernel per ARCH §1. Decide disposition of dh-types/dh-kvm/dh-smoke." \
  -p 0 --label m0-bootstrap --silent)

BOOT_STUB=$(bd create "Boot real-mode→long-mode stub via dh-cli boot" \
  -d "20-line stub writes to debug-serial and HLTs; dh-cli boot tests/nanokernel/hello.elf prints expected bytes. Hardware-gated: kvm-intel runner." \
  -p 0 --label m0-bootstrap --silent)
bd dep add $BOOT_STUB $CRATE_LAYOUT

PREFLIGHT=$(bd create "Implement dh-workerd --preflight checker" \
  -d "ARCH §7.4 host config + full §2.1 capability table incl. KVM_CAP_X86_USER_SPACE_MSR with KVM_MSR_EXIT_REASON_FILTER; fails loudly on stock kernel." \
  -p 0 --label m0-bootstrap --silent)
bd dep add $PREFLIGHT $CRATE_LAYOUT

# ========================================
# M1: nanokernel guest + pv devices + boot from image
# ========================================

ELF_BOOT=$(bd create "ELF boot path + MachineConfig plumbing" -p 0 --label m1-devices --silent)
bd dep add $ELF_BOOT $BOOT_STUB

PV_CLOCK=$(bd create "pv-clock device model in dh-devices" -p 0 --label m1-devices --silent)
bd dep add $PV_CLOCK $ELF_BOOT

PV_BLK=$(bd create "pv-blk + CoW overlay (base image byte-unchanged)" -p 0 --label m1-devices --silent)
bd dep add $PV_BLK $ELF_BOOT

# ... parallel beads for pv-pad, pv-entropy, serial, detchannel host side (path dep on ../guest-sdk),
# CPUID mask + dh-cli cpuid-diff, nanokernel device-exercise program ...

# ========================================
# M2: detclock — counting + exact landing
# ========================================

DETCLOCK=$(bd create "dh-detclock perf counter (guest-only, pinned) + PMI kick" \
  -d "Skid margin target−8192, single-step refinement, REP rule. Fallback documented: BR_INST_RETIRED.COND with (count, RIP, RCX) tuple per R2." \
  -p 0 --label m2-detclock --silent)
bd dep add $DETCLOCK $PV_CLOCK

# ... counting_semantics test, 10k-target landing test, skid histogram ...

echo ""
echo "Bead graph created! View with:"
echo "  bd ready              # List unblocked tasks"
```

---

## Bead Creation Guidelines

### Priority Levels
- `-p 0` = Critical (blocking other work)
- `-p 1` = High (important but not blocking)
- `-p 2` = Medium (standard work)
- `-p 3` = Low (nice to have)

### Labels (Phase Grouping)
Use `--label` to group beads by milestone/track:
- `m0-bootstrap` - workspace reconciliation, boot stub, preflight checker
- `m1-devices` - ELF boot path, pv device models, detchannel host side, CPUID mask
- `m2-detclock` - perf counter, PMI kick, boundary engine, landing tests
- `m3-runctl` - virtual time, agenda/scheduler, deterministic injection, run control
- `nanokernel` - guest test-program build pipeline and test programs
- `determinism-ci` - kvm-intel runner, determinism-class.lock, regression jobs
- `testing` - test coverage not tied to a single milestone
- `docs` - documentation

### Dependency Rules
1. Never create cycles
2. Every bead should have a clear dependency chain back to setup tasks
3. Use `bd dep add CHILD PARENT` (child depends on parent completing first)
4. Parallel work should share a common ancestor, not depend on each other

### Task Granularity
- Each bead should be completable in **under 750 lines of code**
- Tasks should be atomic enough for one agent to complete without coordination
- If a task requires multiple file areas, consider splitting by file area

---

## File Reservation Planning

For each major work area, note the file patterns that will need exclusive reservation:

```bash
# Example reservation notes (add as bead descriptions)
# detclock work: crates/dh-detclock/**
# device models: crates/dh-devices/src/{clock,pad,entropy,blk,serial}*, one device per bead
# VMM core / run control: crates/dh-vmm/**
# nanokernel test programs: tests/nanokernel/**
# CLI tooling: tools/dh-cli/**
# verification harnesses: crates/dh-verify/**
# CI: .github/workflows/**, ci/determinism-class.lock
```

This helps agents claim appropriate file surfaces when they start work.

---

## Context Documentation

All authoritative context already lives in `.agents/docs/` (see Links section above): the system map (`MAP.md`, read first — clean-room boundary is normative), the hypervisor's ARCHITECTURE/API/INTEGRATION/IMPLEMENTATION-PLAN, the guest-sdk channel contracts, and the phase roadmap. Bead descriptions should cite the specific doc section (e.g. "ARCH §3.1", "IMPLEMENTATION-PLAN M2") that grounds each task.

---

## Verification Steps

After generating the script:

1. **Run it**: `chmod +x setup-beads.sh && ./setup-beads.sh`
2. **Check ready work**: `bd ready` should show initial setup tasks

---

## Completeness Checklist

Ensure your task graph includes:

- [ ] All setup and configuration tasks
- [ ] Core architecture and shared utilities
- [ ] Feature implementation tasks (broken into small units)
- [ ] Error handling and edge cases
- [ ] Unit and integration tests for each feature
- [ ] API documentation
- [ ] Security considerations (input validation, auth checks)
- [ ] Performance considerations where relevant
- [ ] CI/CD and deployment tasks
- [ ] Clear dependency chains with no cycles
