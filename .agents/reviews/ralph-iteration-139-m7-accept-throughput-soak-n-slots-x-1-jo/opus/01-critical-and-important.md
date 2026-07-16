# Critical And Important

## Critical: the soak can pass even when `stress-ng` never starts or dies early

File: `ci/m7-throughput-soak.sh:128`

The script backgrounds `taskset -c "$housekeeping_cores" stress-ng ...` and immediately starts measuring M7 jobs. `set -e` does not propagate failures from a backgrounded process, and `cleanup` ignores the `wait` status. If `taskset` rejects the core list, `stress-ng` is missing runtime support, `stress-ng` exits after startup, or it dies before the deadline, the script can still complete and report throughput as though the required housekeeping load was present. This is the main false-green risk for the M7 acceptance.

Suggested fix snippet:

```bash
taskset -c "$housekeeping_cores" \
  stress-ng --cpu "$housekeeping_count" --io 1 --vm 1 --vm-bytes 25% \
    --timeout "${stress_timeout}s" --metrics-brief &
stress_pid=$!

stress_exit_code() {
  local rc
  set +e
  wait "$stress_pid"
  rc=$?
  set -e
  printf '%s\n' "$rc"
}

require_stress_alive() {
  local phase="$1"
  if ! kill -0 "$stress_pid" 2>/dev/null; then
    local rc
    rc="$(stress_exit_code)"
    echo "::error::stress-ng exited during $phase before the soak window completed (exit $rc)" >&2
    exit 1
  fi
}

sleep 1
require_stress_alive "startup"

start_ns="$(date +%s%N)"
deadline_ns=$(( start_ns + duration_seconds * 1000000000 ))

while :; do
  require_stress_alive "measurement"
  # run the batch...
  if [[ "$(date +%s%N)" -lt "$deadline_ns" ]]; then
    require_stress_alive "measurement"
  fi
done
```

## Important: the housekeeping and slot core lists are only counted, not validated as a topology

File: `ci/m7-throughput-soak.sh:82`

`count_core_list` validates syntax and counts cores, but the script does not reject overlap between `DH_M7_SOAK_SLOT_CORES` and `DH_M7_SOAK_HOUSEKEEPING_CORES`, and it does not preflight that the housekeeping mask is valid in the current cpuset. The Rust M7 harness catches unavailable slot cores later, but housekeeping-core mistakes are only discovered by the background `taskset` path above, which is currently ignored. An accidental overlap also invalidates the documented behavior of keeping load on housekeeping cores rather than on guest slot cores.

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
        lo, hi = map(int, part.split("-", 1))
        values = range(lo, hi + 1)
    else:
        values = [int(part)]
    for value in values:
        if value in seen:
            raise SystemExit("duplicate core")
        seen.add(value)
for value in sorted(seen):
    print(value)
PY
}

overlap="$(
  comm -12 \
    <(expand_core_list "$slot_cores" | sort -n) \
    <(expand_core_list "$housekeeping_cores" | sort -n)
)"
if [[ -n "$overlap" ]]; then
  echo "::error::slot and housekeeping core lists must be disjoint; overlap: $overlap" >&2
  exit 2
fi

taskset -c "$housekeeping_cores" true || {
  echo "::error::DH_M7_SOAK_HOUSEKEEPING_CORES is not valid in this process cpuset: $housekeeping_cores" >&2
  exit 2
}
```

## Important: `INT` and `TERM` cleanup does not force the script to stop

File: `ci/m7-throughput-soak.sh:139`

`trap cleanup EXIT INT TERM` uses the same handler for process exit and external interruption. For `INT`/`TERM`, the handler kills `stress-ng` and then returns. Depending on where the signal lands and how the foreground `cargo test` exits, Bash can continue with the loop after the load generator has been killed, producing misleading follow-on output or measuring without the intended stress.

Suggested fix snippet:

```bash
trap cleanup EXIT
trap 'cleanup; exit 130' INT
trap 'cleanup; exit 143' TERM
```

## Important: the test-partitioning introduction now misdescribes the new soak row

File: `docs/ops/test-partitioning.md:3`

The document still says "Everything is part of `cargo test --workspace`; the live legs self-skip when `/dev/kvm` is not usable." The new M7 throughput soak row at `docs/ops/test-partitioning.md:59` is a standalone shell script, depends on `stress-ng`, invokes an ignored test explicitly, and sets `DH_M7_ACCEPT_ALLOW_SKIP=0`. Operators reading only the table preface could incorrectly assume the soak is covered by the workspace command or that it self-skips safely off the lab host.

Suggested fix snippet:

```markdown
Two hardware classes run this repo's gates. Most test entries are part of
`cargo test --workspace`; the live Rust legs self-skip when `/dev/kvm` is
not usable (open rw probe), so that command is correct everywhere. Rows
marked operator-run, including the M7 throughput soak script, are explicit
commands and may require lab-host-only tools such as `stress-ng`.
```
