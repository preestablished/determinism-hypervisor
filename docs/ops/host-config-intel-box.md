# Host configuration runbook — Intel box (`infra-control`)

Implements **ARCH §7.4** (host configuration, deployment requirement). Every item
here is re-checked at service startup by `dh-workerd --preflight`
(determinism-hypervisor-txb); this runbook is the human-side procedure for putting a
host into a compliant state and recording its determinism-class baseline.

Hardware: Intel Core i5-8400 (Coffee Lake, family 6 model 158 stepping 10),
6 physical cores, **no SMT** (6C/6T), single socket. Ubuntu, kernel 6.8.x.

## Core allocation decision

| Cores | Role |
|---|---|
| 0–1 | Housekeeping: kernel threads, IRQs, sshd, `dh-workerd` control plane |
| 2–5 | **Slot cores**: isolated, one guest vCPU thread pinned per core |

Rationale: 4 slots matches the M-milestone fork-verify workloads; two housekeeping
cores keep the dirty-ring harvest and snapshot I/O off the slot cores. SMT-sibling
idling (ARCH §7.4) is moot on this part — there are no siblings.

The core list is defined **once**, as `SLOT_CORES` in
[`apply-host-config.sh`](./apply-host-config.sh); the `--verify` mode checks against
that same constant, so this document's `2-5` literals are descriptive, not load-bearing.

## §7.4 requirement → state audit (2026-06-09, pre-apply)

| # | Requirement | Current state | Verdict |
|---|---|---|---|
| 1 | `isolcpus=managed_irq,domain,2-5 nohz_full=2-5 rcu_nocbs=2-5` on kernel cmdline | cmdline has none of the three (`/proc/cmdline`); `/sys/devices/system/cpu/isolated` empty | ❌ apply + reboot |
| 1b | Non-managed IRQs off the slot cores — `isolcpus=managed_irq` only steers *managed* IRQs; legacy/driver IRQs follow the default affinity mask | default mask is all CPUs; irqbalance **not installed** (nothing re-spreads IRQs after boot) | ❌ apply (`irqaffinity=0-1` cmdline) |
| 2 | SMT siblings of slot cores idle (or SMT off) | `/sys/devices/system/cpu/smt/control` = `notsupported` (i5-8400 has no HT) | ✅ satisfied by hardware |
| 3 | `kvm_intel` PML enabled (`enable_pml=1` in ARCH; the 6.x param name is **`pml`**) | `/sys/module/kvm_intel/parameters/pml` = `Y` (default) | ✅ — pinned in modprobe.d anyway: a future kernel flipping the default is drift the determinism-class lock cannot catch, since module defaults aren't recorded |
| 4 | `kvm_intel ple_gap=0` (PLE exits off) | `ple_gap` = `128` | ❌ apply |
| 5 | THP off for slot memfds (`MADV_NOHUGEPAGE` convention) | THP mode = `madvise` → memfds get no THP unless code opts in; `dh-workerd` additionally calls `MADV_NOHUGEPAGE` on every slot mapping | ✅ — preflight must assert mode is `madvise` or `never` |
| 6 | `kernel.nmi_watchdog=0` (frees a PMU counter, stops host NMI injection — gates M2 skid acceptance) | `1` | ❌ apply |
| 7 | `perf_event_paranoid` permitting `perf_event_open` by the service user | `4` (Ubuntu hardening level — denies all unprivileged use) | ❌ apply |
| 8 | Pin kernel + microcode as determinism-class baseline | kernel `6.8.0-88-generic`, microcode `0xfa` | ✅ baselined — superseded post-reboot by the `-124` re-baseline (see below); `ci/determinism-class.lock` is the source of truth |

### CPU frequency decision (not a §7.4 bullet — decided, not omitted)

intel_pstate runs `powersave` on this box, so core frequency floats run-to-run.
**Determinism does not depend on frequency**: the virtual-time primitive is
INST_RETIRED, which counts instructions, not cycles — a slower run retires the same
instruction stream. Frequency *does* affect the nightly perf gates (criterion
benches, regression >20% fails). Decision: the apply script pins the `performance`
governor on all cores via a boot-time systemd oneshot
(`determinism-hypervisor-cpufreq.service`). Turbo remains enabled — per-bench
variance from turbo is within the 20% gate margin and the 2× headroom in ARCH §10;
revisit (`intel_pstate/no_turbo`) only if perf-gate flakiness shows up in practice.

## Apply procedure (root required)

Run [`apply-host-config.sh`](./apply-host-config.sh) as root, then **reboot** (the
cmdline isolation flags only take effect at boot; `update-grub` rewrites `grub.cfg`
immediately but `/proc/cmdline` is unchanged until then):

```bash
sudo bash docs/ops/apply-host-config.sh
sudo reboot
```

What it does, file by file — all idempotent (whole-file overwrites, never appends),
nothing outside these five touchpoints:

