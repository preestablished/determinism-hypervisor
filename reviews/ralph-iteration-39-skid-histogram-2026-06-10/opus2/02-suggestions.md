# Suggestions (minor, non-blocking)

### S-1 — Keep `SkidReport.gate` typed instead of stringifying at the boundary

`tools/dh-cli/src/skid.rs:21,76-78`:

```rust
pub gate: Result<(), String>,
...
let gate = histogram.assert_margin(skid_margin).map_err(|v| v.to_string());
```

The library does the right thing (`assert_margin` returns `Result<(),
MarginViolation>` with the typed `max_skid`/`skid_margin` fields). The CLI report
then **throws the type away** with `.to_string()`. Any future programmatic
consumer of `SkidReport` (a CI harness deciding whether to re-baseline, a metrics
exporter) has to re-parse the alert string to recover `max_skid`.

Suggestion: `pub gate: Result<(), MarginViolation>;` and let `main.rs` call
`Display` on it where it already prints `eprintln!("{e}")`. `MarginViolation`
already derives `Clone/Debug/PartialEq` and impls `Display`, so this is a
type-narrowing change with no ergonomic cost. The prompt flagged this as an API
smell — agreed; minor, but it's strictly better and free.

### S-2 — Empty histogram produces an absurd operator alert (`max skid 18446744073709551615`)

`crates/dh-verify/src/skid.rs:67-70` returns `max_skid: u64::MAX` for the empty
case so the gate fails (correct: "no data is not a pass"). But the resulting R1
alert reads:

```
R1 ALERT: measured max skid 18446744073709551615 >= skid_margin/2 (8192 / 2 = 4096)
```

I hit this with `dh-cli skid --samples 0`. The gate behaves correctly (exit 1),
but the message claims a 1.8e19-instruction measurement, which is nonsense an
operator might chase. Consider a distinct `MarginViolation` shape (e.g. an
`Option<u64> observed` or a `NoData` variant / separate error) so the empty case
reads "no samples collected — gate cannot pass" rather than a fake giant number.
Pure cosmetics; the safety behavior is right.

### S-3 — `--samples` silently swallows malformed values, falling back to 200

`tools/dh-cli/src/main.rs:252-256`: `--samples notanumber` and bare `--samples`
(no value) both parse to `None` → `.unwrap_or(200)`, silently measuring the
default. A typo like `--samples 100O` (letter O) would silently run 200 samples
and the operator would never know the flag was ignored. Consider distinguishing
"flag absent → default 200" from "flag present but unparseable → usage error".
Low priority — this is a debug CLI, and the gate value (margin/2) is independent
of sample count — but it's a quiet failure mode.

### S-4 — Consider documenting the tri-modal shape in the artifact or note

The distribution is not a smooth bell — it's exactly three values (27/30/31) with
a near-uniform 334/333/333 split tracking the period/loop-body phase alignment.
That is a genuinely useful determinism property (the skid is quantized to a few
fixed phase offsets, not noisy). A one-line note where the histogram is
documented would help a future reader understand why re-running gives identical
buckets — i.e. that this is a feature (phase-locked), not under-sampling.
