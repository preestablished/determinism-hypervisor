# Phase 1 exit gate — sign-off record (bead dk1)

**Date:** 2026-06-10. **Host:** infra-control (i5-8400, kernel
6.8.0-124-generic, microcode 0xfa — `ci/determinism-class.lock`).
Every item below was re-run LIVE on this date for this sign-off, not
quoted from history.

This is the SEQUENCING GUARD from the phase doc: **hypervisor M4
(snapshots) MUST NOT start until this gate closes** — snapshotting a
nondeterministic VM produces unfalsifiable bugs. With this record, the
guard is satisfied.

**Scope:** the phase doc's full Phase-1 exit gate has four criteria.
This record covers the two owned by THIS repo (the determinism gate
and the landing/counting machinery it depends on). Criteria 3
(snapshot-store M1/M2 benchmark gates) and 4 (guest-sdk agent boots
in-guest and streams logs host-ward) are owned by the sibling
`snapshot-store` and `guest-sdk` repos and tracked there — out of
scope here by ownership, not forgotten. The M4 sequencing guard this
document discharges is the determinism guard, which is fully proven
below.

| # | Gate | Evidence (fresh run, 2026-06-10) |
|---|---|---|
| 1 | Determinism gate 100/100, zero divergence | `dh-cli gate --runs 100`: `PHASE-1 DETERMINISM GATE: PASS (100 runs each)` — plain + timer sub-gates, every fingerprint within each sub-gate identical (timer sub-gate: icount 2,000,000; state hash `7e09ac13…`; timer delivered at 1,234,567; the plain sub-gate has its own distinct fingerprint, equally invariant) |
| 2 | Landing gate: 10,000 targets exact, zero overshoots, incl. REP boundaries | `cargo test -p determinism-tests --test landing_precision`: 2/2 ok — 10,000 targets `icount == N` exactly; 1,000-target REP MOVSB torture (RCX mid-REP detector: no boundary ever mid-REP); tuples bit-identical across boots at margins 8192/256 vs 128 |
| 3 | Max skid < skid_margin/2, histogram archived | `dh-cli skid --samples 50000`: max 79 < 4096; full Prometheus-style histogram archived at [`docs/ops/skid-histogram-2026-06-10.txt`](ops/skid-histogram-2026-06-10.txt) (49,997/50,000 ≤ 31) |
| 4 | counting_semantics green | `cargo test -p determinism-tests --test counting_semantics`: 2/2 ok — per-instruction §3.1 attribution (REP retires once; CPUID/PIO/MMIO/HLT retire zero, measured), trace replays bit-identically |
| 5 | M3 accepts green | `timer_determinism` (100 runs × 10 fires, zero divergence, ~95 s) ok; `if0_deferral` ok; `regression` (both tests: 1e9 ×2 and the 10M-twice companion, state-hash chains identical, ~4–6 s) ok; `m1_acceptance` (full device surface, run-twice bit-identical incl. device snapshots) ok |
| 6 | CI determinism regression required-for-merge and green | Branch protection live: `kvm-intel` + both host legs are required checks (`ci/branch-protection.json`); latest main run 27284395335 SUCCESS; nightly drift + canary wired (`nightly-drift.yaml`) |
| 7 | TSC alignment decision recorded with measured numbers | [`docs/decisions/tsc-alignment.md`](decisions/tsc-alignment.md): KVM_VCPU_TSC_OFFSET device attr chosen; 932 vs 1107 ns/call measured, MSR-path sync-heuristic hazard documented |

## M9 pre-Linux baseline refresh (2026-06-18)

**Host:** infra-control, Linux `6.8.0-124-generic`, Intel(R) Core(TM)
i5-8400 CPU @ 2.80GHz, microcode `0xfa`; live values match
`ci/determinism-class.lock`.

Before M9 Linux edits, the nanokernel default gate was rerun on this
host:

