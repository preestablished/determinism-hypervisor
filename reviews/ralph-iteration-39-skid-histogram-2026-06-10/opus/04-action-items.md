# Action Items

### Critical

None.

### Important

None.

### Suggestions

All optional; safe to merge without addressing any of them.

- **[S1] Warn on malformed `--samples`.** In `tools/dh-cli/src/main.rs:35-40`, emit an `eprintln!` when `--samples` is present but its value is missing or unparseable, instead of silently falling back to 200. Verified live that `dh-cli skid 50 --samples`, `dh-cli skid --samples`, and `dh-cli skid --samples abc` all silently run 200 samples.

- **[S2] Document sparse buckets.** Add a one-line note to `SkidHistogram` (`crates/dh-verify/src/skid.rs`) that only observed skid values get buckets (live runs populate just `{27, 30, 31}`), so artifact readers don't think intermediate buckets were dropped.

- **[S3] Document or check the free-running sum.** `armed_point = before + period` at `tools/dh-cli/src/skid.rs:48` is unchecked u64 addition on a free-running counter — safe at current scale; add a `checked_add` or a bounding comment if sample counts/session length ever grow large.

- **[S4] Note the CLI-vs-test sample-count divergence.** Add a brief comment in `tools/dh-cli/tests/skid_gate.rs:24` explaining the 50-sample test count (suite speed) vs the 200 CLI default, and that the gate property holds at any N.
