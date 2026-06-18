#!/usr/bin/env bash
# Project: determinism-hypervisor
# Change: M9 minimal-Linux guest path, bzImage direct boot
# Generated: 2026-06-18
#
# This script creates the Beads task graph only. It intentionally does not run
# any implementation commands or gates.

set -euo pipefail

export BD_NON_INTERACTIVE=1

if [ ! -d ".beads" ]; then
  bd init --non-interactive
fi

echo "Creating M9 minimal-Linux guest path beads..."

M9_EPIC=$(bd create 'M9 minimal-Linux guest path for bzImage boot' \
  --type epic \
  --priority 0 \
  --labels 'm9,planning' \
  --description 'Implement the minimal Linux guest path for bzImage plus initramfs direct boot, route worker CreateVm to it, preserve deterministic device/runtime contracts, and rerun Phase 1 and Phase 2 gates against Linux while keeping the nanokernel gates green.' \
  --acceptance 'All child beads are closed; Linux Phase 1 gate reports 100/100 zero divergence to Ready and post-Ready budget; Linux Phase 2 record/replay and fork/VerifyReplay acceptance pass with zero Divergence; nanokernel Phase 1/2 evidence remains green; docs contain dated commands and evidence.' \
  --notes 'Scope root. Do not treat this bead as implementation ownership; child beads below carry exact Reservations notes.' \
  --silent)

# ========================================
# Phase 1: Analysis and decisions
# ========================================

ANALYZE_REPO_CONTEXT=$(bd create 'Analyze local architecture homes and M9 affected surfaces' \
  --parent "$M9_EPIC" \
  --type task \
  --priority 0 \
  --labels 'm9,analysis' \
  --description 'Inspect the current repo layout before implementation. Confirm there is no `docs/specs/` or `docs/adr/`, record that architectural decisions belong in `docs/decisions/` and `docs/upstream-divergences.md`, and summarize the module surfaces named by the M9 prompt.' \
  --acceptance 'Issue notes list the absence or presence of `docs/specs/` and `docs/adr/`; list `crates/dh-vmm/src/{boot,config,kvm,cpuid,msr,inject,runctl,hash,recording}.rs`, `crates/dh-devices/src/{bus,ctx,clock,pad,entropy,blk,serial,detchannel}.rs`, `crates/dh-worker/src/{image_resolver,proto_map,service,restore_engine,fork_engine,replay_engine}.rs`, `tools/dh-cli/src/{cli,boot,run,gate}.rs`, `tests/determinism/tests`, `crates/dh-worker/tests`, `tests/nanokernel`, `docs/ops`, and `.github/workflows` with one observed integration point per area.' \
  --notes 'Reservations: none. Read-only survey bead; use `rg --files`, `rg`, and `bd remember` only if persistent repo knowledge is found.' \
  --silent)

ANALYZE_BOOT_PATH=$(bd create 'Analyze current ELF boot path and Linux direct-boot requirements' \
  --parent "$M9_EPIC" \
  --type task \
  --priority 0 \
  --labels 'm9,analysis' \
  --description 'Read `crates/dh-vmm/src/boot.rs`, `crates/dh-vmm/src/config.rs`, `proto/hypervisor.proto`, `crates/dh-worker/src/proto_map.rs`, and the linked M9/reference-workload docs. Map the current ELF-only loader and the Linux bzImage protocol pieces that must be added.' \
  --acceptance 'Notes identify `dh_vmm::boot::load_and_enter` as ELF-only, `MachineConfig::canonical_encode` boot tag 2 as the BzImage preimage, `BzImageBoot.cmdline` as append-only extras, and the required Linux artifacts: setup header validation, boot params zero page, e820 map, canonical cmdline bytes, initramfs placement, entry state, and unsupported-feature rejection.' \
  --notes 'Reservations: none for analysis. Later implementation beads reserve `crates/dh-vmm/src/boot.rs`, `crates/dh-vmm/src/config.rs`, `crates/dh-worker/src/proto_map.rs`, and `proto/hypervisor.proto`.' \
  --silent)

ANALYZE_WORKER_API=$(bd create 'Analyze worker lifecycle and BzImage API seams' \
  --parent "$M9_EPIC" \
  --type task \
  --priority 0 \
  --labels 'm9,analysis' \
  --description 'Trace `CreateVm`, `RestoreSnapshot`, `Fork`, `VerifyReplay`, image-cache resolution, and proto conversion for BzImage inputs. Identify exactly where boot-time initialization is allowed and where restore/fork/replay must rebuild assets without booting.' \
  --acceptance 'Notes cite `crates/dh-worker/src/service.rs::boot_slot` rejecting `ResolvedBoot::BzImage` as UNIMPLEMENTED, `ImageResolver::resolve_create_vm` resolving BzImage kernel/initramfs by verified BLAKE3 blobs, `build_bus` constructing deterministic devices from `device_set`, and restore/fork/replay paths that must never call the Linux boot loader.' \
  --notes 'Reservations: none for analysis. Later beads reserve `crates/dh-worker/src/{service,image_resolver,proto_map,restore_engine,fork_engine,replay_engine}.rs` and worker tests.' \
  --silent)

ANALYZE_CPU_SURFACE=$(bd create 'Enumerate Linux early-boot CPU MSR interrupt and time surface' \
  --parent "$M9_EPIC" \
  --type task \
  --priority 0 \
  --labels 'm9,analysis' \
  --description 'Compare the current CPUID mask, default-deny MSR policy, no-irqchip KVM setup, interrupt injection, run control, state hash, and recording behavior with the expected Linux early boot surface. Decide what instrumentation is needed to confirm APIC/MSR/MMIO requirements once a minimal Linux boot reaches KVM exits.' \
  --acceptance 'Notes list every forbidden surface that must stay disabled: `KVM_CREATE_IRQCHIP`, PIT, IOAPIC, kvmclock leaves, x2APIC, TSC-deadline, raw TSC/RDTSCP, RDRAND/RDSEED, and host PMU exposure. Notes also define the first Linux characterization command to capture denied MSRs, APIC MMIO/MSR accesses, IRQ injections, timer delivery, and guest-only instruction counts.' \
  --notes 'Reservations: none for analysis. Later beads reserve `crates/dh-vmm/src/{kvm,cpuid,msr,inject,runctl,hash,recording}.rs`, `crates/dh-worker/src/{snapshot_engine,restore_engine,replay_engine}.rs`, and DHSNAP golden fixtures if LAPC changes.' \
  --silent)