| Command | Evidence |
|---|---|
| `cargo run -p dh-cli -- gate --runs 100` | PASS: `PHASE-1 DETERMINISM GATE: PASS (100 runs each)` |
| `dh-cli gate` default mode | Still nanokernel-only: `tools/dh-cli/src/gate.rs` boots `nanokernel::landing_loop_elf()` for `plain-landing` and `nanokernel::timer_guest_elf()` for `timer-event`, both through `BootSpec::Elf` |
| `plain-landing` sub-gate | 100/100 identical: `icount=2000000`, `rip=0x1000b4`, `vns=2000000`, state hash `64eecca97eed5c9a3f75c14d76bc6d6a810242ad31366ba84fe4168d72ec6b6a`, `timer=None` |
| `timer-event` sub-gate | 100/100 identical: `icount=2000000`, `rip=0x1000ea`, `vns=2000000`, state hash `25ec9e0e8cf4389caeb7b6c7714c2f647cea49a089289d1c52d82c98f993fc88`, `timer=Some(1234567)` |

## M9 post-Linux nanokernel preservation (2026-06-20)

**Host:** infra-control, Linux `6.8.0-124-generic`, Intel(R) Core(TM)
i5-8400 CPU @ 2.80GHz, microcode `0xfa`; `bash
ci/check-determinism-class.sh` reported all 7 lock keys matched. `/dev/kvm`
was present and rw, `nasm` was `/usr/bin/nasm`, and `taskset -c 2-5` reported
`Cpus_allowed_list: 2-5`.

This addendum proves the original nanokernel Phase 1 gate and its supporting
tests still run after the Linux guest path landed. The Linux rollup below
completes the M9 Phase 1 exit-gate update tracked by
`determinism-hypervisor-4s9.32`.

| Command | Evidence |
|---|---|
| `cargo test --workspace` | PASS. The workspace suite included the live Phase 1 KVM tests, nanokernel crate tests, worker snapshot/fork/replay tests, and the checked-in `pad_echo_6s` corpus reverify. Linux artifact acceptance tests remained ignored in the ordinary workspace run. |
| `cargo run -p dh-cli -- gate --runs 100` | PASS: `PHASE-1 DETERMINISM GATE: PASS (100 runs each)`. The command did not pass `--linux`, so `tools/dh-cli/src/gate.rs` used `BootSpec::Elf` with `nanokernel::landing_loop_elf()` for `plain-landing` and `nanokernel::timer_guest_elf()` for `timer-event`. |
| `plain-landing` sub-gate | 100/100 identical: `icount=2000000`, `rip=0x1000b4`, `vns=2000000`, state hash `64eecca97eed5c9a3f75c14d76bc6d6a810242ad31366ba84fe4168d72ec6b6a`, `timer=None`. |
| `timer-event` sub-gate | 100/100 identical: `icount=2000000`, `rip=0x1000ea`, `vns=2000000`, state hash `02bb7b547cff16219356bc26c6b782af07eb72c2e08331a995aef16e52c85113`, `timer=Some(1234567)`. |
| `cargo test -p determinism-tests --test regression --test timer_determinism --test if0_deferral --test landing_precision --test counting_semantics --test counting_smoke --test m1_acceptance` | PASS. `counting_semantics` 2/2, `counting_smoke` 1/1, `if0_deferral` 5/5, `landing_precision` 2/2, `m1_acceptance` 1/1, `regression` 2/2, `timer_determinism` 5/5. |

Fixture preservation checks before the documentation edits showed no changes
under `tests/nanokernel/**` or
`crates/dh-worker/tests/fixtures/record_replay_corpus/pad_echo_6s/**`.

## M9 Linux Phase 1 rollup (2026-06-20)

**Host:** infra-control, Linux `6.8.0-124-generic`, Intel(R) Core(TM)
i5-8400 CPU @ 2.80GHz, microcode `0xfa`. These Linux gates are
artifact-backed operator-run acceptance gates on the `kvm-intel` host. Final
M9 Phase 1 evidence must use `DH_M9_ALLOW_SKIP=0`; any `*_ALLOW_SKIP=1`
run is guard-path evidence only and is rejected for acceptance.

