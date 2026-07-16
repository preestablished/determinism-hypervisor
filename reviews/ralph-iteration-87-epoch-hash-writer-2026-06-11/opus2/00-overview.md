# Iteration 87 — EPOCH_HASH writer + epoch sink (y62) — 2nd-reviewer overview

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-11
- **Branch:** `ralph/iteration-87-epoch-hash-writer`
- **Bead:** determinism-hypervisor-y62 (a5e prerequisite)
- **Scope:** 3 files, +171 / -1
  - `crates/dh-inputlog/src/dhilog.rs` — `LogWriter::epoch_hash` (AUX, 40B), `wrote_epoch_hash` flag → `FLAG_EPOCH_HASHES` at seal
  - `crates/dh-vmm/src/runctl.rs` — `run_segment_with_epochs` (epoch-link sink), `run_segment` delegates
  - `crates/dh-vmm/src/recording.rs` — `DeviceRail::log_epoch_hashes` + kvm-gated live landing-loop test

## Summary

The writer side of the §8.5 epoch-hash mechanism. `epoch_hash()` emits a 40-byte AUX record (`epoch_index u64 ‖ chain_value [u8;32]`) that exactly matches the reader's frozen decode shape, fully zero-fills its payload buffer, and latches `wrote_epoch_hash` so seal sets `FLAG_EPOCH_HASHES`. `run_segment_with_epochs` collects `(epoch_index, icount, chain_value)` at every epoch-grid stop point and at the pause roll-forward; `run_segment` delegates with an empty throwaway sink (zero behaviour change for existing callers). `DeviceRail::log_epoch_hashes` lands the collected links as records after the quantum returns, resolving the borrow conflict (run owns the links, the rail owns the log) cleanly.

I worked the adversarial angles the prompt flagged. The headline worry — a hostile 39-byte EPOCH_HASH panicking the reader's `p[8..40].try_into().unwrap()` — **does not exist**: `validate_kind` (reader.rs:539) gates `KIND_EPOCH_HASH | KIND_END => payload.len() == 40` and runs inside `parse` before any `body()` is reachable. The reader is total here.

The one substantive finding is a **record/replay semantic asymmetry in the pause roll-forward under `HashEpochs::FinalOnly`**: that push (runctl.rs:375) is *not* gated by `hash_epochs`, so a pause in a FinalOnly run still appends an "epoch hash" to the sink — which, once the y62 writer is wired into the recording path, would emit a `KIND_EPOCH_HASH` record and set `FLAG_EPOCH_HASHES` on a log that was explicitly configured to carry none. See 01.

The epoch-index arithmetic is deterministic (the agenda anchors the grid at **absolute** segment-start-0 multiples, not start-relative — so `point.icount / epoch` is exact integer division on a guaranteed multiple, identical across differently-quantized record/replay runs). This is the key property a5e/39w depend on and it holds; I recommend a doc note pinning it (02).

## Verdict

**Approve with one Important finding to resolve before y62 is wired into recording.** The writer, flag, decode-shape match, seal ordering, and zero-fill are all correct and well-tested. The FinalOnly pause asymmetry should be fixed or explicitly documented as out-of-scope before a5e/czq runs touch FinalOnly + pause.

## Stats

| Severity   | Count |
|------------|-------|
| Critical   | 0     |
| Important  | 1     |
| Suggestions| 4     |
| Positive   | 5     |

**Build:** `cargo build -p dh-inputlog -p dh-vmm` clean.
**Tests:** dh-inputlog 29 + golden fixtures pass; dh-vmm runctl lib tests 11 pass (live test kvm-gated, not exercised in this sandbox).
