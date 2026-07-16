# Suggestions (non-blocking)

### S1 — A skipped acceptance test is indistinguishable from a passing one for the nightly gate (1pa)

**File:** `crates/dh-worker/tests/perf_gates.rs:78-87`

Both early returns (`!kvm_available()` and `cfg!(debug_assertions)`) print to stderr and then
`return` — the test reports **PASS**. For the operator running it by hand with `--nocapture`,
the loud `eprintln!` is visible and fine. But the nightly consumer (1pa) parses test
*results*, where a silent skip and a real pass are the same green check. If the nightly job
is ever misconfigured to build debug, or `/dev/kvm` is absent on the runner, the gate goes
green having measured nothing.

This is a *consumer-contract* concern, so the fix belongs partly in 1pa, but the test can
help: have the nightly invocation assert the gate actually ran. Two concrete options:

- **Env-gated hard failure.** When an env var the nightly job sets (e.g.
  `DH_PERF_GATE_REQUIRED=1`) is present, turn the two skips into `panic!`s instead of
  `return`s. Operators running ad-hoc keep the soft skip; the nightly job cannot
  accidentally pass without measuring.
- **Emit a sentinel line** the nightly job greps for (e.g. `eprintln!("PERF_GATE_RAN")`
  only on the real path), and have 1pa fail if the sentinel is absent. Lighter, no test
  signature change.

Either keeps the ad-hoc ergonomics while closing the skip-equals-pass hole. Worth a note on
1pa regardless of which side implements it.

### S2 — `p50` takes the upper median for `SAMPLES = 30` (even count); fine, but make it deliberate

**File:** `crates/dh-worker/tests/perf_gates.rs:97-100`

`samples[samples.len() / 2]` returns index 15 of a sorted 30-element slice — the upper of the
two central order statistics, i.e. the 53rd percentile, not a 14/15 average. This is the
*conservative* choice (slightly higher value, harder to pass), which is the right bias for a
gate. No change needed, but a one-line comment ("upper median: conservative for a gate") would
stop a future reader from "fixing" it into an averaging median that loosens the gate. The
bench side has no equivalent issue (criterion computes its own statistics).

### S3 — First-sample cold-cache effect on p50 is negligible at n=30 but unmentioned

**File:** `crates/dh-worker/tests/perf_gates.rs:108-120` (and the snapshot/restore loops)

There is no warm-up iteration before the measured loop; sample 0 pays cold-cache /
first-fault costs. The median of 30 is robust to one or two cold samples (it would take 15
inflated samples to move p50), so this is correct as-is. The bench side *does* warm up
(`warm_up_time`). For symmetry and to forestall the question, consider either one discarded
warm-up iteration per gate or a comment noting the median's robustness makes warm-up
unnecessary here. Non-blocking — the math already protects you.

### S4 — Store tempdir grows ~32 MiB per snapshot/restore sample; bounded but worth a guard rail

**File:** `crates/dh-worker/tests/perf_gates.rs` (snapshot loop, 30 samples) and
`benches/perf_gates.rs` (sample_size 10)

Each incremental sample ships 32 MiB into the tempdir-backed store; with content-addressed
dedup, identical-content samples collapse, but the per-sample `dirty.insert` writes
`page as u8 ^ 0x5A` — deterministic across samples, so they *should* dedup to one object.
The restore samples read, not write, so they do not grow the store. Net growth is therefore
bounded (one root + one delta object), which is fine. The bench comment already flags "each
sample ships 32 MiB" and keeps `sample_size(10)`. No action needed; flagging only so the
reviewer of a future change that varies per-sample content (defeating dedup) knows the store
would then grow linearly and the tempdir could blow up at higher sample counts.

### S5 — Bench `sample_size(10)` with ~110 ms/iter vs a 2 s measurement window

**File:** `crates/dh-worker/benches/perf_gates.rs:104-115`

`sample_size(10)` is criterion's legal minimum. At ~110 ms/iter for the incremental snapshot,
10 samples need ~1.1 s of pure measurement; the 2 s `measurement_time` accommodates it.
Criterion treats `measurement_time` as a *target*, not a cap — if the requested sample count
needs more time, it warns ("unable to complete N samples in the time limit") and extends
rather than failing, which is acceptable for a trend instrument. The 5 s window for restore
(~317 ms/iter × 10 = ~3.2 s) is comfortable. No change required; this is a correct, if tight,
configuration. If the criterion warnings prove noisy in the nightly log, bump
`measurement_time` to 3 s for the snapshot group.