The accepted 4s9.24, 4s9.25, and 4s9.26 producer evidence used this staged
artifact set:

| Artifact | BLAKE3 |
|---|---|
| `bzImage` | `595466463a37efac6822ffccf3e61d0a2230e7d223a94c0bce5eb78b2f43bee9` |
| `initramfs.cpio` | `f130e1a329bf934651d89dccdec0a2dccd33862319cbbe95c30e0505382d12d4` |
| `base.img` | `488de202f73bd976de4e7048f4e1f39a776d86d582b7348ff53bf432b987fca8` |
| `game.img` | `e02849845005d9d34fa3245d98fa59116a0245ed0136b496dbd2defebdc203ac` |

Later M4/M5/M7 Linux evidence in the Phase 2 rollup uses a newer reference-workload
`initramfs.cpio` hash. The gate records intentionally keep hashes attached
to the command evidence that produced them.

| Gate | Command | Evidence |
|---|---|---|
| Linux Phase 1 CLI Ready/post-READY gate | `DH_M9_ALLOW_SKIP=0 cargo run -p dh-cli -- gate --linux --runs 100 --bzimage "$DH_M9_BZIMAGE" --initramfs "$DH_M9_INITRAMFS" --base-image "$DH_M9_BASE_IMAGE" --game-image "$DH_M9_GAME_IMAGE"` | PASS: `gate linux-phase1 runs=100 verdict=PASS`; 100/100 zero divergence. All runs matched `ready_event_kind=14`, `ready_unit=0`, `ready_region_count=3`, `ready_manifest_generation=6`, Ready payload digest `ddf4f8ffe8774c4ca4a78226302fefb0a67b1425da7940233a8ba4be99efdc16`, `ready_icount=641326674`, Ready state hash `f8b813ed797076ba9e5233e3a9bef09c6c4162abbd9e71fb8e721aa14fedc8e3`, `config_hash=e3391619f4c3af368e418febd94b511b668a9b6b7f6211c5e09d754ca0d03da6`, `post_ready_budget=2000000`, `post_ready_icount=643326674`, and post-READY state hash `b28f9c23a421ee04571bab4e4c94d243400a924e727a1f589a4516d24d17933c`. |
| Linux timer/IRQ determinism | `DH_M9_ALLOW_SKIP=0 cargo test -p determinism-tests --test linux_timer_determinism --release -- --ignored --nocapture` | PASS: 100 cold Linux cases; `ready_icount=641326674`, vector `241`, delivered icounts `[642326674, 643326674, 644326674]`, final state hash `34bd14779d12c6005d2d16541bf40d8880b43cd720adcf5a6178327bb7b99dfe`. The gate compares delivered icount list, timer source/vector/deadline metadata, and final state hash, and fails if kvmclock, TSC-deadline, x2APIC, PIT, IOAPIC, or in-kernel irqchip surfaces appear. |
| Linux landing/counting | `DH_M9_ALLOW_SKIP=0 cargo test -p determinism-tests --test linux_landing_counting --release -- --ignored --nocapture` | PASS: 100 exact post-READY targets across two cold boots with identical `(icount, rip, rcx, state_hash, timer metadata)` tuples. Evidence: `ready_icount=641326674`, `timer_vector=241`, `timer_delivered_icount=642326674`, first target hash `7e1ac064d87ea33022b083f48aeeeb7617583e82da2bd19858dec80ec392a1c7`, last target hash `65602f999f0e0b5cdbf2dd09cd9ad13d2c151a38d4f0844e1c066a8136550d5c`. |

CI and nightly classification lives in
[`docs/ops/test-partitioning.md`](ops/test-partitioning.md) and
[`docs/ops/github-runner.md`](ops/github-runner.md): the Linux artifact-backed
Phase 1 gates are operator-run acceptance gates, while the nanokernel/default
workspace and `dh-cli gate --runs 100` coverage remains separate regression
coverage.

## M9 final acceptance suite (2026-06-21)