DECIDE_READY_AND_BLOCK=$(bd create 'Resolve Linux READY and pv-blk device contracts before implementation' \
  --parent "$M9_EPIC" \
  --type decision \
  --priority 0 \
  --labels 'm9,analysis,docs' \
  --description 'Resolve the cross-repo contract mismatch before any device implementation. The linked guest-sdk/reference-workload docs define `Ready` as EventKind 14 and reference a `linux-direct` image with a read-only game image at `/dev/vdb` via `virtio-blk`; M9 chooses this repo deterministic `PvBlk` at `0xD000_4000` plus a Linux guest driver that presents the game image as `/dev/vdb`.' \
  --acceptance 'A committed decision in `docs/decisions/` or a numbered entry in `docs/upstream-divergences.md` states that M9 uses deterministic pv-blk at `0xD000_4000`, the Linux guest driver names the read-only game image `/dev/vdb`, and a deterministic virtio-blk subset is explicitly out of scope unless a new superseding bead is filed. The same decision pins EventKind 14 `Ready{unit, region_count, manifest_generation}` as the only accepted Linux READY point and explicitly rejects serial-only markers.' \
  --notes 'Reservations: `docs/decisions/*`, `docs/upstream-divergences.md`, `crates/dh-devices/src/blk.rs`, `crates/dh-devices/src/detchannel.rs`. This decision blocks Linux device/runtime and gate work.' \
  --silent)

ANALYZE_TESTS=$(bd create 'Analyze nanokernel gates and worker regression test patterns' \
  --parent "$M9_EPIC" \
  --type task \
  --priority 0 \
  --labels 'm9,analysis,testing' \
  --description 'Read the existing Phase 1 determinism tests, CLI gate, worker M5 record/replay, M7 fork/VerifyReplay, replay/restore tests, and nanokernel fixtures. Identify reusable harness patterns and golden artifacts for Linux variants.' \
  --acceptance 'Notes name the current nanokernel-only entry points: `dh-cli gate --runs 100`, `tests/determinism/tests/{common,regression,timer_determinism,if0_deferral,landing_precision,m1_acceptance}.rs`, `crates/dh-worker/tests/{m5_record_replay,m5_frame_scheduling,m5_net_loopback,m7_fork_verify,replay_engine,restore_engine}.rs`, and `tests/nanokernel`. Notes identify which tests become Linux variants and which remain nanokernel characterization controls.' \
  --notes 'Reservations: none for analysis. Later beads reserve only the specific test files they extend.' \
  --silent)

# ========================================
# Phase 2: Preparation and characterization
# ========================================

PREP_NANOKERNEL_BASELINE=$(bd create 'Pin nanokernel Phase 1 and Phase 2 baseline before Linux edits' \
  --parent "$M9_EPIC" \
  --type task \
  --priority 0 \
  --labels 'm9,prep,testing' \
  --description 'Capture current nanokernel behavior before Linux changes so regressions are separable from M9 failures. Do not update existing nanokernel fixture bytes in this bead.' \
  --acceptance '`cargo test --workspace` passes on the current host class or records the exact host-gated skips; `cargo run -p dh-cli -- gate --runs 100` passes on kvm-intel; existing M5 corpus reverify and documented M7 operator commands are recorded with date, host kernel, microcode, and determinism-class lock. `dh-cli gate` still defaults to nanokernel mode.' \
  --notes 'Reservations: `docs/phase-1-exit-gate.md`, `docs/phase-2-exit-gate.md` for dated evidence only. Do not edit `tests/nanokernel/**` or checked-in corpus files here.' \
  --silent)

PREP_LINUX_FIXTURES=$(bd create 'Define Linux fixture and artifact inputs for M9 gates' \
  --parent "$M9_EPIC" \
  --type task \
  --priority 0 \
  --labels 'm9,prep,testing' \
  --description 'Define how tests and operator gates locate the pinned bzImage, deterministic initramfs, base image, and game image without committing large artifacts accidentally. Align env vars, local paths, and image-cache registration with reference-workload artifacts.' \
  --acceptance 'Docs and test helpers define exactly these artifact env vars: `DH_M9_BZIMAGE`, `DH_M9_INITRAMFS`, `DH_M9_BASE_IMAGE`, `DH_M9_GAME_IMAGE`, and `DH_M9_IMAGE_CACHE`. Unit tests fail loudly when a Linux acceptance test is requested and any required artifact is missing. Acceptance evidence forbids `*_ALLOW_SKIP=1` for final M9 gates.' \
  --notes 'Reservations: `crates/dh-worker/tests/common/mod.rs`, `tests/determinism/tests/common/mod.rs`, `tools/dh-cli/tests/**`, `docs/ops/test-partitioning.md`, `docs/ops/github-runner.md`.' \
  --silent)

PREP_CMDLINE_POLICY=$(bd create 'Specify canonical Linux cmdline and BzImage extras policy' \
  --parent "$M9_EPIC" \
  --type decision \
  --priority 0 \
  --labels 'm9,prep,analysis' \
  --description 'Turn the prompt command line into a precise config/proto policy before hashing. The hypervisor-owned baseline must be forced; BzImage extras may only add whitelisted append-only tokens.' \
  --acceptance 'Decision text names the exact forced baseline bytes: `console=ttyS0 nokaslr norandmaps random.trust_cpu=off tsc=unstable clocksource=dh-pvclock nohz=off highres=off init=/init`. It lists accepted extras, at minimum `quiet` and `loglevel=<n>`, rejects duplicates and unsupported tokens, defines canonical ordering and spacing, and states that the canonical bytes are what `MachineConfig::config_hash` snapshots and what the boot params cmdline pointer exposes to Linux.' \
  --notes 'Reservations: `docs/decisions/*`, `docs/upstream-divergences.md`, `crates/dh-vmm/src/config.rs`, `crates/dh-worker/src/proto_map.rs`, `proto/hypervisor.proto`.' \
  --silent)

PREP_LINUX_EXIT_TRACE=$(bd create 'Characterize Linux early-boot exits after first entry path' \
  --parent "$M9_EPIC" \
  --type task \
  --priority 1 \
  --labels 'm9,analysis,testing' \
  --description 'Add and run an opt-in diagnostic harness after the first bzImage entry path exists. The harness boots the M9 Linux fixture far enough to enumerate early exits, denied MSRs, APIC MMIO/MSR accesses, IRQ/timer behavior, and first detchannel activity without using host-time sources.' \
  --acceptance '`DH_M9_TRACE_BOOT=1 DH_M9_ALLOW_SKIP=0 cargo test -p determinism-tests --test linux_boot_trace --release -- --ignored --nocapture` produces `target/m9/linux_boot_trace.json` containing exit kind counts, denied MSR indices, APIC MMIO/MSR addresses, `lapic_required=true|false`, first detchannel status if reached, and terminal reason. The harness is ignored by default and never relaxes default-deny MSR behavior to make progress.' \
  --notes 'Reservations: `tests/determinism/tests/linux_boot_trace.rs`, `tests/determinism/tests/common/mod.rs`, `crates/dh-vmm/src/{kvm,msr,runctl}.rs` only for public diagnostic hooks. Intentional ordering: this characterization depends on the first Linux entry implementation so it can collect real KVM exits.' \
  --silent)

