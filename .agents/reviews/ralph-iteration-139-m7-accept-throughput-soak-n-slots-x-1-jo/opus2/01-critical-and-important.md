# Critical And Important

## 1. Background `stress-ng` failures are ignored

Severity: Important, must-fix before this is an acceptance gate

Path/line: `ci/m7-throughput-soak.sh:128`

Description: The script starts `taskset ... stress-ng ... &`, records the PID, and never treats that process as part of the pass/fail condition. Under `set -e`, a failing background command does not fail the parent script unless it is explicitly waited on and checked. The current cleanup path waits with `|| true`, so an invalid housekeeping core list, `taskset` failure, early `stress-ng` error, or `stress_timeout` expiry during an overlong final batch can all leave the cargo loop running without the intended housekeeping load. That is a direct false-green risk for the stated "throughput soak under housekeeping load" acceptance.

Suggested fix snippet:

```bash
taskset -c "$housekeeping_cores" \
  stress-ng --cpu "$housekeeping_count" --io 1 --vm 1 --vm-bytes 25% \
    --timeout "${stress_timeout}s" --metrics-brief &
stress_pid=$!

stress_exited() {
  local status=0
  wait "$stress_pid" || status=$?
  echo "::error::stress-ng exited before the soak completed (status=$status)" >&2
  exit 1
}

ensure_stress_alive() {
  if ! kill -0 "$stress_pid" 2>/dev/null; then
    stress_exited
  fi
}

sleep 1
ensure_stress_alive

while :; do
  ensure_stress_alive
  now_ns="$(date +%s%N)"
  if [[ "$now_ns" -ge "$deadline_ns" && "$jobs_done" -gt 0 ]]; then
    break
  fi

  DH_M7_ACCEPT_JOBS="$batch_jobs" \
  DH_M7_ACCEPT_SLOT_CORES="$slot_cores" \
  DH_M7_ACCEPT_ALLOW_SKIP=0 \
    cargo test -p dh-worker --test m7_fork_verify --release -- --ignored --nocapture

  ensure_stress_alive
  jobs_done=$(( jobs_done + batch_jobs ))
done
```

If the timeout is kept, it should be long enough for the whole measured run, including the final batch overrun. Another robust option is to omit the `stress-ng --timeout` and let the existing cleanup trap own its lifecycle.

## 2. Acceptance-invalid core overlap is not rejected

Severity: Important, should-fix

Path/line: `ci/m7-throughput-soak.sh:82`

Description: The wrapper counts the slot and housekeeping core lists, but it does not preserve the expanded sets or reject overlap. The documented local smoke intentionally uses `DH_M7_SOAK_SLOT_CORES=0-1` and `DH_M7_SOAK_HOUSEKEEPING_CORES=0-1`, so overlap is useful for constrained development. For the operator acceptance, however, overlap means `stress-ng` is no longer pinned only to housekeeping cores and can compete directly with slot vCPU cores. That changes the measurement from "M7 under housekeeping load" into a different test and can lead to misleading pass/fail results.

Suggested fix snippet:

```bash
expand_core_list() {
  local spec="$1"
  SPEC="$spec" python3 - <<'PY'
import os

spec = os.environ["SPEC"]
seen = set()
for part in spec.split(","):
    part = part.strip()
    if "-" in part:
        lo_s, hi_s = part.split("-", 1)
        values = range(int(lo_s), int(hi_s) + 1)
    else:
        values = [int(part)]
    seen.update(values)
for value in sorted(seen):
    print(value)
PY
}

if [[ "${DH_M7_SOAK_ALLOW_CORE_OVERLAP:-0}" != "1" ]]; then
  overlap="$(
    comm -12 \
      <(expand_core_list "$slot_cores") \
      <(expand_core_list "$housekeeping_cores") \
      | paste -sd, -
  )"
  if [[ -n "$overlap" ]]; then
    echo "::error::slot and housekeeping cores overlap: $overlap" >&2
    echo "::error::set DH_M7_SOAK_ALLOW_CORE_OVERLAP=1 only for local smoke runs" >&2
    exit 2
  fi
fi
```

## 3. The wrapper can count zero-test success on an unsupported host

Severity: Important, should-fix

Path/line: `ci/m7-throughput-soak.sh:125`

Description: The underlying integration test is crate-gated to `target_arch = "x86_64"` (`crates/dh-worker/tests/m7_fork_verify.rs:22`). On the intended `kvm-intel` host, `DH_M7_ACCEPT_ALLOW_SKIP=0` correctly converts missing KVM or unavailable slot cores into a failure. On an unsupported architecture, though, `cargo test --test m7_fork_verify -- --ignored` may succeed with no runnable ignored test, after which the wrapper increments `jobs_done` by `batch_jobs`. The docs say this is `kvm-intel`-only, but an acceptance wrapper should fail closed before counting synthetic work.

Suggested fix snippet:

```bash
if [[ "$(uname -m)" != "x86_64" ]]; then
  echo "::error::M7 throughput soak must run on the x86_64 kvm-intel host" >&2
  exit 2
fi

if ! cargo test -p dh-worker --test m7_fork_verify --release -- --ignored --list \
    | grep -q '^m7_accept_1000_seeded_forks_verify_replay_all:'; then
  echo "::error::M7 ignored acceptance test was not discovered" >&2
  exit 2
fi
```
