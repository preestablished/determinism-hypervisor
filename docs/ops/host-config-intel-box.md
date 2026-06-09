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

## §7.4 requirement → state audit (2026-06-09, pre-apply)

| # | Requirement | Current state | Verdict |
|---|---|---|---|
| 1 | `isolcpus=managed_irq,domain,2-5 nohz_full=2-5 rcu_nocbs=2-5` on kernel cmdline | cmdline has none of the three (`/proc/cmdline`); `/sys/devices/system/cpu/isolated` empty | ❌ apply + reboot |
| 2 | SMT siblings of slot cores idle (or SMT off) | `/sys/devices/system/cpu/smt/control` = `notsupported` (i5-8400 has no HT) | ✅ satisfied by hardware |
| 3 | `kvm_intel` PML enabled (`enable_pml=1` in ARCH; the 6.x param name is **`pml`**) | `/sys/module/kvm_intel/parameters/pml` = `Y` (default) | ✅ — pin it in modprobe.d anyway |
| 4 | `kvm_intel ple_gap=0` (PLE exits off) | `ple_gap` = `128` | ❌ apply |
| 5 | THP off for slot memfds (`MADV_NOHUGEPAGE` convention) | THP mode = `madvise` → memfds get no THP unless code opts in; `dh-workerd` additionally calls `MADV_NOHUGEPAGE` on every slot mapping | ✅ — preflight must assert mode is `madvise` or `never` |
| 6 | `kernel.nmi_watchdog=0` (frees a PMU counter, stops host NMI injection — gates M2 skid acceptance) | `1` | ❌ apply |
| 7 | `perf_event_paranoid` permitting `perf_event_open` by the service user | `4` (Ubuntu hardening level — denies all unprivileged use) | ❌ apply |
| 8 | Pin kernel + microcode as determinism-class baseline | kernel `6.8.0-88-generic`, microcode `0xfa` | ✅ recorded in `ci/determinism-class.lock` |

## Apply procedure (root required)

Run [`apply-host-config.sh`](./apply-host-config.sh) as root, then **reboot** (the
cmdline isolation flags only take effect at boot):

```bash
sudo bash docs/ops/apply-host-config.sh
sudo reboot
```

What it does, file by file — all idempotent, nothing outside these four touchpoints:

1. **`/etc/default/grub.d/99-determinism-hypervisor.cfg`** — appends
   `isolcpus=managed_irq,domain,2-5 nohz_full=2-5 rcu_nocbs=2-5 nmi_watchdog=0`
   to `GRUB_CMDLINE_LINUX_DEFAULT` via a drop-in (does not edit
   `/etc/default/grub`), then runs `update-grub`. `nmi_watchdog=0` on the cmdline
   disables the hard-lockup detector earlier than the sysctl alone.
2. **`/etc/modprobe.d/determinism-hypervisor.conf`** —
   `options kvm_intel pml=1 ple_gap=0`. Takes effect on next module load; the
   script reloads `kvm_intel` immediately if no VMs are running.
3. **`/etc/sysctl.d/99-determinism-hypervisor.conf`** —
   `kernel.nmi_watchdog=0` and `kernel.perf_event_paranoid=1`, applied immediately
   with `sysctl --system`. Paranoid level 1 lets the (non-root) service user open
   CPU-scoped counting events; we deliberately avoid `CAP_PERFMON` file caps so the
   `dh-workerd` binary stays cap-free and replaceable by `cargo build` output.
4. **THP** — no change needed while the distro default is `madvise`; the script
   asserts this and fails loudly if some prior tuning set `always`.

## Post-reboot verification

```bash
cat /sys/devices/system/cpu/isolated          # expect: 2-5
cat /sys/devices/system/cpu/nohz_full         # expect: 2-5
cat /sys/module/kvm_intel/parameters/ple_gap  # expect: 0
cat /sys/module/kvm_intel/parameters/pml      # expect: Y
sysctl kernel.nmi_watchdog                    # expect: 0
sysctl kernel.perf_event_paranoid             # expect: 1
cat /sys/kernel/mm/transparent_hugepage/enabled  # expect: [madvise] or [never]
```

`dh-workerd --preflight` automates exactly this list once it lands.

## Determinism-class baseline & re-baseline procedure

The host's `(cpu_model, microcode, host_kernel)` tuple is pinned in
[`ci/determinism-class.lock`](../../ci/determinism-class.lock). The nightly compares
the live host against the lock and **fails on drift** — that is the designed tripwire
for unattended `apt upgrade` pulling a new kernel or `intel-microcode` package.

To absorb a deliberate kernel/microcode update:

1. Hold packages until ready: `apt-mark hold linux-image-generic intel-microcode`.
2. When bumping: update, reboot, re-run the counting-semantics empirics test and
   re-baseline the record/replay corpus (IMPLEMENTATION-PLAN, host-environment
   pinning — a bump is a *procedure*, not an incident).
3. Update `ci/determinism-class.lock` with the new `uname -r` and
   `grep -m1 microcode /proc/cpuinfo` values in the same commit as the corpus
   re-baseline.