`determinism-hypervisor-4s9.35` was rerun end-to-end on the reference
Linux/KVM host before closing M9. The tested code was commit
`f855dfb9800e969e8371016112aace7703ee402d`; later commits are docs-only
evidence publication. Raw local transcripts are under
`target/m9-final-acceptance-20260621T004402Z`; that path is local operator
scratch, not a repo-tracked artifact. The durable audit record is this
committed summary plus the Beads closeout comment on
`determinism-hypervisor-4s9.35`.

**Host:** infra-control, Linux `6.8.0-124-generic`, Intel(R) Core(TM)
i5-8400 CPU @ 2.80GHz, microcode `0xfa`; `ci/determinism-class.lock` matched
the live host. `docs/ops/apply-host-config.sh --verify` passed, including
`/dev/kvm` access for `infra-admin`, housekeeping CPUs 0-1, and isolated/nohz
slot cores 2-5. `cargo run -p dh-worker --bin dh-workerd -- --preflight`
reported `preflight OK`. The shell was pinned to `Cpus_allowed_list: 0-1`,
and `taskset -c 2-5` children reported `Cpus_allowed_list: 2-5`.

The repository self-hosted runner listener was suspended locally while this
exclusive KVM suite ran because passwordless sudo was unavailable for stopping
the systemd service. The transcript set records listener PID 808415 and the
stopped `Tl` state used for the reservation. The listener was resumed after
the KVM gates as session hygiene; no CI workflow run is used as acceptance
evidence for this final suite.

The final suite rehashed the current staged reference-workload image artifacts
below. The `m9-refwork-contract` workload binary was validated from inside the
initramfs by the fixture-contract test, but was not materialized and rehashed
as a separate file in this final run. The `initramfs.cpio` hash intentionally
differs from the earlier Phase 1 producer row above; the earlier row is left
attached to its original evidence.

| Artifact | Path | BLAKE3 |
|---|---|---|
| `bzImage` | `/home/infra-admin/.cache/dh-m9/reference-workload/bzImage` | `595466463a37efac6822ffccf3e61d0a2230e7d223a94c0bce5eb78b2f43bee9` |
| `initramfs.cpio` | `/home/infra-admin/.cache/dh-m9/reference-workload/initramfs.cpio` | `87edf64db22dc85ef0c6b17fdc6e58a8f73391a6053e96f7a1056da7d08b9f57` |
| `base.img` | `/home/infra-admin/.cache/dh-m9/reference-workload/base.img` | `488de202f73bd976de4e7048f4e1f39a776d86d582b7348ff53bf432b987fca8` |
| `game.img` | `/home/infra-admin/.cache/dh-m9/reference-workload/game.img` | `e02849845005d9d34fa3245d98fa59116a0245ed0136b496dbd2defebdc203ac` |