# ========================================
# Phase 3: Core implementation
# ========================================

IMPL_CMDLINE_CANON=$(bd create 'Implement BzImage cmdline canonicalization before MachineConfig hashing' \
  --parent "$M9_EPIC" \
  --type feature \
  --priority 0 \
  --labels 'm9,impl' \
  --description 'Implement the canonical Linux cmdline policy in config/proto/worker mapping so `BzImageBoot.cmdline` extras are validated and normalized before `MachineConfig` is hashed, snapshotted, or used by the boot loader.' \
  --acceptance '`cargo test -p dh-vmm config::` and `cargo test -p dh-worker proto_map::` include BzImage cases proving allowed extras normalize to one byte string, unsupported extras fail with INVALID_ARGUMENT, duplicate baseline tokens are rejected, `MAX_CMDLINE` is enforced after baseline composition, and the MCFG/hash preimage cmdline bytes equal the boot params cmdline bytes in a host-only test.' \
  --notes 'Reservations: `crates/dh-vmm/src/config.rs`, `crates/dh-worker/src/proto_map.rs`, `crates/dh-worker/src/service.rs`, `proto/hypervisor.proto`, `crates/dh-worker/tests/**` for conformance cases.' \
  --silent)

IMPL_BZIMAGE_PARSE=$(bd create 'Implement bzImage setup-header parser and validation' \
  --parent "$M9_EPIC" \
  --type feature \
  --priority 0 \
  --labels 'm9,impl' \
  --description 'Add a host-runnable bzImage parser in `crates/dh-vmm/src/boot.rs` or a new `boot/linux.rs` module. Validate setup header magic, protocol version, load flags, payload offsets, kernel alignment, initrd support, cmdline support, and unsupported protocol features with deterministic error messages.' \
  --acceptance '`cargo test -p dh-vmm linux_bzimage --lib` covers at least one valid synthetic bzImage, bad magic, truncated setup header, unsupported protocol version, payload overflow, unsupported relocatable feature combinations, initramfs too large for placement, and cmdline too long. No `/dev/kvm` is required for these tests.' \
  --notes 'Reservations: `crates/dh-vmm/src/boot.rs` or `crates/dh-vmm/src/boot/**`, `crates/dh-vmm/src/lib.rs` if a module is added.' \
  --silent)

IMPL_LINUX_LAYOUT=$(bd create 'Implement Linux boot params e820 cmdline and initramfs layout' \
  --parent "$M9_EPIC" \
  --type feature \
  --priority 0 \
  --labels 'm9,impl' \
  --description 'Lay out Linux boot params zero page, e820 memory map, command line, initramfs, kernel payload, loader scratch pages, and deterministic low-memory reservations. Preserve the existing MMIO hole mapping behavior needed for deterministic devices and add APIC MMIO mapping only through the chosen deterministic model.' \
  --acceptance 'Host-only layout tests assert exact GPAs, non-overlap, zero-filled boot params, e820 entries excluding the deterministic MMIO holes, command-line pointer and size, initramfs address and size, kernel payload placement, and stable byte-for-byte layout across two identical inputs. Tests reject configurations that would overlap guest RAM, loader pages, initramfs, kernel, cmdline, APIC MMIO, or device windows.' \
  --notes 'Reservations: `crates/dh-vmm/src/boot.rs` or `crates/dh-vmm/src/boot/**`, `crates/dh-vmm/src/kvm.rs` only if constants for non-RAM MMIO holes move.' \
  --silent)

IMPL_LINUX_ENTRY=$(bd create 'Program deterministic Linux bzImage entry state' \
  --parent "$M9_EPIC" \
  --type feature \
  --priority 0 \
  --labels 'm9,impl' \
  --description 'Add the KVM vCPU setup for the Linux direct-boot entry state and route it through a new public loader such as `dh_vmm::boot::load_bzimage_and_enter`. Apply the default-deny MSR filter before first guest instruction and keep deterministic CPUID masking.' \
  --acceptance '`DH_M9_ALLOW_SKIP=0 cargo test -p determinism-tests --test linux_boot_trace --release linux_entry_smoke -- --ignored --nocapture` enters a minimal Linux bzImage fixture, records deterministic early exits with the MSR filter applied, and proves no KVM in-kernel irqchip, PIT, IOAPIC, kvmclock CPUID leaves, TSC-deadline, x2APIC, RDRAND, RDSEED, RDTSCP, or invariant TSC are exposed. Existing ELF `load_and_enter` tests still pass unchanged.' \
  --notes 'Reservations: `crates/dh-vmm/src/boot.rs`, `crates/dh-vmm/src/{kvm,cpuid,msr}.rs`, `tests/determinism/tests/linux_boot_trace.rs`.' \
  --silent)

IMPL_LINUX_CPU_COMPAT=$(bd create 'Implement non-lAPIC Linux early-boot CPU and run-control compatibility' \
  --parent "$M9_EPIC" \
  --type feature \
  --priority 0 \
  --labels 'm9,impl' \
  --description 'Implement deterministic Linux early-boot compatibility outside the lAPIC model: allowed MSR handling, rejected MSR behavior, interrupt injection invariants, guest-only instruction counting, timer conversion, state-hash preimage coverage, and recording/replay treatment for any new deterministic exit surface exposed by the trace harness.' \
  --acceptance '`DH_M9_TRACE_BOOT=1 DH_M9_ALLOW_SKIP=0 cargo test -p determinism-tests --test linux_boot_trace --release -- --ignored --nocapture` shows no unclassified denied MSR, MMIO, IRQ, or timer exit remains before READY. `cargo test -p dh-vmm linux_cpu_compat --lib` proves each newly allowed MSR or exit is deterministic, covered by state hash or replay as appropriate, and still rejects raw host time, host entropy, kvmclock, PIT, IOAPIC, TSC-deadline, and in-kernel irqchip.' \
  --notes 'Reservations: `crates/dh-vmm/src/{kvm,cpuid,msr,inject,runctl,hash,recording}.rs`, `crates/dh-worker/src/{replay_engine,restore_engine,snapshot_engine}.rs`, `tests/determinism/tests/linux_boot_trace.rs`.' \
  --silent)

