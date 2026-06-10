# Big Change Planning with Beads

## Agent Instructions

You are an expert software architect creating a comprehensive task breakdown for a change to an existing codebase. This task graph will be executed by AI agents working in parallel, coordinated through MCP Agent Mail with file reservations to prevent conflicts.

<quality_expectations>
Create a thorough, production-ready task graph. Include all necessary analysis, preparation, implementation, testing, and documentation tasks. Go beyond the basics — consider edge cases, error handling, security considerations, backwards compatibility, and integration points. Each task should be specific enough for an agent to execute independently without ambiguity.
</quality_expectations>

<critical_constraint>
You must NOT implement any of the changes yourself. Your ONLY output is a bash shell script containing `bd create` and `bd dep add` commands. Do NOT use `bd add` — the correct command is `bd create`. Do not write code. Do not create files other than the shell script. Do not modify existing files. Read and analyze the codebase, then produce the script.
</critical_constraint>

## Change Information

### Change Type
NEW_FEATURE

### Description

**Phase 2 of `determinism-hypervisor` — Fork & Replay (the timeline tree exists).**
Implement the M4→M7 integration chain so the platform can snapshot a running guest,
restore it, fork it into many divergent children, and replay any (snapshot, input log)
pair to a bit-identical result, with the hypervisor reachable over gRPC as a worker
daemon. The phase ends at **Platform Milestone 1** (MAP.md build-order step 1): fork a
guest 1000× and verify bit-identical re-execution.

The four milestones (full definitions: `.agents/docs/determinism-hypervisor/IMPLEMENTATION-PLAN.md` M4–M7):

