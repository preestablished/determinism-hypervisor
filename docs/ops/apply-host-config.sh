#!/usr/bin/env bash
# Apply ARCH §7.4 host configuration on the Intel box (infra-control).
# Companion to host-config-intel-box.md — read that first. Idempotent; run as root.
# A reboot is required afterwards for the cmdline isolation flags.
set -euo pipefail

SLOT_CORES="2-5"

[[ $EUID -eq 0 ]] || { echo "must run as root (sudo)" >&2; exit 1; }

# Guard: this config is host-specific (core list assumes 6C/6T i5-8400 layout).
if ! grep -q "GenuineIntel" /proc/cpuinfo; then
  echo "not an Intel host; refusing to apply" >&2
  exit 1
fi
NPROC=$(nproc --all)
if [[ "$NPROC" -ne 6 ]]; then
  echo "expected 6 CPUs (i5-8400), found $NPROC — adjust SLOT_CORES before applying" >&2
  exit 1
fi

# THP must be madvise or never; we never set 'always' right, we only refuse it.
THP=$(cat /sys/kernel/mm/transparent_hugepage/enabled)
if [[ "$THP" == *"[always]"* ]]; then
  echo "transparent_hugepage=always is set; fix that tuning first (expect madvise/never)" >&2
  exit 1
fi

# 1. GRUB cmdline drop-in (isolation + early nmi_watchdog off)
GRUB_DROPIN=/etc/default/grub.d/99-determinism-hypervisor.cfg
cat > "$GRUB_DROPIN" <<EOF
# ARCH §7.4 — managed by determinism-hypervisor docs/ops/apply-host-config.sh
GRUB_CMDLINE_LINUX_DEFAULT="\$GRUB_CMDLINE_LINUX_DEFAULT isolcpus=managed_irq,domain,${SLOT_CORES} nohz_full=${SLOT_CORES} rcu_nocbs=${SLOT_CORES} nmi_watchdog=0"
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

# Reload kvm_intel now if nothing is using it (otherwise the reboot picks it up).
if lsmod | grep -q '^kvm_intel' && [[ "$(lsmod | awk '$1=="kvm_intel"{print $3}')" == "0" ]]; then
  modprobe -r kvm_intel && modprobe kvm_intel
  echo "kvm_intel reloaded: ple_gap=$(cat /sys/module/kvm_intel/parameters/ple_gap) pml=$(cat /sys/module/kvm_intel/parameters/pml)"
else
  echo "kvm_intel in use; new params apply at reboot"
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

echo
echo "Done. Reboot now to apply the cmdline isolation flags:"
echo "  sudo reboot"
echo "Then verify per docs/ops/host-config-intel-box.md (post-reboot verification)."
