#!/usr/bin/env bash
# Determinism-class drift check (bead q10): compare the LIVE host against
# ci/determinism-class.lock per the lock's parse contract (# and blank
# lines ignored; split on the FIRST '='; values byte-exact).
#
# Exit 0 = host matches the baselined class. Any drift exits 1 with a
# per-key report and a pointer to the re-baseline procedure
# (docs/ops/host-config-intel-box.md) — drift is the designed tripwire,
# and a bump re-baselines the record/replay corpus; it is a procedure,
# not an incident.
set -euo pipefail

LOCK="$(dirname "$0")/determinism-class.lock"
[[ -f "$LOCK" ]] || { echo "::error::lock file missing: $LOCK"; exit 1; }

# Live extraction (the same commands documented in the runbook).
cpuinfo_field() { grep -m1 "^$1" /proc/cpuinfo | cut -d: -f2- | sed 's/^[[:space:]]*//'; }
live_value() {
    case "$1" in
        cpu_vendor)    cpuinfo_field "vendor_id" ;;
        cpu_family)    cpuinfo_field "cpu family" ;;
        cpu_model_id)  cpuinfo_field "model" ;;
        cpu_stepping)  cpuinfo_field "stepping" ;;
        cpu_brand)     cpuinfo_field "model name" ;;
        microcode)     cpuinfo_field "microcode" ;;
        host_kernel)   uname -r ;;
        *)             echo "<unknown key>" ;;
    esac
}

drift=0
checked=0
while IFS= read -r line; do
    [[ -z "$line" || "$line" == \#* ]] && continue
    key="${line%%=*}"
    want="${line#*=}"
    got="$(live_value "$key")"
    checked=$((checked + 1))
    if [[ "$got" != "$want" ]]; then
        echo "::error::determinism-class drift: $key: lock='$want' live='$got'"
        drift=1
    else
        echo "ok: $key=$want"
    fi
done < "$LOCK"

if [[ "$checked" -eq 0 ]]; then
    echo "::error::lock file parsed to zero keys — comparator or lock is broken"
    exit 1
fi
if [[ "$drift" -ne 0 ]]; then
    echo "::error::host drifted from ci/determinism-class.lock. If deliberate," \
         "follow the re-baseline procedure in docs/ops/host-config-intel-box.md" \
         "(unhold, upgrade, re-run counting-semantics empirics, re-baseline the" \
         "corpus, update the lock in a reviewed commit)."
    exit 1
fi
echo "determinism class matches the lock ($checked keys)."