IMPL_LAPIC_MODEL=$(bd create 'Implement deterministic lAPIC compatibility model for Linux early boot' \
  --parent "$M9_EPIC" \
  --type feature \
  --priority 1 \
  --labels 'm9,impl' \
  --description 'If the trace harness shows Linux needs local APIC compatibility, implement the minimal deterministic xAPIC/lAPIC behavior as persisted Rust state rather than a boot-only shim. Keep in-kernel irqchip, PIT, IOAPIC, kvmclock, and TSC-deadline disabled.' \
  --acceptance '`target/m9/linux_boot_trace.json` contains `lapic_required=true` or `lapic_required=false`. If false, `cargo test -p dh-vmm linux_lapic_not_required --lib` asserts the trace reaches READY with no nontrivial lAPIC state and no LAPC format bump. If true, `cargo test -p dh-vmm linux_lapic --lib` implements and verifies served registers, reset values, interrupt acceptance, rejected timers, no host-time reads, and no KVM irqchip creation. `rg -n "KVM_CREATE_IRQCHIP|KVM_CREATE_PIT|kvmclock" crates` shows no new enabled creation path.' \
  --notes 'Reservations: `crates/dh-devices/src/**` for a new lAPIC model if needed, `crates/dh-vmm/src/{kvm,inject,runctl,msr,hash,recording}.rs`, `crates/dh-worker/src/{snapshot_engine,restore_engine,replay_engine}.rs` if state becomes persisted.' \
  --silent)

IMPL_LAPC_PERSISTENCE=$(bd create 'Persist lAPIC state in DHSNAP state hash restore and replay' \
  --parent "$M9_EPIC" \
  --type feature \
  --priority 1 \
  --labels 'm9,impl,testing' \
  --description 'When the lAPIC model is non-empty, replace the current empty `LAPC` v1 placeholder with a versioned deterministic section and include it in state hash, snapshot, restore, fork, replay, VerifyReplay, and golden fixtures.' \
  --acceptance 'If `target/m9/linux_boot_trace.json` has `lapic_required=false`, `cargo test -p dh-worker lapc_empty_v1 --tests` asserts `LAPC` stays empty v1. If it has `lapic_required=true`, `cargo test -p dh-snapshot`, `cargo test -p dh-vmm hash::`, and `cargo test -p dh-worker lapc --tests` pass; DHSNAP golden fixture names or versions are updated; restore rejects malformed LAPC sections; replay and VerifyReplay compare LAPC-derived state and fail on deliberate LAPC mutations.' \
  --notes 'Reservations: `crates/dh-snapshot/src/dhsnap.rs`, `crates/dh-snapshot/tests/**`, `crates/dh-worker/src/{snapshot_engine,restore_engine,replay_engine}.rs`, `crates/dh-vmm/src/hash.rs`, `docs/upstream-divergences.md`.' \
  --silent)

IMPL_LINUX_DEVICE_CONTRACT=$(bd create 'Implement selected Linux deterministic device contract' \
  --parent "$M9_EPIC" \
  --type feature \
  --priority 0 \
  --labels 'm9,impl' \
  --description 'Implement the block/device path chosen by the READY and block contract decision, and ensure the Linux guest sees only deterministic pv-clock, pv-pad, pv-entropy, read-only game image access, serial console, and detchannel surfaces before READY.' \
  --acceptance '`DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test linux_worker_api --release pvblk_dev_vdb -- --ignored --nocapture` proves deterministic pv-blk at `0xD000_4000` is visible to the Linux fixture as `/dev/vdb`, is read-only for the base game image, persists overlay/device state through snapshot/hash/replay, and cannot read host entropy or host time. READY tests prove CHANNEL_INIT, Hello, autostart, Start, expected regions, and Ready EventKind 14 all happen before any host-injected input.' \
  --notes 'Reservations: `crates/dh-devices/src/{bus,ctx,clock,pad,entropy,blk,serial,detchannel}.rs`, `crates/dh-vmm/src/{blkfile,recording,runctl}.rs`, `crates/dh-worker/src/service.rs`, `docs/decisions/*`, `docs/upstream-divergences.md`.' \
  --silent)

IMPL_IMAGE_RESOLVER=$(bd create 'Harden image resolver coverage for bzImage initramfs artifacts' \
  --parent "$M9_EPIC" \
  --type task \
  --priority 1 \
  --labels 'm9,impl,testing' \
  --description 'Confirm and extend the worker image-cache resolver for Linux boot blobs. BzImage and initramfs bytes must be content-addressed, size-capped, regular files, no symlink escape, verified before use, and reported with actionable errors.' \
  --acceptance '`cargo test -p dh-worker image_resolver::` covers BzImage and initramfs success, missing blob, hash mismatch, non-regular file, symlink/ELOOP, kernel cap `MAX_KERNEL_BYTES`, initramfs cap `MAX_INITRAMFS_BYTES`, and error mapping through CreateVm. Tests prove bytes are verified before `ResolvedBoot::BzImage` escapes the resolver.' \
  --notes 'Reservations: `crates/dh-worker/src/image_resolver.rs`, `crates/dh-worker/src/service.rs`, `crates/dh-worker/tests/**` if integration cases are needed.' \
  --silent)

IMPL_WORKER_BZIMAGE=$(bd create 'Route worker CreateVm BzImage boot through the Linux loader' \
  --parent "$M9_EPIC" \
  --type feature \
  --priority 0 \
  --labels 'm9,impl' \
  --description 'Replace the current `ResolvedBoot::BzImage` UNIMPLEMENTED path in `crates/dh-worker/src/service.rs::boot_slot` with the deterministic Linux boot loader and make CreateVm install the same bus/image/runtime state used by snapshots and replay.' \
  --acceptance '`cargo test -p dh-worker service::` includes CreateVm BzImage success against the Linux fixture, INVALID_ARGUMENT or FAILED_PRECONDITION for bad cmdline/artifacts, and no regression for ELF CreateVm. A test asserts BzImage CreateVm calls the Linux loader exactly once and stores the resulting MachineConfig hash in DHILOG and runtime state.' \
  --notes 'Reservations: `crates/dh-worker/src/service.rs`, `crates/dh-worker/src/image_resolver.rs`, `crates/dh-worker/src/proto_map.rs`, `crates/dh-worker/tests/**`.' \
  --silent)