1. **M4 — Snapshot / restore / fork + snapshot-store integration.** Dirty-ring
   harvest, DHSNAP codec (golden-bytes tests), XSAVE canonicalization, state hash
   chain, TakeSnapshot/RestoreSnapshot against a live snapshot-store, tier-A CoW fork
   (memfd-sealed frozen parents, `F_SEAL_FUTURE_WRITE`), tier-B mmap restore.
   *Depends on snapshot-store M4 (gRPC surface + client lib) — assumed complete
   before this graph executes (see Constraints); integrate via the gRPC surface
   first, then switch to the M5 UDS SEQPACKET fast path when it lands (the phase
   doc's staged adoption).*
2. **M5 — Input log (DHILOG v1) + replay.** Full codec (golden bytes + `cargo fuzz`
   target), recording during runs, PAD_SET/DEV_EVENT/NET_RX landing, AUX records,
   sealing, replay path, log concatenation. *Depends on M4.* NET_RX landing implies
   the **pv-net loopback device** (ARCH §6.7, MMIO `0xD000_5000`), which does not
   yet exist in `crates/dh-devices` — building it (plus its DHSNAP device section
   and a nanokernel net-loopback test program) is in M5 scope.
3. **M6 — Worker daemon (gRPC :7400) + introspection + capture engine.**
   `dh-workerd`: slot manager, leases, full proto surface (API.md §2),
   ReadGuestMemory / GetFramebuffer / StreamGuestEvents, /healthz + metrics,
   WatchSlots, and the capture engine (ARCH §6.10 — lands here, exercised against the
   real guest-sdk region manifest in Phase 3). *Depends on M5.*
4. **M7 — Platform Milestone 1: fork 1000× + verified re-execution.** Boot guest →
   root snapshot → 1000 forks batched across slots, each runs a distinct seeded
   1-guest-second random pad burst, TakeSnapshot each, VerifyReplay each
   (snapshot, log) pair. *Depends on M6.*

**Current repo state (verified 2026-06-10 — ground every bead against this):**

- Phase 1 (M0–M3) is complete and signed off (`docs/phase-1-exit-gate.md`, bead dk1).
  The determinism gate is green — the sequencing precondition for M4 holds.
- `crates/dh-snapshot/src/lib.rs` is a 9-line stub: the entire DHSNAP codec,
  dirty-page tracking integration, and snapshot orchestration are greenfield.
- `crates/dh-vmm/src/hash.rs` already provides `StateHashChain`
  (`push_link`/`push_final_link`) and `canonical_vcpu_blob` from the Phase-1 minimal
  hash work — M4 extends this seam rather than recreating it. `dh-vmm` also already
  has `boundary.rs`, `agenda.rs`, `inject.rs`, `runctl.rs`, `tsc.rs`, `msr.rs`,
  `cpuid.rs`, `blkfile.rs` (CoW overlay), `kvm.rs`.
- The TSC alignment decision is already made and documented in
  `docs/decisions/tsc-alignment.md` (the M3 benchmark deliverable) — M4 restore
  behavior must follow it, not reopen it.
- `crates/dh-inputlog/src/dhilog.rs` is a 537-line partial DHILOG codec from
  Phase 1. M5 extends it to the full v1 spec (API.md §3): replay, sealing,
  concatenation, fuzz target, golden-bytes fixtures.
- `crates/dh-worker/` has a 356-line preflight checker and a 24-line `dh-workerd`
  binary stub. Slot manager, leases, gRPC service, metrics are greenfield.
- `crates/dh-detclock`, `crates/dh-devices` (all five pv devices + detchannel host
  side), `tests/nanokernel` (guest program toolchain), `tests/determinism`
  (regression harness), and `ci/determinism-class.lock` + `ci/check-determinism-class.sh`
  exist from Phase 1.
- **Proto seam:** `proto/hypervisor.proto` is an empty placeholder service. The
  canonical schema home is the sibling repo's `../control-plane/crates/determinism-proto`
  crate (workspace path dep, `hypervisor` feature), but that crate currently contains
  only hand-written placeholder structs (`SnapshotRef`, `Lease`, `KvmCaps`) — **no
  tonic/prost codegen exists anywhere yet**. M6 requires the full API.md §2 surface.
  The graph needs an early analysis/decision bead: establish real protobuf codegen in
  determinism-proto (cross-repo edits to `../control-plane` are in scope and allowed)
  vs. promoting this repo's `proto/hypervisor.proto`, then make `dh-proto` re-export
  the result. Do not let M6 implementation beads start before this seam is settled.
- **Sibling repos are present on the execution host** at `../control-plane`,
  `../guest-sdk`, `../snapshot-store` (the workspace already has path deps on the
  first two; M4 integrates via `../snapshot-store/crates/snapstore-client`, same
  path-dep pattern). Note: as of 2026-06-10 `snapstore-client` is itself a 7-line
  stub — the store's M4/M5 are concurrent work in its own repo (assumed complete
  before this graph executes). The graph must therefore open the M4
  store-integration track with an explicit **readiness-verification bead**:
  cargo-check the path dep and confirm the client exposes the surface in
  `.agents/docs/snapshot-store/API.md`; if absent, halt that track and file an
  issue. Host-runnable M4 work (DHSNAP codec, golden bytes, XSAVE canonicalization,
  dirty-ring plumbing) does not block on it. Likewise, as of 2026-06-10 the
  workspace path dep `../guest-sdk/crates/detguest-host` is **absent** (guest-sdk
  currently ships only `detguest-agent`/`detguest-wire`/`m0-proto-client`), so
  `cargo` cannot load this workspace's manifest on the dev host until guest-sdk
  restores that crate — the graph's first analysis bead must verify workspace
  buildability and file/track the cross-repo gap if it persists.

**Key risks the graph must encode as explicit test/mitigation beads** (full table:
IMPLEMENTATION-PLAN.md §Key risks): R7 XSAVE byte-instability (canonicalization on
both snapshot and hash paths + fault-injection coverage), R8 dirty-page tracking
misses (THP off, ring-full chaos test, roundtrip catches misses by construction,
`--paranoid-hash` audit mode), R9 CoW fork aliasing (`F_SEAL_FUTURE_WRITE` on frozen
parents, software-enforced `Frozen` slot-state machine, per-slot KVM fds), R12
snapshot-store coupling (joint integration tests against the real store, never a
mock; refs returned only after durability).

### Links to Relevant Documentation

All references are **repo-local** under `.agents/docs/` (synced 2026-06-10 from the
upstream planning repo; the upstream `~/.agents/projects/determinism/` tree is **not
available on the execution host** — never reference it). Read `MAP.md` first; it
includes the clean-room source boundary, which is normative.

