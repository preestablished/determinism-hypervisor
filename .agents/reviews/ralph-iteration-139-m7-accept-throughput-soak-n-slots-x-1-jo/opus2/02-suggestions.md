# Suggestions

## 1. Use a monotonic clock for rate and deadline math

Path/line: `ci/m7-throughput-soak.sh:141`

Rationale: `date +%s%N` is wall-clock time. An NTP step or manual clock adjustment during a 30-minute soak can skew both the deadline and the computed rate. The risk is small on the lab box, but monotonic time matches the measurement being made.

Snippet:

```bash
now_ns() {
  python3 - <<'PY'
import time
print(time.monotonic_ns())
PY
}

start_ns="$(now_ns)"
deadline_ns=$(( start_ns + duration_seconds * 1000000000 ))
...
elapsed_ns=$(( $(now_ns) - start_ns ))
```

## 2. Make the result output easier to audit

Path/line: `ci/m7-throughput-soak.sh:172`

Rationale: The script prints millijobs/second and a floored `required_jobs_at_elapsed`, but operators usually reason in jobs/second and total jobs. A decimal jobs/sec line would make copied logs easier to interpret without recalculating the units.

Snippet:

```bash
echo "  actual_jobs_per_second=$(( actual_millijobs_per_second / 1000 )).$(
  printf '%03d' "$(( actual_millijobs_per_second % 1000 ))"
)"
echo "  target_jobs_per_second=$(( target_millijobs_per_second / 1000 )).$(
  printf '%03d' "$(( target_millijobs_per_second % 1000 ))"
)"
```

## 3. Document the exact acceptance defaults in the runbook table

Path/line: `docs/ops/test-partitioning.md:59`

Rationale: The docs correctly identify this as a 30-minute operator-run soak, but the table hides the load placement and target formula. Including the default core sets makes accidental local or wrong-host runs easier to spot in review logs.

Snippet:

```markdown
| M7 throughput soak under housekeeping load | `DH_M7_SOAK_SLOT_CORES=2-5 DH_M7_SOAK_HOUSEKEEPING_CORES=0-1 DH_M7_SOAK_SECONDS=1800 ci/m7-throughput-soak.sh` | 30 min plus final batch; target defaults to slot_count x 1 job/s |
```

## 4. Note that the wrapper can exceed the nominal duration

Path/line: `ci/m7-throughput-soak.sh:25`

Rationale: The loop starts a batch whenever the deadline has not yet passed and exits only after a completed batch. That is a good "at least 30 minutes" semantic, but it means a slow or large final batch can make the command run longer than `DH_M7_SOAK_SECONDS`.

Snippet:

```bash
echo "  duration_seconds=$duration_seconds batch_jobs=$batch_jobs"
echo "  note=run continues until the in-flight batch after the deadline completes"
```
