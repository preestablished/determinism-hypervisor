# Suggestions

## Use non-overflowing rate math for the final comparison

Path: `ci/m7-throughput-soak.sh:169`

The default values are safely below signed 64-bit overflow, but Bash arithmetic makes future larger soaks fragile because `jobs_done * 1000 * 1000000000` grows quickly. Moving the calculation and comparison into Python also allows a clearer decimal result in logs.

Suggested snippet:

```bash
read -r actual_millijobs_per_second required_jobs < <(
  JOBS_DONE="$jobs_done" ELAPSED_NS="$elapsed_ns" TARGET_MJPS="$target_millijobs_per_second" \
    python3 - <<'PY'
import os
jobs = int(os.environ["JOBS_DONE"])
elapsed_ns = int(os.environ["ELAPSED_NS"])
target = int(os.environ["TARGET_MJPS"])
actual = jobs * 1000 * 1_000_000_000 // elapsed_ns
required = (target * elapsed_ns + 999_999_999_999) // 1_000_000_000_000
print(actual, required)
PY
)
```

## Either use `required_jobs_at_elapsed` in the failure or remove it

Path: `ci/m7-throughput-soak.sh:170`

The script computes `required_jobs` and logs it, but the actual gate compares only the rate. That is mathematically equivalent for the current integer calculation, but the unused-looking value invites confusion during an operator-run acceptance. It would be clearer to either remove it or include it in the failure line.

Suggested snippet:

```bash
if [[ "$actual_millijobs_per_second" -lt "$target_millijobs_per_second" ]]; then
  echo "::error::M7 throughput below target: jobs_done=$jobs_done required_jobs_at_elapsed=$required_jobs actual=${actual_millijobs_per_second}mjob/s target=${target_millijobs_per_second}mjob/s" >&2
  exit 1
fi
```

## Document that one configured slot is held by the reusable root parent

Path: `ci/m7-throughput-soak.sh:25`

The soak target is based on `count(DH_M7_SOAK_SLOT_CORES)`, while the M7 harness reserves one slot as the root parent and uses the remaining slots for concurrent children. That may be intentional for the milestone's canonical `N_slots` target, but it is worth stating because otherwise operators may expect child concurrency to equal the target slot count.

Suggested snippet:

```bash
echo "  child_concurrency=$(( slot_count - 1 )) (one slot is reserved for the reusable root parent)"
```

And in the usage text:

```text
The target uses count(DH_M7_SOAK_SLOT_CORES); the M7 harness reserves one of
those slots for the reusable root parent and runs children on the remainder.
```

## Log tool versions in the acceptance transcript

Path: `ci/m7-throughput-soak.sh:110`

The runner doc pins `stress-ng`, and this is an operator-run acceptance. Printing `stress-ng --version`, `taskset --version`, and `cargo --version` near the config block would make later acceptance transcripts easier to audit.

Suggested snippet:

```bash
echo "  stress_ng_version=$(stress-ng --version | head -n1)"
echo "  taskset_version=$(taskset --version | head -n1)"
echo "  cargo_version=$(cargo --version)"
```