IMPL_RESTORE_FORK_BOOT_ONCE=$(bd create 'Ensure Linux restore fork and replay never rerun boot initialization' \
  --parent "$M9_EPIC" \
  --type feature \
  --priority 0 \
  --labels 'm9,impl,testing' \
  --description 'Prove `RestoreSnapshot`, tier-A `Fork`, replay restore, and `VerifyReplay` rebuild deterministic bus/image assets and restore DHSNAP/MCFG state without invoking the Linux boot loader or redoing initramfs/READY setup.' \
  --acceptance '`cargo test -p dh-worker --test restore_engine linux_boot_once --release -- --ignored --nocapture` and `cargo test -p dh-worker --test replay_engine linux_boot_once --release -- --ignored --nocapture` include a boot-call counter/fake loader guard proving restore/fork/replay call the BzImage loader zero times. Tests compare restored/forked Linux `machine_config_hash`, `state_hash`, detchannel EVTC state, and pv-blk/detchannel device state to the root snapshot.' \
  --notes 'Reservations: `crates/dh-worker/src/{service,restore_engine,fork_engine,replay_engine,runtime}.rs`, `crates/dh-worker/tests/{restore_engine,replay_engine,fork_engine,m7_fork_verify}.rs`.' \
  --silent)

IMPL_CLI_LINUX=$(bd create 'Add dh-cli Linux boot run and gate entry points without worker-private dependency' \
  --parent "$M9_EPIC" \
  --type feature \
  --priority 1 \
  --labels 'm9,impl' \
  --description 'Extend `dh-cli boot`, `dh-cli run`, and `dh-cli gate` with Linux guest options while preserving the existing default nanokernel path. M9 pins the CLI seam to explicit local artifact paths and direct VMM/device harnessing; do not use `dh-worker` private modules or the gRPC ops path for these commands.' \
  --acceptance '`cargo test -p dh-cli --tests` covers argument parsing for `--linux`, `--bzimage`, `--initramfs`, `--base-image`, `--game-image`, `--cmdline-extra`, and default nanokernel behavior. `cargo run -p dh-cli -- gate --runs 2` still uses nanokernel. `DH_M9_ALLOW_SKIP=0 cargo run -p dh-cli -- gate --linux --runs 2 --bzimage "$DH_M9_BZIMAGE" --initramfs "$DH_M9_INITRAMFS" --base-image "$DH_M9_BASE_IMAGE" --game-image "$DH_M9_GAME_IMAGE"` reports Ready EventKind 14 rather than serial text.' \
  --notes 'Reservations: `tools/dh-cli/src/{cli,boot,run,gate}.rs`, `tools/dh-cli/tests/**`. Do not import `dh-worker::image_resolver` into `dh-cli`; do not route Linux gate through `tools/dh-cli/src/ops.rs`.' \
  --silent)

# ========================================
# Phase 4: Linux verification gates
# ========================================

TEST_LINUX_READY=$(bd create 'Add Linux boot-to-READY determinism tests' \
  --parent "$M9_EPIC" \
  --type task \
  --priority 0 \
  --labels 'm9,testing' \
  --description 'Add KVM-gated tests that cold boot the Linux fixture to guest-sdk Ready EventKind 14 and compare deterministic bootstrap identity across runs.' \
  --acceptance '`DH_M9_ALLOW_SKIP=0 cargo test -p determinism-tests --test linux_ready --release -- --ignored --nocapture` runs at least 2 cold boots using `DH_M9_BZIMAGE`, `DH_M9_INITRAMFS`, `DH_M9_BASE_IMAGE`, and `DH_M9_GAME_IMAGE`; it asserts equal `ready_icount`, Ready payload `unit`, `region_count`, `manifest_generation`, `machine_config_hash`, and `state_hash`. The test fails if any env var is unset, only serial output is observed, or any host input is consumed before Ready.' \
  --notes 'Reservations: `tests/determinism/tests/linux_ready.rs`, `tests/determinism/tests/common/mod.rs`, `crates/dh-worker/tests/common/mod.rs` if worker-backed helpers are shared.' \
  --silent)

TEST_PHASE1_LINUX_GATE=$(bd create 'Add Phase 1 Linux gate with 100 cold boots and post-READY budget' \
  --parent "$M9_EPIC" \
  --type task \
  --priority 0 \
  --labels 'm9,testing' \
  --description 'Add the operator/CLI Phase 1 Linux determinism gate. It must boot Linux to Ready 100 times with no host-injected input, compare the Ready fingerprint, then run a fixed post-READY icount budget.' \
  --acceptance '`DH_M9_ALLOW_SKIP=0 cargo run -p dh-cli -- gate --linux --runs 100 --bzimage "$DH_M9_BZIMAGE" --initramfs "$DH_M9_INITRAMFS" --base-image "$DH_M9_BASE_IMAGE" --game-image "$DH_M9_GAME_IMAGE"` exits 0 and prints a PASS artifact showing 100/100 zero divergence for `ready_icount`, Ready payload, `machine_config_hash`, `state_hash`, and fixed post-READY budget state hash. No run may be skipped for acceptance evidence.' \
  --notes 'Reservations: `tools/dh-cli/src/gate.rs`, `tools/dh-cli/tests/**`, `docs/phase-1-exit-gate.md`, `docs/ops/test-partitioning.md` for command text after implementation.' \
  --silent)

TEST_LINUX_TIMER_IRQ=$(bd create 'Add Linux timer and IRQ determinism subgate' \
  --parent "$M9_EPIC" \
  --type task \
  --priority 0 \
  --labels 'm9,testing' \
  --description 'Add a Linux equivalent of the current `timer-event` gate that exercises deterministic timer/interrupt delivery after READY, including any lAPIC compatibility model.' \
  --acceptance '`DH_M9_ALLOW_SKIP=0 cargo test -p determinism-tests --test linux_timer_determinism --release -- --ignored --nocapture` runs 100 Linux cases and asserts identical delivered icount list, vector/source metadata, final `state_hash`, and no host-time timer source. The gate fails if KVM PIT, IOAPIC, kvmclock, TSC-deadline, or in-kernel irqchip is created or advertised.' \
  --notes 'Reservations: `tests/determinism/tests/linux_timer_determinism.rs`, `tests/determinism/tests/common/mod.rs`, `crates/dh-vmm/src/{runctl,inject,kvm}.rs`.' \
  --silent)

TEST_LINUX_LANDING_COUNTING=$(bd create 'Add Linux landing precision and instruction counting regression tests' \
  --parent "$M9_EPIC" \
  --type task \
  --priority 0 \
  --labels 'm9,testing' \
  --description 'Add Linux variants for exact landing/counting behavior so M9 covers the Phase 1 surfaces currently exercised by nanokernel landing, counting, and regression tests.' \
  --acceptance '`DH_M9_ALLOW_SKIP=0 cargo test -p determinism-tests --test linux_landing_counting --release -- --ignored --nocapture` proves at least 100 Linux post-READY landing targets stop at exact requested icount, compares `(icount, rip, rcx, state_hash)` across two cold boots, covers an interrupt-adjacent target, and records zero overshoots or skipped runs. The test also proves guest-only instruction counting by rejecting any host-side exit count as the comparison axis.' \
  --notes 'Reservations: `tests/determinism/tests/linux_landing_counting.rs`, `tests/determinism/tests/common/mod.rs`, `crates/dh-vmm/src/{runctl,hash}.rs` only for fixes exposed by the test.' \
  --silent)

