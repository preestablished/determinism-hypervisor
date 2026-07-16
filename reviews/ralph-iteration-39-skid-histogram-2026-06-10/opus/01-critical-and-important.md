# Critical & Important Findings

## None.

No Critical and no Important issues were found in this iteration. The verification walk below records why each scrutiny point clears, so a future reader does not have to re-derive the safety argument.

### Measurement validity — CLEARS

The counter is enabled once and free-runs for the whole session; it is never reset per sample. Each sample reads `before = counter.read()`, arms `period` via `PERF_EVENT_IOC_PERIOD` (immediate-effect, iter-33-documented), and the overflow therefore fires when the free-running count reaches `before + period`. `skid = after − (before + period)`. The guest does not run between `read()` and `arm_period` — the vCPU only advances inside `guard.run()`, and both calls happen on the host thread between run-loop iterations (`crates/dh-detclock/src/counter.rs:140`, `tools/dh-cli/src/skid.rs:47-49`). `exclude_host` keeps host instructions out of the count. The arm tolerates serial `IoOut` exits mid-sample (ignored at `tools/dh-cli/src/skid.rs:56`) for robustness; with cmdline `4e9` the landing loop never reaches its completion OUT, so in practice the only exit is the kick EINTR. Verified: `after − (before+period)` is the genuine instructions-retired overshoot.

### Stale-kick hazard between samples — CLEARS (and proven empirically)

Overflow fires once per period crossing. After EINTR the guest is stopped, so no further counting occurs before the period is parked to `NEVER_FIRES_PERIOD` (`tools/dh-cli/src/skid.rs:69-71`). A second queued overflow within one period is impossible: periods are ≥ 10k and the kick stops the guest within ~30 instructions (measured max skid 31). A stale queued kick leaking into sample k+1 would EINTR early, drive `after < armed_point`, and trip the loud "stale signal?" error at `tools/dh-cli/src/skid.rs:62-65`. **Across 5 live CLI runs (100 samples each) that error never fired**, and `sum` was bit-identical (2931) every run — strong evidence the harness is not flaky.

### pid-as-tid fix — CLEARS, complete

`dh-vmm::run::current_tid()` (`crates/dh-vmm/src/run.rs:129`) wraps `gettid()` behind the targeted `#[allow(unsafe_code)]` precedent already used elsewhere in the module. Both dh-cli call sites are fixed (`tools/dh-cli/src/run.rs:35`, `tools/dh-cli/src/skid.rs:35`); the old `std::process::id() as i32` helper is deleted. A workspace grep for `process::id`/`getpid` finds only legitimate uses: `crates/dh-vmm/src/run.rs:253` (tgkill needs the real pid plus the tid — correct), and `crates/dh-vmm/src/blkfile.rs:80` (pid in a temp filename — correct). No tid-routing remnant survives.

### assert_margin strictness — CLEARS

`max < skid_margin / 2` is strict; the unit test asserts `4096 == 8192/2` FAILS (`crates/dh-verify/src/skid.rs:128-129`). Empty histogram returns `Err` with sentinel `max_skid = u64::MAX` (`:67-70`), and the test confirms a default `SkidHistogram` fails the gate (`:130`). Matches the bead's "no data is not a pass" rule.

### Prometheus format — CLEARS

Buckets are cumulative (`cumulative += count` before each `le` line, `crates/dh-verify/src/skid.rs:96-100`); `+Inf` equals `samples` (`:101`); `# TYPE … histogram` header present (`:95`); `_sum`/`_count` emitted (`:102-103`). The unit test pins cumulative values 2→3→3. `sum` is a `u128` printed as an integer — the Prometheus exposition format accepts integer literals for `_sum`, so no float formatting is required. Live output validated against a real run.
