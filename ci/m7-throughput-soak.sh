#!/usr/bin/env bash
# M7 throughput soak (bead 0kn): keep the M7 fork/VerifyReplay path under
# housekeeping-core stress for a sustained wall-clock window, then fail if the
# completed child-job rate is below N_slots x 1 job/s.

set -euo pipefail

slot_cores="${DH_M7_SOAK_SLOT_CORES:-2-5}"
housekeeping_cores="${DH_M7_SOAK_HOUSEKEEPING_CORES:-0-1}"
duration_seconds="${DH_M7_SOAK_SECONDS:-1800}"
target_millijobs_per_second="${DH_M7_SOAK_TARGET_MILLIJOBS_PER_SECOND:-}"
batch_jobs="${DH_M7_SOAK_BATCH_JOBS:-}"

usage() {
  cat <<'EOF'
Usage: ci/m7-throughput-soak.sh

Environment:
  DH_M7_SOAK_SLOT_CORES                  default: 2-5
  DH_M7_SOAK_HOUSEKEEPING_CORES          default: 0-1
  DH_M7_SOAK_SECONDS                     default: 1800
  DH_M7_SOAK_BATCH_JOBS                  default: slot_count * 10
  DH_M7_SOAK_TARGET_MILLIJOBS_PER_SECOND default: slot_count * 1000

The acceptance target is N_slots x 1 job/s for at least 30 minutes on the
kvm-intel host:

  DH_M7_SOAK_SECONDS=1800 ci/m7-throughput-soak.sh

Developer smoke on this constrained shell can lower the target explicitly, e.g.:

  DH_M7_SOAK_SECONDS=1 DH_M7_SOAK_SLOT_CORES=0-1 \
  DH_M7_SOAK_HOUSEKEEPING_CORES=0-1 DH_M7_SOAK_BATCH_JOBS=2 \
  DH_M7_SOAK_TARGET_MILLIJOBS_PER_SECOND=1 ci/m7-throughput-soak.sh
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

require_positive_int() {
  local name="$1"
  local value="$2"
  if [[ -z "$value" || "$value" == *[!0-9]* || "$value" == "0" ]]; then
    echo "::error::$name must be a positive integer, got '$value'" >&2
    exit 2
  fi
}

count_core_list() {
  local spec="$1"
  SPEC="$spec" python3 - <<'PY'
import os
import sys

spec = os.environ["SPEC"]
seen = set()
for part in spec.split(","):
    part = part.strip()
    if not part:
        raise SystemExit("empty core-list component")
    if "-" in part:
        lo_s, hi_s = part.split("-", 1)
        lo, hi = int(lo_s), int(hi_s)
        if lo > hi:
            raise SystemExit("descending core range")
        values = range(lo, hi + 1)
    else:
        values = [int(part)]
    for value in values:
        if value in seen:
            raise SystemExit("duplicate core")
        seen.add(value)
print(len(seen))
PY
}

require_positive_int DH_M7_SOAK_SECONDS "$duration_seconds"

slot_count="$(count_core_list "$slot_cores")" || {
  echo "::error::DH_M7_SOAK_SLOT_CORES must be a kernel-style core list, got '$slot_cores'" >&2
  exit 2
}
housekeeping_count="$(count_core_list "$housekeeping_cores")" || {
  echo "::error::DH_M7_SOAK_HOUSEKEEPING_CORES must be a kernel-style core list, got '$housekeeping_cores'" >&2
  exit 2
}

if [[ "$slot_count" -lt 2 ]]; then
  echo "::error::DH_M7_SOAK_SLOT_CORES must provide at least two slots" >&2
  exit 2
fi
if [[ "$housekeeping_count" -lt 1 ]]; then
  echo "::error::DH_M7_SOAK_HOUSEKEEPING_CORES must provide at least one core" >&2
  exit 2
fi

if [[ -z "$target_millijobs_per_second" ]]; then
  target_millijobs_per_second=$(( slot_count * 1000 ))
fi
require_positive_int DH_M7_SOAK_TARGET_MILLIJOBS_PER_SECOND "$target_millijobs_per_second"

if [[ -z "$batch_jobs" ]]; then
  batch_jobs=$(( slot_count * 10 ))
fi
require_positive_int DH_M7_SOAK_BATCH_JOBS "$batch_jobs"

command -v stress-ng >/dev/null || {
  echo "::error::stress-ng missing; see docs/ops/github-runner.md provisioning" >&2
  exit 2
}
command -v taskset >/dev/null || {
  echo "::error::taskset missing; util-linux is required for housekeeping pinning" >&2
  exit 2
}

echo "M7 throughput soak config:"
echo "  slot_cores=$slot_cores slot_count=$slot_count"
echo "  housekeeping_cores=$housekeeping_cores housekeeping_count=$housekeeping_count"
echo "  duration_seconds=$duration_seconds batch_jobs=$batch_jobs"
echo "  target_millijobs_per_second=$target_millijobs_per_second"

cargo test -p dh-worker --test m7_fork_verify --release --no-run

stress_timeout=$(( duration_seconds + 120 ))
taskset -c "$housekeeping_cores" \
  stress-ng --cpu "$housekeeping_count" --io 1 --vm 1 --vm-bytes 25% \
    --timeout "${stress_timeout}s" --metrics-brief &
stress_pid=$!

cleanup() {
  if kill -0 "$stress_pid" 2>/dev/null; then
    kill "$stress_pid" 2>/dev/null || true
    wait "$stress_pid" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

start_ns="$(date +%s%N)"
deadline_ns=$(( start_ns + duration_seconds * 1000000000 ))
jobs_done=0
iteration=0

while :; do
  now_ns="$(date +%s%N)"
  if [[ "$now_ns" -ge "$deadline_ns" && "$jobs_done" -gt 0 ]]; then
    break
  fi
  iteration=$(( iteration + 1 ))
  echo "M7 throughput soak batch $iteration: running $batch_jobs jobs"
  DH_M7_ACCEPT_JOBS="$batch_jobs" \
  DH_M7_ACCEPT_SLOT_CORES="$slot_cores" \
  DH_M7_ACCEPT_ALLOW_SKIP=0 \
    cargo test -p dh-worker --test m7_fork_verify --release -- --ignored --nocapture
  jobs_done=$(( jobs_done + batch_jobs ))
  elapsed_ns=$(( $(date +%s%N) - start_ns ))
  echo "M7 throughput soak progress: jobs_done=$jobs_done elapsed_seconds=$(( elapsed_ns / 1000000000 ))"
done

end_ns="$(date +%s%N)"
elapsed_ns=$(( end_ns - start_ns ))
if [[ "$elapsed_ns" -le 0 ]]; then
  echo "::error::elapsed time did not advance" >&2
  exit 1
fi

actual_millijobs_per_second=$(( jobs_done * 1000 * 1000000000 / elapsed_ns ))
required_jobs=$(( target_millijobs_per_second * elapsed_ns / 1000 / 1000000000 ))

echo "M7 throughput soak result:"
echo "  jobs_done=$jobs_done"
echo "  elapsed_seconds=$(( elapsed_ns / 1000000000 ))"
echo "  actual_millijobs_per_second=$actual_millijobs_per_second"
echo "  target_millijobs_per_second=$target_millijobs_per_second"
echo "  required_jobs_at_elapsed=$required_jobs"

if [[ "$actual_millijobs_per_second" -lt "$target_millijobs_per_second" ]]; then
  echo "::error::M7 throughput below target: ${actual_millijobs_per_second} mjob/s < ${target_millijobs_per_second} mjob/s" >&2
  exit 1
fi