TEST_PHASE2_RECORD_REPLAY=$(bd create 'Add Linux M5 record replay corpus and reverify gate' \
  --parent "$M9_EPIC" \
  --type task \
  --priority 0 \
  --labels 'm9,testing' \
  --description 'Add Linux variants of M5 record/replay coverage using a Linux root snapshot and deterministic post-READY input script. Replay must verify every EPOCH_HASH and the END state hash.' \
  --acceptance '`DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test m5_record_replay --release linux -- --ignored --nocapture` records or reverifies a Linux corpus with every `EPOCH_HASH` verified, END `state_hash` equal to the recorded child snapshot, zero Divergence, and no accepted skipped runs. Checked-in corpus updates include expected hashes, determinism-class lock reference, and fixture README when fixture size policy allows.' \
  --notes 'Reservations: `crates/dh-worker/tests/m5_record_replay.rs`, `crates/dh-worker/tests/fixtures/record_replay_corpus/**`, `ci/determinism-class.lock` only if intentionally rebaselined, `docs/upstream-divergences.md` if corpus policy changes.' \
  --silent)

TEST_PHASE2_FRAME_NET=$(bd create 'Add Linux M4 M5 frame scheduling and pv-net regression coverage' \
  --parent "$M9_EPIC" \
  --type task \
  --priority 0 \
  --labels 'm9,testing' \
  --description 'Add representative Linux variants for worker M4/M5 regression surfaces beyond record/replay: snapshot transparency, frame scheduling continuity, and net loopback or a documented Linux-equivalent deterministic I/O fixture.' \
  --acceptance '`DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux cargo test -p dh-worker --test m4_transparency --release linux -- --ignored --nocapture`, `DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux cargo test -p dh-worker --test m5_frame_scheduling --release linux -- --ignored --nocapture`, and `DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux cargo test -p dh-worker --test m5_net_loopback --release linux -- --ignored --nocapture` all pass with zero Divergence. When M9 ships without Linux pv-net, the `m5_net_loopback` Linux filter must execute a `linux_pvblk_io_loopback` case that writes and reads the deterministic pv-blk overlay, then verifies replayed DHILOG records and final state hashes.' \
  --notes 'Reservations: `crates/dh-worker/tests/{m4_transparency,m5_frame_scheduling,m5_net_loopback}.rs`, `crates/dh-worker/tests/common/mod.rs`, `docs/upstream-divergences.md` if a replacement fixture is accepted.' \
  --silent)

TEST_PHASE2_FORK_VERIFY=$(bd create 'Add Linux M7 fork VerifyReplay acceptance and nightly canary' \
  --parent "$M9_EPIC" \
  --type task \
  --priority 0 \
  --labels 'm9,testing' \
  --description 'Add Linux guest mode to the M7 fork/VerifyReplay acceptance harness. Full acceptance remains operator-run; a Linux 100-child canary runs nightly.' \
  --acceptance '`DH_M9_ALLOW_SKIP=0 DH_M7_ACCEPT_SLOT_CORES=2-5 DH_M7_ACCEPT_GUEST=linux cargo test -p dh-worker --test m7_fork_verify --release -- --ignored --nocapture` completes 1000 fork children with 1000/1000 VerifyDone, zero Divergence, and matching end_state_hash. `DH_M7_ACCEPT_GUEST=linux cargo test -p dh-worker --test m7_fork_verify --release m7_accept_cross_slot_rerun_10_seeded_forks_identical_refs -- --ignored --nocapture` proves same-seed child snapshot refs match across sampled slots. Nightly runs a 100-child Linux canary.' \
  --notes 'Reservations: `crates/dh-worker/tests/m7_fork_verify.rs`, `crates/dh-worker/tests/common/mod.rs`, `.github/workflows/nightly-drift.yaml`, `docs/ops/test-partitioning.md`.' \
  --silent)

TEST_LINUX_WORKER_API=$(bd create 'Add Linux worker API integration tests for CreateVm Run Snapshot Restore Fork VerifyReplay' \
  --parent "$M9_EPIC" \
  --type task \
  --priority 1 \
  --labels 'm9,testing' \
  --description 'Add representative worker-backed Linux integration tests across the public API, including StreamGuestEvents and region reads after READY.' \
  --acceptance '`DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test linux_worker_api --release -- --ignored --nocapture` proves CreateVm BzImage, Run until NextSdkEvent Ready kind 14, StreamGuestEvents filtering, TakeSnapshot, RestoreSnapshot, ReadGuestMemory region_ranges, Fork, Run child, and VerifyReplay all work on Linux. The test asserts restored region manifest generation and layout versions match the Ready payload.' \
  --notes 'Reservations: `crates/dh-worker/tests/linux_worker_api.rs`, `crates/dh-worker/tests/common/mod.rs`, `crates/dh-worker/src/service.rs` only for API fixes exposed by the test.' \
  --silent)

TEST_NANOKERNEL_PRESERVE=$(bd create 'Preserve nanokernel gates and golden fixtures after M9' \
  --parent "$M9_EPIC" \
  --type task \
  --priority 0 \
  --labels 'm9,testing,cleanup' \
  --description 'Run the existing nanokernel gates after the Linux path lands and prove they remain independent regression coverage. Do not remove or silently update nanokernel fixtures.' \
  --acceptance '`cargo test --workspace` passes; `cargo run -p dh-cli -- gate --runs 100` passes and still defaults to nanokernel; current Phase 1 determinism tests pass; M5 nanokernel record/replay corpus reverify passes; documented M7 nanokernel operator commands remain valid; no file under `tests/nanokernel/**` or existing corpus fixtures changes unless a dedicated bead documents and accepts that change.' \
  --notes 'Reservations: `docs/phase-1-exit-gate.md`, `docs/phase-2-exit-gate.md`, `docs/ops/test-partitioning.md`; avoid editing `tests/nanokernel/**` except for an explicit follow-up bead.' \
  --silent)

# ========================================
# Phase 5: Documentation, CI, and rollout
# ========================================