- `.agents/docs/MAP.md` — system map + clean-room source boundary + build-order
  milestones (Platform Milestone 1 = this phase's M7)
- `.agents/docs/phases/phase-2-fork-and-replay.md` — this phase's scope, cross-repo
  ordering, and exit gate
- `.agents/docs/determinism-hypervisor/IMPLEMENTATION-PLAN.md` — M4–M7 acceptance
  criteria (the normative source for bead acceptance), testing strategy table, risk
  table
- `.agents/docs/determinism-hypervisor/ARCHITECTURE.md` — mechanisms: pv devices
  incl. the pv-net loopback (§6, §6.7), capture engine (§6.10), preflight/host
  config (§7.4), snapshot/restore/fork + MSR/XSAVE/hash rules (§8: contents §8.1,
  dirty-page tracking §8.2, restore §8.3, fork §8.4, state hash §8.5), metrics
  series (§9), perf budgets (§10)
- `.agents/docs/determinism-hypervisor/API.md` — worker gRPC surface (§2,
  `DeterminismClass` §2.8), DHILOG v1 records (§3), DHSNAP container + ENTR state
  (§4), snapshot-manifest interchange with snapshot-store incl. determinism-class
  fields (§5)
- `.agents/docs/determinism-hypervisor/INTEGRATION.md` — snapshot-store wiring, slot
  leasing protocol, canonical exploration-step sequence, VerifyReplay model,
  guest-sdk contract obligations
- `.agents/docs/snapshot-store/` — README, API.md (gRPC + page-channel protocol),
  ARCHITECTURE.md (fast path), INTEGRATION.md, IMPLEMENTATION-PLAN.md — the service
  M4 integrates against (assumed implemented; see Constraints)
- `.agents/docs/guest-sdk/ARCHITECTURE.md` + `API.md` — detchannel layout, FRAME_MARK
  consistency rule, pv-device contracts (cited by the DEV_EVENT recording work)
- `docs/decisions/tsc-alignment.md` — settled TSC restore mechanism (do not reopen)
- `docs/phase-1-exit-gate.md` — what is already proven and signed off

### Affected Areas

- `crates/dh-snapshot` — DHSNAP codec, XSAVE canonicalization, dirty-page harvest
  orchestration, tier-A/tier-B snapshot/restore/fork engine (greenfield; primary M4
  surface)
- `crates/dh-vmm` — dirty-ring plumbing, memfd-backed guest memory + sealing, fork
  slot mechanics, restore path, `hash.rs` chain extension (M4)
- `crates/dh-inputlog` — DHILOG v1 completion: replay, sealing, concatenation, fuzz,
  golden bytes (M5)
- `crates/dh-devices` — canonical DEV_EVENT recording of every host-side channel
  mutation; entropy `{seed, stream, word_pos}` snapshot state (M4/M5); **new pv-net
  loopback device** per ARCH §6.7 (M5 — NET_RX/NET_TX landing has no other
  producer/consumer)
- `crates/dh-detclock` — replay-side landing reuse; no new counter work expected
- `crates/dh-proto` + `proto/hypervisor.proto` + `../control-plane/crates/determinism-proto`
  — full v1 gRPC schema + codegen (M6; cross-repo seam decision)
- `crates/dh-worker` — slot manager, leases, gRPC service on :7400, introspection
  RPCs, capture engine, /healthz + metrics on :7401, WatchSlots (M6); extend the
  existing preflight checker (`preflight.rs`) for new M4/M6 host requirements
  (dirty-ring capability, memfd sealing, THP-off/`MADV_NOHUGEPAGE` per ARCH §7.4)
- `crates/dh-verify` — VerifyReplay execution path (epoch-hash comparison, VerifyDone
  / Divergence reporting; bisection itself is M8, out of scope), M7 gate harness
- `tests/nanokernel` — new guest programs: pad-echo (M5), fake-frame emitter with
  FRAME_MARK across snapshot/restore (M5), entropy-draw program (M4 ENTR golden),
  net-loopback program (M5 NET_RX landing), and an M6 capture fixture: a program
  that publishes a minimal detchannel region manifest with a FRAMEBUFFER-flagged
  region and a bumpable `layout_version` (the capture-neutrality C5 and
  `FAILED_PRECONDITION` C2 tests have no other manifest producer until guest-sdk
  arrives in Phase 3)
- `tests/determinism` — snapshot-transparency, fork-transparency, ring-full chaos,
  record/replay corpus
- `ci/` + `.github/workflows/` — nightly fuzz job, criterion perf gates (>20%
  regression fails), nightly 100-fork verify, record/replay corpus re-verification,
  chaos jobs; **workflow edits**: both CI lanes currently check out only the
  `control-plane` and `guest-sdk` siblings — the M4 path dep adds a required
  third checkout of `snapshot-store`; the R12 "never a mock" joint tests also need
  a decision + implementation for running a real `snapstore-server` instance on the
  `kvm-intel` runner (build-and-spawn test fixture vs box provisioning), and the
  runner needs new tools provisioned (grpcurl, stress-ng, cargo-fuzz nightly
  toolchain, protoc for tonic codegen — document in `docs/ops/`)
- `tools/dh-cli` — operator verbs for snapshot/restore/fork/replay debugging
- `.agents/docs/` — already synced as part of planning (no bead needed unless drift
  is found)

### Success Criteria

Phase 2 exit gate (`.agents/docs/phases/phase-2-fork-and-replay.md`), scoped to this
repo:

1. **Platform Milestone 1:** from one mid-boot snapshot, fork 1000 children with
   distinct input logs; every child re-executed from (root snapshot + spliced log)
   reproduces its recorded chained state hash exactly. Zero divergences.
2. Fork latency and snapshot-commit latency within budget: hypervisor tier-A fork
   < 10 ms p50 (the snapshot-store delta-commit 8 ms p50 budget is store-side, but
   the joint exploration-step storage budget ≤ 100 ms is verified end-to-end here).
3. The worker daemon serves the full v1 gRPC surface; two concurrent slots fork and
   replay without cross-talk.

(Exit-gate items owned by other repos — snapshot-store crash-injection suite,
reference-workload M2 — are **not** in this graph.)

Milestone acceptance criteria the graph must encode verbatim as test beads
(IMPLEMENTATION-PLAN.md is normative; summaries here):

**M4:**
- Snapshot transparency roundtrip: boot → run 1e8 → snapshot → destroy → restore →
  run 1e8 → H1; versus boot → run 2e8 → H2; **H1 == H2**.
- Fork transparency: same test with a tier-A fork in the middle; and parent frozen →
  child diverges → parent's second child re-run matches first child given same inputs.
- Dirty-ring-full forced (ring size 512) — hashes unchanged vs large ring.
- ENTR golden: snapshot → restore reproduces the next 1024 entropy draws
  bit-identically (`{seed, stream, word_pos}` round trip).
- Perf gates (p50, Intel box, 128 MiB guest): fork < 10 ms; incremental snapshot
  ≤ 8k dirty pages < 15 ms; tier-B warm restore < 150 ms.

**M5:**
- Record/replay: scripted 60s-vns pad sequence on nanokernel pad-echo; replay from
  snapshot reproduces end_state_hash and every EPOCH_HASH. Repeat 100×.
- Fuzz: 24h `cargo fuzz` on the parser, no panics/OOM; then 1h nightly CI job.
- Golden-bytes fixtures for DHILOG v1.0 and DHSNAP v1.0 checked in; byte-identical
  re-serialization asserted.
- at_frame scheduling (absolute FRAME_COUNTER basis) and frame_budget stops verified
  against the FRAME_MARK table, including across a snapshot/restore.

**M6:**
- Integration test drives the whole API over UDS: restore→inject→run→snapshot(with
  CaptureSpec)→destroy, 64 slots concurrently, per-slot hashes match single-slot
  baselines (catches PMU counter collisions and core-pinning bugs).
- Capture-neutrality (ARCH §6.10 C5): identical child refs and epoch hashes for
  capture vs no-capture runs; `layout_version` mismatch fails `FAILED_PRECONDITION`.
- `grpcurl` smoke documented; metrics include every ARCH §9 series.

**M7:**
- 1000/1000 VerifyReplay return VerifyDone with matching end_state_hash; zero
  Divergence.
- Determinism cross-check: 10 of the 1000 jobs re-run from the root on a different
  slot reproduce identical (content-addressed) child snapshot refs.
- Throughput: sustained ≥ N_slots × 1 job/s (within the ARCH §10 per-job budget)
  for ≥ 30 min under simultaneous host load (`stress-ng` on housekeeping cores) —
  exits, PMI, and hashes unaffected by load; hashes are the assertion. The concrete
  N_slots value is the canonical slot count in MAP.md.

**CI deliverables** (testing-strategy table): nightly M7-style 100-fork verify;
checked-in record/replay corpus re-verified nightly against
`ci/determinism-class.lock`; criterion benches for fork/snapshot/restore/landing with
> 20% regression failing nightly; chaos jobs (host load, tiny dirty rings, PMI
storms, snapshot-store latency injection); 1h nightly fuzz.

### Constraints

- **No users exist.** Ignore backwards compatibility and feature flags entirely — no
  migration shims, no compatibility layers, no gradual rollout.
- **snapshot-store is assumed implemented before this graph executes** (its M4 gRPC
  surface + client lib; the M5 UDS SEQPACKET fast path with memfd fd-passing follows
  — adopt it when it lands, gRPC first). This is an assumption about the *future*
  state of concurrent sibling work, not the verified present (the client crate is a
  stub today — see Current repo state); the M4 store-integration track opens with
  the readiness-verification bead described there. Integrate against the real store
  via `../snapshot-store/crates/snapstore-client` (path dep, same pattern as
  `detguest-host`) — never a mock (risk R12). If the store's actual surface diverges
  from `.agents/docs/snapshot-store/API.md`, the sibling repo's own docs/code are
  authoritative; file a documentation issue on mismatch.
- **Sibling-repo edit policy:** `../control-plane` is editable for the proto seam
  only (determinism-proto schema + codegen); `../snapshot-store` and `../guest-sdk`
  are read-only — integrate against them and file issues for gaps, never patch them
  from this graph.
- **Hardware-gated verification protocol:** implementation agents run on a
  macOS/aarch64 dev host. Host-runnable tests are verified locally; every
  hardware-gated acceptance criterion (KVM, PMU, perf gates, M6 64-slot, M7) is
  verified by pushing the branch and observing the `kvm-intel` self-hosted runner's
  CI results — beads must phrase acceptance accordingly. The 24h fuzz run does not
  fit an agent bead: model it as an operator-run / `workflow_dispatch` task whose
  CI artifact is the recurring 1h nightly job.
- **The M4→M5→M6→M7 chain is strictly sequential** and is the phase's critical path.
  Intra-milestone parallelism is encouraged (e.g. DHSNAP codec golden tests are
  host-runnable and independent of dirty-ring plumbing; DHILOG codec/fuzz work is
  host-runnable; proto/codegen seam work can start during M4/M5), but no bead of
  milestone N+1's hardware-gated acceptance may start before milestone N's acceptance
  beads close.
- **Clean-room source boundary** (normative, MAP.md): implementation agents use only
  this repo, the sibling repos listed above, `.agents/docs/`, public hardware/OS/KVM/
  Rust/crate documentation, and operator-supplied artifacts. If a requirement can't
  be met from the allowed source set, stop and file a documentation issue.
- **Test partitioning must be stated per bead:** host-runnable (any machine incl.
  macOS/aarch64 — codecs, golden bytes, fuzz, agenda math) vs hardware-gated
  (`kvm-intel` self-hosted runner / Intel box — anything touching KVM, PMU, perf
  gates). Unit-layer crates must keep building on aarch64.
- **Settled decisions stay settled:** TSC alignment (`docs/decisions/tsc-alignment.md`);
  the chained state hash and DHILOG v1 formats freeze at this phase's golden-bytes
  fixtures.
- **Out of scope for this graph:** M8 (divergence bisection, FAULTED_S hardening —
  next phase), M9 (Linux bzImage guest), `RunWithFrameCapture` (Phase 7),
  control-plane image-blob fetch (Phase 6), snapshot-store M6/M8 work
  (store repo), reference-workload M2 (its repo). VerifyReplay itself (needed by M7)
  IS in scope; its bisection refinement is not.
- Perf numbers are measured on the quiesced Intel box at p50 with the 128 MiB demo
  guest (MAP.md canonical figure) — beads must say where the measurement runs.
- **This Constraints section overrides the generic boilerplate below** ("Change-
  Specific Considerations", "Completeness Checklist"): do not emit beads for feature
  flags, A/B testing, gradual rollout, data-migration scripts, dual-write periods,
  or rollback plans — none apply here.

---

## Your Task

Analyze this codebase change and create a comprehensive **Beads task graph** using the `bd` CLI. Beads provides dependency-aware, conflict-free task management for multi-agent execution.

Before creating the task graph, you MUST first analyze the affected areas of the codebase:

1. Check `docs/decisions/` and `docs/phase-1-exit-gate.md` for existing architectural decisions
2. Examine the directory/module structure of the affected areas listed above
3. Identify key interfaces, APIs, and integration points that must be preserved
4. Note existing test patterns and coverage in the affected areas
5. Assess risk areas where changes could break existing functionality

Use your analysis to make each bead specific — reference actual file paths, module names, and patterns you observed.

Then generate a shell script that creates the complete task graph.

**IMPORTANT: Your ONLY deliverable is a bash shell script with `bd create` commands. Not an implementation plan. Not a design document. Not a code review. A runnable `.sh` script.**

---

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
# Change: Refactor auth middleware for compliance
# Generated: 2026-06-10

set -e

# Initialize beads if needed
if [ ! -d ".beads" ]; then
    bd init
fi

echo "Creating change beads..."

# ========================================
# Phase 1: Analysis & Preparation
# ========================================

ANALYZE_CURRENT=$(bd create "Analyze current auth middleware implementation in src/auth/ — document all session token storage patterns and consumer dependencies" -p 0 --label analysis --silent)

IDENTIFY_DEPS=$(bd create "Map all modules importing from src/auth/ and catalog their usage patterns" -p 0 --label analysis --silent)

CHAR_TESTS=$(bd create "Add characterization tests capturing current auth middleware behavior before refactoring" -p 0 --label prep --silent)
bd dep add $CHAR_TESTS $ANALYZE_CURRENT

# ========================================
# Phase 2: Core Implementation
# ========================================

IMPL_NEW_STORAGE=$(bd create "Implement compliant session token storage in src/auth/session.ts replacing in-memory store" -p 0 --label impl --silent)
bd dep add $IMPL_NEW_STORAGE $CHAR_TESTS
bd dep add $IMPL_NEW_STORAGE $IDENTIFY_DEPS

IMPL_MIGRATION=$(bd create "Create migration script for existing session data to new storage format" -p 1 --label impl --silent)
bd dep add $IMPL_MIGRATION $IMPL_NEW_STORAGE

UPDATE_CONSUMERS=$(bd create "Update all consumer modules to use new auth middleware API surface" -p 1 --label impl --silent)
bd dep add $UPDATE_CONSUMERS $IMPL_NEW_STORAGE

# ========================================
# Phase 3: Testing & Validation
# ========================================

UNIT_TESTS=$(bd create "Add unit tests for new session storage implementation" -p 1 --label testing --silent)
bd dep add $UNIT_TESTS $IMPL_NEW_STORAGE

INTEGRATION_TESTS=$(bd create "Add integration tests for auth flow end-to-end with new middleware" -p 1 --label testing --silent)
bd dep add $INTEGRATION_TESTS $UPDATE_CONSUMERS

REGRESSION_CHECK=$(bd create "Run full regression suite and verify characterization tests still pass" -p 0 --label testing --silent)
bd dep add $REGRESSION_CHECK $INTEGRATION_TESTS
bd dep add $REGRESSION_CHECK $UNIT_TESTS

# ========================================
# Phase 4: Cleanup & Documentation
# ========================================

UPDATE_DOCS=$(bd create "Update auth middleware documentation and API reference" -p 2 --label docs --silent)
bd dep add $UPDATE_DOCS $REGRESSION_CHECK

CLEANUP=$(bd create "Remove deprecated session storage code and update changelog" -p 3 --label cleanup --silent)
bd dep add $CLEANUP $REGRESSION_CHECK

echo ""
echo "Bead graph created! View with:"
echo "  bd ready              # List unblocked tasks"
```

---

## Bead Creation Guidelines

### Priority Levels
- `-p 0` = Critical (blocking other work, or high-risk changes needing early validation)
- `-p 1` = High (important implementation work)
- `-p 2` = Medium (standard work)
- `-p 3` = Low (cleanup, nice-to-haves)

### Labels (Phase Grouping)
Use `--label` to group beads by phase:
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

---

## Verification Steps

After generating the script:

1. **Run it**: `chmod +x setup-beads.sh && ./setup-beads.sh`
2. **Check ready work**: `bd ready` should show initial analysis/prep tasks

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