1. **`/etc/default/grub.d/99-determinism-hypervisor.cfg`** — appends
   `isolcpus=managed_irq,domain,2-5 nohz_full=2-5 rcu_nocbs=2-5 irqaffinity=0-1
   nmi_watchdog=0` to `GRUB_CMDLINE_LINUX_DEFAULT` via a drop-in (does not edit
   `/etc/default/grub`), then runs `update-grub`. `irqaffinity=0-1` sets the
   default affinity mask for non-managed IRQs; `nmi_watchdog=0` on the cmdline
   disables the hard-lockup detector earlier than the sysctl alone.
2. **`/etc/modprobe.d/determinism-hypervisor.conf`** —
   `options kvm_intel pml=1 ple_gap=0`. The script first asserts both param names
   exist on the running kernel (the spec's `enable_pml` already renamed to `pml`
   once — a stale name here would break KVM at next boot), then reloads
   `kvm_intel` immediately if refcount is 0; a failed reload is a loud warning,
   and the reboot picks the params up regardless.
3. **`/etc/sysctl.d/99-determinism-hypervisor.conf`** —
   `kernel.nmi_watchdog=0` and `kernel.perf_event_paranoid=1`, applied immediately
   with `sysctl --system`. Paranoid level 1 lets the (non-root) service user open
   CPU-scoped counting events; we deliberately avoid `CAP_PERFMON` file caps so the
   `dh-workerd` binary stays cap-free and replaceable by `cargo build` output.
4. **`/etc/systemd/system/determinism-hypervisor-cpufreq.service`** — boot-time
   oneshot pinning the `performance` governor (see frequency decision above);
   enabled and started immediately.
5. **THP** — no change needed while the distro default is `madvise`; the script
   asserts this and fails loudly if some prior tuning set `always`.

The script refuses to run on any host that is not family 6 / model 158 / 6 CPUs —
a different CPU is a different determinism class and needs a deliberate re-baseline,
not a silent config copy.

## Post-reboot verification

```bash
bash docs/ops/apply-host-config.sh --verify   # read-only, non-root, exit 0 = compliant
```

This checks, against the script's own `SLOT_CORES`: `cpu.isolated`, `cpu.nohz_full`,
`kvm_intel.ple_gap=0`, `kvm_intel.pml=Y`, `nmi_watchdog=0`, `perf_event_paranoid=1`,
THP mode `madvise`/`never`, `performance` governor, and that **no IRQ's
`effective_affinity_list` includes a slot core**. `dh-workerd --preflight`
automates the same list once it lands. Residual, accepted: nohz_full cores still
see a ~1/s housekeeping tick; zero-tick is not achievable.

## Determinism-class baseline & re-baseline procedure

The host tuple is pinned in [`ci/determinism-class.lock`](../../ci/determinism-class.lock)
(key=value; parse contract in the file header). **At baseline time**, hold the
packages that define the class so unattended upgrades can't silently move it:

```bash
sudo apt-mark hold linux-image-generic linux-generic intel-microcode
```

The nightly compares the live host against the lock and **fails on drift** — that is
the designed tripwire, and the hold is what makes drift a deliberate act.

This tripwire already fired once, at apply time (2026-06-09): the §7.4 reboot booted
the already-installed `6.8.0-124-generic` instead of the `6.8.0-88-generic` the
pre-apply audit recorded (the audit table above is the historical pre-apply
snapshot). No corpus or recorded icounts existed yet, so the lock was simply
re-baselined to `-124` — and the apt-mark hold matters precisely because the next
such surprise will not be free.

To absorb a deliberate kernel/microcode update:

1. `sudo apt-mark unhold ...`, upgrade, reboot.
2. Re-run the counting-semantics empirics test and re-baseline the record/replay
   corpus (IMPLEMENTATION-PLAN, host-environment pinning — a bump is a *procedure*,
   not an incident).
3. Regenerate the lock values with the exact extraction commands below (they emit
   the lock's own formats byte-for-byte) and commit the lock change in the same
   commit as the corpus re-baseline. Re-hold the packages.

```bash
# cpu_vendor / cpu_family / cpu_model_id / cpu_stepping / cpu_brand
awk -F': ' '/^vendor_id/{print "cpu_vendor="$2; exit}'   /proc/cpuinfo
awk -F': ' '/^cpu family/{print "cpu_family="$2; exit}'  /proc/cpuinfo
awk -F': ' '/^model\t/{print "cpu_model_id="$2; exit}'   /proc/cpuinfo
awk -F': ' '/^stepping/{print "cpu_stepping="$2; exit}'  /proc/cpuinfo
awk -F': ' '/^model name/{print "cpu_brand="$2; exit}'   /proc/cpuinfo
# microcode
awk -F': ' '/^microcode/{print "microcode="$2; exit}'    /proc/cpuinfo
# host_kernel
echo "host_kernel=$(uname -r)"
```
