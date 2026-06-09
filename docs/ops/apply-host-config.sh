#!/usr/bin/env bash
# Apply ARCH §7.4 host configuration on the Intel box (infra-control).
# Companion to host-config-intel-box.md — read that first. Idempotent; run as root.
# A reboot is required afterwards for the cmdline isolation flags.
#
# Usage:
#   sudo bash apply-host-config.sh            # apply (root)
#   bash apply-host-config.sh --verify        # read-only post-reboot check
set -euo pipefail

SLOT_CORES="2-5"        # single source of truth — docs and --verify derive from this
HOUSEKEEPING="0-1"

# --- read-only verification mode -------------------------------------------
verify() {
  local fail=0
  check() { # label actual expected
    if [[ "$2" == "$3" ]]; then
      printf 'ok    %-28s %s\n' "$1" "$2"
    else
      printf 'FAIL  %-28s got [%s] want [%s]\n' "$1" "$2" "$3"; fail=1
    fi
  }
  check "cpu.isolated"        "$(cat /sys/devices/system/cpu/isolated)"  "$SLOT_CORES"
  check "cpu.nohz_full"       "$(cat /sys/devices/system/cpu/nohz_full)" "$SLOT_CORES"
  check "kvm_intel.ple_gap"   "$(cat /sys/module/kvm_intel/parameters/ple_gap 2>/dev/null || echo missing)" "0"
  check "kvm_intel.pml"       "$(cat /sys/module/kvm_intel/parameters/pml 2>/dev/null || echo missing)" "Y"
  check "nmi_watchdog"        "$(cat /proc/sys/kernel/nmi_watchdog)" "0"
  check "perf_event_paranoid" "$(cat /proc/sys/kernel/perf_event_paranoid)" "1"

  local thp; thp=$(grep -o '\[.*\]' /sys/kernel/mm/transparent_hugepage/enabled)
  if [[ "$thp" == "[madvise]" || "$thp" == "[never]" ]]; then
    printf 'ok    %-28s %s\n' "thp.mode" "$thp"
  else
    printf 'FAIL  %-28s got %s want [madvise] or [never]\n' "thp.mode" "$thp"; fail=1
  fi

  local gov; gov=$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null || echo none)
  check "cpufreq.governor(cpu0)" "$gov" "performance"

  # No IRQ may have a slot core in its effective affinity (irqaffinity= at
  # boot; per-driver overrides would show up here). The [$SLOT_CORES] glob
  # relies on single-digit CPU ids — guaranteed by the 6-CPU host guard.
  local irq_bad=""
  for f in /proc/irq/*/effective_affinity_list; do
    local v; v=$(cat "$f" 2>/dev/null) || continue
    case "$v" in
      *["$SLOT_CORES"]*) irq_bad+="${f%/effective_affinity_list}=${v} " ;;
    esac
  done
  if [[ -z "$irq_bad" ]]; then
    printf 'ok    %-28s none on slot cores\n' "irq.effective_affinity"
  else
    printf 'FAIL  %-28s on slot cores: %s\n' "irq.effective_affinity" "$irq_bad"; fail=1
  fi

  exit "$fail"
}
[[ "${1:-}" == "--verify" ]] && verify

# --- apply mode (root) ------------------------------------------------------
[[ $EUID -eq 0 ]] || { echo "must run as root (sudo); use --verify for the read-only check" >&2; exit 1; }

# Guard: this config is pinned to the baseline host (i5-8400, Coffee Lake,
# family 6 model 158, 6C/6T) — the same tuple ci/determinism-class.lock records.
if ! grep -q "GenuineIntel" /proc/cpuinfo; then
  echo "not an Intel host; refusing to apply" >&2
  exit 1
fi
FAMILY=$(awk -F': ' '/^cpu family/{print $2; exit}' /proc/cpuinfo)
MODEL=$(awk -F': ' '/^model\t/{print $2; exit}' /proc/cpuinfo)
if [[ "$FAMILY" != "6" || "$MODEL" != "158" ]]; then
  echo "expected family 6 model 158 (i5-8400 baseline), found family $FAMILY model $MODEL" >&2
  echo "this is a different determinism class — update SLOT_CORES, the runbook and ci/determinism-class.lock deliberately, then adjust this guard" >&2
  exit 1
fi
NPROC=$(nproc --all)
if [[ "$NPROC" -ne 6 ]]; then
  echo "expected 6 CPUs (i5-8400), found $NPROC — adjust SLOT_CORES before applying" >&2
  exit 1
fi

# THP must be madvise or never; we never set THP mode here, we only refuse to
# proceed if some prior tuning set 'always'.
THP=$(cat /sys/kernel/mm/transparent_hugepage/enabled)
if [[ "$THP" == *"[always]"* ]]; then
  echo "transparent_hugepage=always is set; fix that tuning first (expect madvise/never)" >&2
  exit 1
fi

# Param-rename guard: assert the kvm_intel params we are about to pin exist on
# the RUNNING kernel (enable_pml->pml already renamed once); writing a bad
# option to modprobe.d would break KVM at next boot instead of failing here.
if [[ -d /sys/module/kvm_intel ]]; then
  for p in pml ple_gap; do
    if [[ ! -f /sys/module/kvm_intel/parameters/$p ]]; then
      echo "kvm_intel has no '$p' parameter on this kernel — param renamed again? Fix the modprobe options below first." >&2
      exit 1
    fi
  done
else
  echo "kvm_intel not loaded; cannot validate param names — load it first (modprobe kvm_intel)" >&2
  exit 1
fi

# 1. GRUB cmdline drop-in: scheduler/tick/RCU isolation of the slot cores,
#    default IRQ affinity to housekeeping (isolcpus=managed_irq only steers
#    *managed* IRQs; irqaffinity= covers the rest at boot — irqbalance is not
#    installed on this box, so nothing re-spreads them later), and early
#    nmi_watchdog off. The 99- prefix sorts the drop-in after /etc/default/grub
#    has set GRUB_CMDLINE_LINUX_DEFAULT, so the append composes correctly.
GRUB_DROPIN=/etc/default/grub.d/99-determinism-hypervisor.cfg
cat > "$GRUB_DROPIN" <<EOF
# ARCH §7.4 — managed by determinism-hypervisor docs/ops/apply-host-config.sh
GRUB_CMDLINE_LINUX_DEFAULT="\$GRUB_CMDLINE_LINUX_DEFAULT isolcpus=managed_irq,domain,${SLOT_CORES} nohz_full=${SLOT_CORES} rcu_nocbs=${SLOT_CORES} irqaffinity=${HOUSEKEEPING} nmi_watchdog=0"
EOF
echo "wrote $GRUB_DROPIN"
update-grub

# 2. kvm_intel module params
MODPROBE_CONF=/etc/modprobe.d/determinism-hypervisor.conf
cat > "$MODPROBE_CONF" <<'EOF'
# ARCH §7.4 — PML on (dirty logging accel), PLE exits off (single vCPU guests)
options kvm_intel pml=1 ple_gap=0
EOF
echo "wrote $MODPROBE_CONF"

# Reload kvm_intel now if nothing is using it (otherwise the reboot picks the
# params up). Failure to reload is a warning, not an error — the reboot that
# the cmdline change already requires will load the new params anyway.
REFCNT=$(awk '$1=="kvm_intel"{print $3}' /proc/modules || true)
if [[ "$REFCNT" == "0" ]]; then
  if modprobe -r kvm_intel && modprobe kvm_intel; then
    echo "kvm_intel reloaded: ple_gap=$(cat /sys/module/kvm_intel/parameters/ple_gap) pml=$(cat /sys/module/kvm_intel/parameters/pml)"
  else
    echo "WARNING: kvm_intel reload failed (module became busy?); params apply at reboot" >&2
    modprobe kvm_intel || true   # never leave the module unloaded
  fi
else
  echo "kvm_intel in use (refcnt=${REFCNT:-?}); new params apply at reboot"
fi

# 3. sysctls (effective immediately and persisted)
SYSCTL_CONF=/etc/sysctl.d/99-determinism-hypervisor.conf
cat > "$SYSCTL_CONF" <<'EOF'
# ARCH §7.4 — free the PMU counter / stop host NMI injection; let the service
# user perf_event_open CPU counting events without file caps.
kernel.nmi_watchdog = 0
kernel.perf_event_paranoid = 1
EOF
echo "wrote $SYSCTL_CONF"
sysctl --system >/dev/null
sysctl kernel.nmi_watchdog kernel.perf_event_paranoid

# 4. CPU frequency: pin the performance governor on all cores (intel_pstate
#    default here is powersave). Determinism itself is frequency-independent
#    (INST_RETIRED counts instructions, not cycles) — this is for the perf
#    gates' run-to-run stability (runbook, "CPU frequency decision").
FREQ_UNIT=/etc/systemd/system/determinism-hypervisor-cpufreq.service
cat > "$FREQ_UNIT" <<'EOF'
# ARCH §7.4 companion — managed by determinism-hypervisor docs/ops/apply-host-config.sh
[Unit]
Description=Pin performance cpufreq governor for determinism-hypervisor perf gates
After=multi-user.target

[Service]
Type=oneshot
ExecStart=/bin/sh -c 'for g in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do echo performance > "$g"; done'

[Install]
WantedBy=multi-user.target
EOF
echo "wrote $FREQ_UNIT"
systemctl daemon-reload
systemctl enable --now determinism-hypervisor-cpufreq.service
echo "governor(cpu0)=$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor)"

echo
echo "Done. /proc/cmdline is unchanged until reboot (update-grub only rewrote"
echo "grub.cfg) — reboot now to apply the isolation flags:"
echo "  sudo reboot"
echo "Then verify:  bash docs/ops/apply-host-config.sh --verify"
