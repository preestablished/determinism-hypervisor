# Positive notes — iteration 87 (opus2)

## P1 — Decode-shape match is exact and zero-filled

`LogWriter::epoch_hash` (dhilog.rs:262–271) emits `epoch_index u64 (LE) ‖ chain_value [u8;32]` into a `[0u8; 40]` buffer, filling `[0..8]` and `[8..40]` — the entire buffer is written, no uninitialized/leaked padding (the no_std codec research's "zero-fill before writing payloads" rule). The layout is byte-for-byte the reader's `RecordBody::EpochHash { epoch_index: u64at(0), chain_value: p[8..40] }` and is covered by the reader's `==40` length gate. Encode/decode are symmetric and the live test round-trips `links == decoded records`.

## P2 — Producer-site decision (the hard part of y62) resolved cleanly

y62 explicitly called out the borrow tension: `run_segment` owns the epoch links but not the log; the rail owns the log but not the links. The chosen design — `run_segment_with_epochs` appends `(epoch_index, icount, chain_value)` into a caller-owned `Vec`, the rail lands them via `log_epoch_hashes` after the quantum returns — avoids any callback-into-borrowed-log gymnastics and keeps the run loop free of log knowledge. The doc comment at runctl.rs:177–184 states the contract precisely, including *why* (on_exit holds the log) and the non-obvious carve-out that final-pause links at NON-epoch boundaries travel in END.end_state_hash, not as epoch hashes.

## P3 — Zero behaviour change for existing callers

`run_segment` becomes a thin delegate to `run_segment_with_epochs(..., &mut Vec::new())`. All 11 existing runctl lib tests (goal-poll, budget, halt, timer, two-vector, pause roll-forward, unwired-modes) pass unchanged, confirming the refactor is non-perturbing.

## P4 — Seal flag ordering is correct and the END carve-out is preserved

`wrote_epoch_hash` is latched in `epoch_hash()` (before seal) and OR'd into the header flags at seal (dhilog.rs:343–346) — independent of the `has_aux` snapshot/restore dance (dhilog.rs:333/335) that deliberately excludes the AUX-flagged END record from setting `FLAG_HAS_AUX`. EPOCH_HASH records are real AUX records, so they legitimately set `FLAG_HAS_AUX` too. The reader's cross-check (`has_epoch_hashes() == saw_epoch_hash`, reader.rs:509) ties the flag to actual record presence, and the live test asserts both `has_epoch_hashes()` and the three decoded records.

## P5 — Live test is a genuine end-to-end proof with a tight grid

`epoch_hashes_flow_from_quantum_to_sealed_log` uses `epoch_len = 30_000` against a 100k budget so a single quantum crosses three epochs (30k/60k/90k), asserts the exact `(index, icount)` pairs `[(1,30k),(2,60k),(3,90k)]`, asserts no all-zero hashes, then seals and re-parses to confirm the records and the header flag survive the round trip. The `out.reason == BudgetReached` at 100k (a non-epoch point) correctly demonstrates the design: the final link at 100k is *not* sinked as an epoch hash, and the test reads exactly 3 EPOCH_HASH records — it does not conflate END.end_state_hash with the epoch set. The budget-at-100k / grid-at-90k split is well chosen.