DOC_PHASE_EVIDENCE=$(bd create 'Update Phase 1 and Phase 2 exit gates with Linux and nanokernel evidence' \
  --parent "$M9_EPIC" \
  --type task \
  --priority 1 \
  --labels 'm9,docs' \
  --description 'Update exit-gate records with fresh dated Linux evidence and confirm nanokernel evidence remains current. The docs must distinguish CI, nightly, and operator-run gates.' \
  --acceptance '`docs/phase-1-exit-gate.md` includes the exact Linux 100-run Ready/post-READY/timer command, Linux landing/counting command, date, host, artifact hashes, and 100/100 zero-divergence result. `docs/phase-2-exit-gate.md` includes Linux M4/M5 frame/net regressions, Linux M5 corpus reverify, Linux M7 1000-child acceptance, cross-slot same-seed refs, nightly 100-child canary, and nanokernel preservation evidence. Both docs explicitly state `*_ALLOW_SKIP=1` evidence is not accepted.' \
  --notes 'Reservations: `docs/phase-1-exit-gate.md`, `docs/phase-2-exit-gate.md`.' \
  --silent)

DOC_OPS_CI=$(bd create 'Document Linux gate commands runner requirements and CI nightly classification' \
  --parent "$M9_EPIC" \
  --type task \
  --priority 1 \
  --labels 'm9,docs,cleanup' \
  --description 'Update operational docs and workflows for the Linux M9 gates, including hardware requirements, pinned tools, artifact env vars, and which gates are required, nightly, or operator-run.' \
  --acceptance '`docs/ops/test-partitioning.md` lists exact Linux gate commands, env vars, expected run counts, and CI/nightly/operator classification. `docs/ops/github-runner.md` lists any new pinned tools or kernel/image requirements. `.github/workflows/ci.yaml` and `.github/workflows/nightly-drift.yaml` include Linux jobs only where the docs classify them as required or nightly, and preserve existing nanokernel lanes.' \
  --notes 'Reservations: `docs/ops/test-partitioning.md`, `docs/ops/github-runner.md`, `.github/workflows/ci.yaml`, `.github/workflows/nightly-drift.yaml`.' \
  --silent)

DOC_DIVERGENCES=$(bd create 'Record accepted drift from sibling phase and workload docs' \
  --parent "$M9_EPIC" \
  --type task \
  --priority 1 \
  --labels 'm9,docs' \
  --description 'Document any accepted divergence from the cited `~/.agents/projects/determinism` phase, hypervisor, guest-sdk, and reference-workload docs. This includes the block-device contract, Linux gate scheduling, command-line policy, LAPC format changes, and fixture storage policy.' \
  --acceptance '`docs/upstream-divergences.md` has numbered entries for every accepted drift discovered during M9, with old text, local amendment, authority files/tests, and rollback or follow-up path. If no drift remains beyond already-updated local decisions, the entry says so and points to the merged decision docs.' \
  --notes 'Reservations: `docs/upstream-divergences.md`, `docs/decisions/*`.' \
  --silent)

FINAL_ACCEPTANCE=$(bd create 'Run full M9 acceptance suite and publish final evidence' \
  --parent "$M9_EPIC" \
  --type task \
  --priority 0 \
  --labels 'm9,testing,cleanup' \
  --description 'Run the complete M9 acceptance suite after implementation, tests, docs, and CI wiring land. This bead is the final merge gate for the task graph.' \
  --acceptance 'All commands pass on the documented host: `cargo test --workspace`; `cargo run -p dh-cli -- gate --runs 100`; `DH_M9_ALLOW_SKIP=0 cargo run -p dh-cli -- gate --linux --runs 100 --bzimage "$DH_M9_BZIMAGE" --initramfs "$DH_M9_INITRAMFS" --base-image "$DH_M9_BASE_IMAGE" --game-image "$DH_M9_GAME_IMAGE"`; `DH_M9_ALLOW_SKIP=0 cargo test -p determinism-tests --test linux_landing_counting --release -- --ignored --nocapture`; Linux M4/M5 frame/net regressions; Linux M5 corpus reverify; Linux M7 1000-child VerifyReplay acceptance; Linux M7 cross-slot same-seed refs; nanokernel M5 corpus reverify; documented nanokernel M7 commands. Final notes include artifact paths, hashes, host kernel/microcode, determinism-class lock, and workflow run links if CI/nightly jobs were used.' \
  --notes 'Reservations: no implementation files. May update `docs/phase-1-exit-gate.md`, `docs/phase-2-exit-gate.md`, and issue notes with final evidence only.' \
  --silent)

# ========================================
# Dependencies
# ========================================

bd dep add "$ANALYZE_BOOT_PATH" "$ANALYZE_REPO_CONTEXT"
bd dep add "$ANALYZE_WORKER_API" "$ANALYZE_REPO_CONTEXT"
bd dep add "$ANALYZE_CPU_SURFACE" "$ANALYZE_REPO_CONTEXT"
bd dep add "$DECIDE_READY_AND_BLOCK" "$ANALYZE_REPO_CONTEXT"
bd dep add "$ANALYZE_TESTS" "$ANALYZE_REPO_CONTEXT"

bd dep add "$PREP_NANOKERNEL_BASELINE" "$ANALYZE_TESTS"
bd dep add "$PREP_LINUX_FIXTURES" "$ANALYZE_BOOT_PATH"
bd dep add "$PREP_LINUX_FIXTURES" "$ANALYZE_WORKER_API"
bd dep add "$PREP_LINUX_FIXTURES" "$DECIDE_READY_AND_BLOCK"
bd dep add "$PREP_CMDLINE_POLICY" "$ANALYZE_BOOT_PATH"
bd dep add "$PREP_CMDLINE_POLICY" "$ANALYZE_WORKER_API"
bd dep add "$PREP_LINUX_EXIT_TRACE" "$ANALYZE_CPU_SURFACE"
bd dep add "$PREP_LINUX_EXIT_TRACE" "$PREP_LINUX_FIXTURES"

