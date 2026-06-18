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

## Known refinements baked into the gate (not exceptions)

- §3.1 exit-instruction retirement is the MEASURED rule (retire zero,
  not "once"); the spec was reconciled (beads 0sc, gfb) and the
  empirics run in CI on every kernel/microcode bump.
- The single-step engine re-arms guest_debug after handled exits
  (MMIO-write exits eat the trap — found and fixed by the
  counting_semantics work, regression-pinned).
- The §7.2 CPUID mask is host-placement-invariant (APIC-ID byte and
  topology leaves zeroed — found by review, verified across all cores).

## What this unblocks

M4 (snapshot/restore/fork) may begin. The restore-side preimages are
already staged: §8.1 doc-order MSR blob with the normalized-TSC slot,
device snapshot sections (EVTC layout v1, empty-serial rule), the
SegmentHeader encoder fingerprint, and `StateHashChain::from_value`
for chain continuation.
