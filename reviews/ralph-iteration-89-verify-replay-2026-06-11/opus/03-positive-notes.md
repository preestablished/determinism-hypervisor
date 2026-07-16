# Positive notes

### The ARCH §1 dependency split is respected exactly

The model lives in `dh-verify` (`verify.rs`) and the executor lives in `dh-worker`
(`verify_replay.rs`), with `dh-worker` importing `dh_verify::verify::{...}` and never
the reverse. This honors the CI-enforced "nothing depends on dh-worker" rule and the
module docs spell out *why* the split exists. The reporting model is genuinely
host-runnable (its unit test in verify.rs:73-104 needs no KVM), which is the right
place for cw2 and the future RPC to share a vocabulary.

### The executor is honestly thin — it does not re-verify

`verify_replay` correctly delegates *all* verification to `replay_segment` and only
translates the outcome. It does not re-hash, re-walk, or second-guess the engine.
The EpochOk stream is reconstructed from the log records "every one of which the
replay proved," and a `Done`/`Divergence` is appended. This is the right division of
labor: the engine already fails fast on the first bad epoch link and pins the
verified count, so the wrapper inheriting that is correct (modulo I3's debug-only
pin). No duplicated verification logic to drift.

### The Ok-verdict / Err-infrastructure boundary is a genuinely good design call

Separating "this recording diverged" (a *verdict*, `Ok(report)`) from "I could not
run the verification" (store/parse/KVM/unwired, `Err`) is exactly the distinction a
1000x acceptance harness needs: a divergence is a data point to record, an
infrastructure failure is a test-harness bug to fix. The module doc states this
crisply ("infrastructure failures ... stay errors — they are not verdicts about the
recording") and the live test *asserts the distinction* — it `.expect("verification
ran")` on the poisoned case and then checks for a `Divergence` *inside* the Ok report,
proving a divergence is not smuggled out as an error. That assertion is the most
valuable line in the test.

### `EpochOk` and `VerifyDone` mirror the proto field-for-field

Despite the `Divergence` fidelity gap (I2), the other two variants are faithful:
`EpochOk { epoch_index, icount }` matches proto `EpochOk { epoch_index, icount }`
(hypervisor.proto:335), and `Done { total_icount, end_state_hash }` matches proto
`VerifyDone { total_icount, end_state_hash }` (hypervisor.proto:336-338). The
deliberate *absence* of the M8 bisection fields (icount_lo/hi, rip pair) from the
model is the correct phase-1 scope, and the doc says so.

### The live test exercises the real boundary case the bead cares about

The good recording asserts `epochs_ok() == 10` and `total == 3 * QUANTUM` (300k) with
a non-zero end hash; the poisoned recording asserts `first_bad_epoch == 1` — i.e. the
30_000/30_000 = 1 boundary the prompt flags as the one to get right. For the path it
covers, the arithmetic is correct and the test pins it. (The gap is only that it does
not cover the *other* five divergence kinds — see S4.)

### Style consistency with gate.rs

`VerifyReport` follows the `GateReport` template: a `Vec` of events plus thin query
methods (`verified()`/`done()`/`divergence()`/`epochs_ok()` mirror
`passed()`/`first_divergence`). A reader who knows `gate.rs` reads `verify.rs` for
free. Adopting `gate.rs`'s `artifact()` affordance (S1) would complete the parallel.

### Clean, accurate comments about prior review history

The executor and engine carry references to specific prior-iteration review fixes
(iteration-88 I1/I2, the structured side-channel, the halt-coincidence contract).
This is good provenance — it shows the divergence reporting was already hardened and
explains *why* the engine surfaces structured divergence data the way it does.