| Gate | Command | Evidence |
|---|---|---|
| Workspace regression suite | `cargo test --workspace` | PASS across the workspace during the final suite. |
| Nanokernel Phase 1 CLI gate | `cargo run -p dh-cli -- gate --runs 100` | PASS: `PHASE-1 DETERMINISM GATE: PASS (100 runs each)`. `plain-landing` matched `icount=2000000`, `rip=0x1000b4`, `vns=2000000`, state hash `64eecca97eed5c9a3f75c14d76bc6d6a810242ad31366ba84fe4168d72ec6b6a`; `timer-event` matched `icount=2000000`, `rip=0x1000ea`, `vns=2000000`, state hash `02bb7b547cff16219356bc26c6b782af07eb72c2e08331a995aef16e52c85113`, timer `1234567`. |
| Linux fixture contract | `DH_M9_ALLOW_SKIP=0 cargo test -p determinism-tests --test linux_fixture_contract -- --ignored --nocapture` | PASS: `M9 initramfs contract ok`, autostart unit 0, exec `/opt/m9-refwork-contract`, expected regions `framebuffer`, `meta`, and `wram`; `1 passed`, no skips accepted. |
| Linux Ready fixture | `DH_M9_ALLOW_SKIP=0 cargo test -p determinism-tests --test linux_ready --release -- --ignored --nocapture` | PASS: `ready_icount=641343512`, `unit=0`, `region_count=3`, `manifest_generation=6`, `machine_config_hash=2b638bdf9f61ea0b9c14958d48b9a0eda743ace322866fb90f5fc387256226e6`, Ready state hash `5449bd8fae5587b9f69542b9be646bf6a54a64cb7b323811418b208079c41fd5`; `1 passed`, no skips accepted. |
| Linux Phase 1 CLI Ready/post-READY gate | `DH_M9_ALLOW_SKIP=0 cargo run -p dh-cli -- gate --linux --runs 100 --bzimage "$DH_M9_BZIMAGE" --initramfs "$DH_M9_INITRAMFS" --base-image "$DH_M9_BASE_IMAGE" --game-image "$DH_M9_GAME_IMAGE"` | PASS: `gate linux-phase1 runs=100 verdict=PASS`; `M9 LINUX PHASE-1 GATE: PASS (100 runs, Ready EventKind 14, post-Ready budget 2000000)`. All runs matched `ready_event_kind=14`, `ready_unit=0`, `ready_region_count=3`, `ready_manifest_generation=6`, Ready payload digest `ddf4f8ffe8774c4ca4a78226302fefb0a67b1425da7940233a8ba4be99efdc16`, `ready_icount=641343512`, Ready state hash `5449bd8fae5587b9f69542b9be646bf6a54a64cb7b323811418b208079c41fd5`, config hash `2b638bdf9f61ea0b9c14958d48b9a0eda743ace322866fb90f5fc387256226e6`, `post_ready_budget=2000000`, `post_ready_icount=643343512`, and post-READY state hash `389518828fe672f094a8bc54068134186635175f69563052ddbb9a8fe18bd23a`. |
| Linux timer/IRQ determinism | `DH_M9_ALLOW_SKIP=0 cargo test -p determinism-tests --test linux_timer_determinism --release -- --ignored --nocapture` | PASS: 100 cold Linux cases; `ready_icount=641343512`, vector `241`, delivered icounts `[642343512, 643343512, 644343512]`, final state hash `af397aa09f3d568388fb0b8ab88dbb259d0a1020975f160973c1379fdd606b57`; `1 passed`, no skips accepted. |
| Linux landing/counting | `DH_M9_ALLOW_SKIP=0 cargo test -p determinism-tests --test linux_landing_counting --release -- --ignored --nocapture` | PASS: 100 exact post-READY targets; `ready_icount=641343512`, `timer_vector=241`, `timer_delivered_icount=642343512`, first target hash `79420d3ae1bcebe610188c1eb1e0e53db018feaefcfca62e9a78b8cf9db128ba`, last target hash `ee8cd0198d7cf95c4ae765dd46ced05d115b4d810e2fb1afd1b7cf72cf900281`; `1 passed`, no skips accepted. |

## Known refinements baked into the gate (not exceptions)

- §3.1 exit-instruction retirement is the MEASURED rule (retire zero,
  not "once"); the spec was reconciled (beads 0sc, gfb) and the
  empirics run in CI on every kernel/microcode bump.
- The single-step engine re-arms guest_debug after handled exits
  (MMIO-write exits eat the trap — found and fixed by the
  counting_semantics work, regression-pinned).
- The §7.2 CPUID mask is host-placement-invariant (APIC-ID byte and
  topology leaves zeroed — found by review, verified across all cores).

## Historical Phase 1 unblock context

At the original Phase 1 sign-off, M4 (snapshot/restore/fork) was allowed to
begin. The later M9 addenda above document that Linux M4/M5/M7 acceptance has
since landed. The restore-side preimages were already staged: §8.1 doc-order
MSR blob with the normalized-TSC slot, device snapshot sections (EVTC layout
v1, empty-serial rule), the SegmentHeader encoder fingerprint, and
`StateHashChain::from_value` for chain continuation.
