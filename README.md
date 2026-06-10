# determinism-hypervisor
Deterministic hypervisor; KVM VMM; x86_64 Linux only

## Workspace layout

The Cargo workspace follows `.agents/docs/determinism-hypervisor/ARCHITECTURE.md`
section 1:

- `crates/dh-vmm`: core VMM library. It owns the former `dh-types` slot-state
  scaffold and the former `dh-kvm` capability-check scaffold. Depends on
  `dh-detclock`/`dh-devices`/`dh-inputlog` per ARCH §1, plus `dh-proto` and
  `dh-snapshot` (a deliberate superset; ARCH §1's dependency line under-lists
  these two).
- `crates/dh-detclock`: guest instruction counter and PMI boundary timing home.
- `crates/dh-devices`: deterministic device-model home.
- `crates/dh-inputlog`: DHILOG core. Its dependency set is intentionally limited
  to `blake3`.
- `crates/dh-snapshot`: snapshot codec and dirty-page tracking home.
- `crates/dh-verify`: determinism verification and diagnostics home.
- `crates/dh-proto`: thin wrappers over the sibling
  `../control-plane/crates/determinism-proto` path dependency.
- `crates/dh-worker`: daemon layer. It may depend on all workspace crates;
  nothing depends on it — not other crates, and not `tools/dh-cli` either
  (ARCH §1: "nothing depends on `dh-worker`").
- `tools/dh-cli`: local debug CLI. Drives the VMM directly via `dh-vmm`.
- `tests/nanokernel` and `tests/determinism`: architecture test homes.

Disposition of the initial scaffold-only crates:

- `dh-types` was folded into `dh-vmm`; shared public types should be introduced
  only when a later architecture section requires them outside the VMM boundary.
- `dh-kvm` was folded into `dh-vmm`; KVM setup and capability policy are part of
  the VMM core in ARCH section 1.
- `dh-smoke` was retired as a crate; its smoke assertion moved into `dh-worker`
  package tests, with `tests/determinism` reserved for end-to-end gates.

## dh-cli (local debug CLI)

```text
dh-cli caps                      # §2.1 capability summary (kvm_m0_missing_caps=N)
dh-cli cpuid-diff                # supported vs §7.2-masked CPUID + table hash
dh-cli boot <guest.elf> [--mem-mib N] [--cmdline S] [--json]
dh-cli run  <guest.elf> (--icount-budget N | --vns-budget N) ...
dh-cli skid [--samples N]        # PMI skid histogram + margin gate
dh-cli gate [--runs N]           # the Phase-1 determinism gate (plain + timer)
```

x86_64-only (the binary prints an honest stub elsewhere). The embedded
test guests come from `tests/nanokernel`.

## Measured numbers (lab box: i5-8400, kernel 6.8.0-124, ucode 0xfa)

- **PMI skid**: typical 18–31, observed maxima 39–81 across separate
  50,000-sample runs (stochastic tail); production `skid_margin` 8192 —
  two orders of magnitude of headroom; gate alerts at margin/2.
- **TSC restore**: KVM_VCPU_TSC_OFFSET device attr chosen over
  MSR-write restore — 932 vs 1107 ns/call ioctl latency, and the MSR
  path additionally risks KVM's sync-heuristic value quantization
  (`docs/decisions/tsc-alignment.md`).
- **Landing**: 10,000 random targets in 100M instructions, zero
  overshoots; tuples bit-identical across two boots with different
  margins (first 100 targets at the production 8192 margin and the
  rest at 256 on boot A; all 10,000 re-landed at 128 on boot B) —
  §3.2 margin-independence proven live.
- **Run-twice regression**: 1e9 instructions ×2 from cold boot,
  state-hash chains identical, ~4 s.

## R2 status (INST_RETIRED counting empirics)

**PASSED with a measured refinement.** The §3.1 empirics
(`counting_semantics`, runs in CI on every kernel/microcode bump):
plain instructions +1; REP MOVSB retires exactly once on completion;
VM-exiting instructions (CPUID, PIO OUT, MMIO read/write, HLT) retire
**zero** — they exit before retirement and KVM completes them
host-side, invisible to the `exclude_host=1` counter. (The original
spec text claimed "exactly once"; the empirics refuted that and the
vendored ARCH §3.1 now records the measured rule.) Bit-stable across
cold boots, cores, processes, and load. The BR_INST_RETIRED fallback
(risk R2) has NOT been needed; the documented trigger is a
counting-semantics failure on a future kernel/microcode bump.

## More docs

- Test partitioning + Intel-box runbook: `docs/ops/test-partitioning.md`
- Merge policy / required checks: `CONTRIBUTING.md`
- Host config + determinism-class re-baseline: `docs/ops/host-config-intel-box.md`