bd dep add "$IMPL_CMDLINE_CANON" "$PREP_CMDLINE_POLICY"
bd dep add "$IMPL_BZIMAGE_PARSE" "$ANALYZE_BOOT_PATH"
bd dep add "$IMPL_LINUX_LAYOUT" "$IMPL_BZIMAGE_PARSE"
bd dep add "$IMPL_LINUX_LAYOUT" "$IMPL_CMDLINE_CANON"
bd dep add "$IMPL_LINUX_ENTRY" "$IMPL_LINUX_LAYOUT"
bd dep add "$IMPL_LINUX_ENTRY" "$ANALYZE_CPU_SURFACE"
bd dep add "$PREP_LINUX_EXIT_TRACE" "$IMPL_LINUX_ENTRY"
bd dep add "$IMPL_LINUX_CPU_COMPAT" "$PREP_LINUX_EXIT_TRACE"
bd dep add "$IMPL_LAPIC_MODEL" "$PREP_LINUX_EXIT_TRACE"
bd dep add "$IMPL_LAPIC_MODEL" "$IMPL_LINUX_ENTRY"
bd dep add "$IMPL_LAPIC_MODEL" "$IMPL_LINUX_CPU_COMPAT"
bd dep add "$IMPL_LAPC_PERSISTENCE" "$IMPL_LAPIC_MODEL"
bd dep add "$IMPL_LINUX_DEVICE_CONTRACT" "$DECIDE_READY_AND_BLOCK"
bd dep add "$IMPL_LINUX_DEVICE_CONTRACT" "$PREP_LINUX_FIXTURES"
bd dep add "$IMPL_IMAGE_RESOLVER" "$ANALYZE_WORKER_API"
bd dep add "$IMPL_IMAGE_RESOLVER" "$PREP_LINUX_FIXTURES"
bd dep add "$IMPL_WORKER_BZIMAGE" "$IMPL_LINUX_ENTRY"
bd dep add "$IMPL_WORKER_BZIMAGE" "$IMPL_LINUX_CPU_COMPAT"
bd dep add "$IMPL_WORKER_BZIMAGE" "$IMPL_LINUX_DEVICE_CONTRACT"
bd dep add "$IMPL_WORKER_BZIMAGE" "$IMPL_IMAGE_RESOLVER"
bd dep add "$IMPL_WORKER_BZIMAGE" "$IMPL_LAPC_PERSISTENCE"
bd dep add "$IMPL_RESTORE_FORK_BOOT_ONCE" "$IMPL_WORKER_BZIMAGE"
bd dep add "$IMPL_CLI_LINUX" "$IMPL_LINUX_ENTRY"
bd dep add "$IMPL_CLI_LINUX" "$IMPL_LINUX_DEVICE_CONTRACT"
bd dep add "$IMPL_CLI_LINUX" "$IMPL_CMDLINE_CANON"

bd dep add "$TEST_LINUX_READY" "$IMPL_WORKER_BZIMAGE"
bd dep add "$TEST_LINUX_READY" "$IMPL_RESTORE_FORK_BOOT_ONCE"
bd dep add "$TEST_PHASE1_LINUX_GATE" "$TEST_LINUX_READY"
bd dep add "$TEST_PHASE1_LINUX_GATE" "$IMPL_CLI_LINUX"
bd dep add "$TEST_LINUX_TIMER_IRQ" "$TEST_LINUX_READY"
bd dep add "$TEST_LINUX_TIMER_IRQ" "$IMPL_LINUX_CPU_COMPAT"
bd dep add "$TEST_LINUX_TIMER_IRQ" "$IMPL_LAPC_PERSISTENCE"
bd dep add "$TEST_LINUX_LANDING_COUNTING" "$TEST_LINUX_READY"
bd dep add "$TEST_LINUX_LANDING_COUNTING" "$IMPL_LINUX_CPU_COMPAT"
bd dep add "$TEST_PHASE2_RECORD_REPLAY" "$TEST_LINUX_READY"
bd dep add "$TEST_PHASE2_RECORD_REPLAY" "$IMPL_RESTORE_FORK_BOOT_ONCE"
bd dep add "$TEST_PHASE2_FRAME_NET" "$TEST_LINUX_READY"
bd dep add "$TEST_PHASE2_FRAME_NET" "$IMPL_RESTORE_FORK_BOOT_ONCE"
bd dep add "$TEST_PHASE2_FORK_VERIFY" "$TEST_PHASE2_RECORD_REPLAY"
bd dep add "$TEST_PHASE2_FORK_VERIFY" "$TEST_PHASE2_FRAME_NET"
bd dep add "$TEST_PHASE2_FORK_VERIFY" "$IMPL_RESTORE_FORK_BOOT_ONCE"
bd dep add "$TEST_LINUX_WORKER_API" "$TEST_LINUX_READY"
bd dep add "$TEST_LINUX_WORKER_API" "$IMPL_RESTORE_FORK_BOOT_ONCE"
bd dep add "$TEST_NANOKERNEL_PRESERVE" "$PREP_NANOKERNEL_BASELINE"
bd dep add "$TEST_NANOKERNEL_PRESERVE" "$TEST_PHASE1_LINUX_GATE"
bd dep add "$TEST_NANOKERNEL_PRESERVE" "$TEST_LINUX_LANDING_COUNTING"
bd dep add "$TEST_NANOKERNEL_PRESERVE" "$TEST_PHASE2_FRAME_NET"
bd dep add "$TEST_NANOKERNEL_PRESERVE" "$TEST_PHASE2_FORK_VERIFY"

bd dep add "$DOC_PHASE_EVIDENCE" "$TEST_PHASE1_LINUX_GATE"
bd dep add "$DOC_PHASE_EVIDENCE" "$TEST_LINUX_TIMER_IRQ"
bd dep add "$DOC_PHASE_EVIDENCE" "$TEST_LINUX_LANDING_COUNTING"
bd dep add "$DOC_PHASE_EVIDENCE" "$TEST_PHASE2_RECORD_REPLAY"
bd dep add "$DOC_PHASE_EVIDENCE" "$TEST_PHASE2_FRAME_NET"
bd dep add "$DOC_PHASE_EVIDENCE" "$TEST_PHASE2_FORK_VERIFY"
bd dep add "$DOC_PHASE_EVIDENCE" "$TEST_NANOKERNEL_PRESERVE"
bd dep add "$DOC_OPS_CI" "$IMPL_CLI_LINUX"
bd dep add "$DOC_OPS_CI" "$TEST_PHASE1_LINUX_GATE"
bd dep add "$DOC_OPS_CI" "$TEST_PHASE2_FORK_VERIFY"
bd dep add "$DOC_DIVERGENCES" "$DECIDE_READY_AND_BLOCK"
bd dep add "$DOC_DIVERGENCES" "$IMPL_LAPC_PERSISTENCE"
bd dep add "$DOC_DIVERGENCES" "$DOC_OPS_CI"

bd dep add "$FINAL_ACCEPTANCE" "$DOC_PHASE_EVIDENCE"
bd dep add "$FINAL_ACCEPTANCE" "$DOC_OPS_CI"
bd dep add "$FINAL_ACCEPTANCE" "$DOC_DIVERGENCES"
bd dep add "$FINAL_ACCEPTANCE" "$TEST_LINUX_WORKER_API"

echo "Created M9 bead graph rooted at $M9_EPIC"
