# Positive Notes

### P-1 — Dependency direction is correct and well-justified

`dh-verify` owns the event shapes (`VerifyProgress`, `VerifyReport`) as pure
types; `dh-worker` imports them for execution. The module doc-comment
(verify.rs:1-4) explicitly states the rationale (ARCH §1: nothing depends on
dh-worker, so the executor imports the model, never the reverse). This is the
right layering and it is documented at the point of decision. **Confirmed:
dh-verify's Cargo.toml gains zero new dependencies** (still only
`dh-snapshot`), so verify.rs is genuinely pure types and builds host-only /
aarch64-clean as required.

### P-2 — The infrastructure-vs-verdict boundary is crisp and correct

`verify_replay` returns `Ok(report-with-Done-or-Divergence)` for product-
property outcomes and propagates `Err` for store/log-parse/KVM/NotYetWired
failures (verify_replay.rs:85). This is exactly the right contract for cw2's
exit gate: a divergence is a verdict to be *counted*, an infrastructure failure
is a bug to be *fixed*. The doc-comment states it plainly (verify_replay.rs:25-27).

### P-3 — The EpochOk reconstruction is sound under adversarial tracing

The "reconstruct EpochOk from the log's own records" shortcut initially looks
like it could over-report (emit OKs the engine never verified). It does not —
the engine builds `expected_epochs` from the same filter, fails fast on the
first mismatch, and pins `verified == expected_epochs.len()` before returning
`Ok` (replay_engine.rs:336). Combined with the parser's icount-monotonicity +
END-last invariants, a hostile log cannot smuggle an unreached EpochHash past
the wrapper. The honesty argument in the doc-comment (verify_replay.rs:7-11)
holds. (See I-1 for the one polish: make the `debug_assert` intent explicit.)

### P-4 — The test exercises BOTH the verdict variants the API documents

The new live test covers the happy path (10 EpochOk + Done, end identity
non-zero) *and* the divergence verdict (RAM-poisoned → `Ok` report with a
`Divergence`, not an `Err`). The poison is chosen thoughtfully — the
doc-comment (test:96-100) explains *why* RAM divergence (not a chain-seed
poison) is the honest mismatch: a poisoned seed would travel through time and
stay self-consistent. Asserting the first epoch diverges (poison at 0x60_0000
before any quantum; first link at icount 30_000 hashes full RAM) correctly
proves the earliest-possible divergence point. Per the research file's "assert
the failure paths, not just the happy path" guidance, this is the right shape.

### P-5 — The divergence-cannot-fire-earlier reasoning checks out

The prompt asks whether the poison divergence could fire *earlier* than epoch
1. It cannot: the first EPOCH_HASH link is at icount 30_000 (= epoch_len), and
that is the first point the live chain folds in a full-RAM hash. There is no
verification point before icount 30_000, so epoch index 1 (`30_000/30_000`) is
necessarily the first observable divergence. The test's
`assert_eq!(*first_bad_epoch, 1)` is exactly right.
