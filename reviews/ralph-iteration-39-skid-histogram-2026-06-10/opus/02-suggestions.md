# Suggestions (non-blocking)

These are polish items. None block merge.

## S1 — `--samples` parsing silently swallows malformed args

`tools/dh-cli/src/main.rs:35-40` parses only the rigid form `skid --samples N`. Anything else falls back to the default 200 **silently**:

- `dh-cli skid 50 --samples` → runs 200 (the bare `50` is ignored)
- `dh-cli skid --samples` (no N) → runs 200
- `dh-cli skid --samples abc` → runs 200

Verified live: all three above ran 200 samples without complaint. For a debug CLI this is acceptable (the prompt explicitly judged the rigid order OK), but a user who fat-fingers the flag gets a longer run than intended with no signal. A one-line `eprintln!` warning when `args.get(1) == Some("--samples")` but the value fails to parse would remove the foot-gun cheaply. Low priority.

## S2 — Histogram has only 3 populated buckets; consider noting the discreteness

Live runs produce exactly `{27, 30, 31}` — no 28 or 29. This is a real and interesting property (PMI delivery latency is quantized on this silicon), not a bug, but a reader of the artifact might wonder whether intermediate buckets were dropped. A one-line doc comment on `SkidHistogram` noting that buckets are sparse (only observed skids appear) would pre-empt the question. Cosmetic.

## S3 — `before + period` free-running sum is unchecked

`tools/dh-cli/src/skid.rs:48` computes `armed_point = before + period` on a free-running u64 counter. For any realistic session (≤ 200 samples × 100k + a 4e9 guest budget) this is nowhere near `u64::MAX`, so it is safe today. If `measure` ever grows to very large sample counts or long-lived counters, a `checked_add` (or a comment pinning the bound) would document the assumption. Very low priority.

## S4 — Default sample count differs between CLI (200) and gate test (50)

The CLI defaults to 200 samples (`main.rs:40`) while `tests/skid_gate.rs:24` measures 50. Both are deliberate (the test trades coverage for suite speed), but a brief comment in the test noting "fewer than the CLI default for suite speed; the gate property holds at any N" would make the divergence self-explanatory. Cosmetic.
