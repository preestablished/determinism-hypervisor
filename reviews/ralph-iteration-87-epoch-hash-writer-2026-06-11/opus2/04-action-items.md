# Action items — iteration 87 (opus2)

### Critical

None.

### Important

- [ ] **I1 — Decide and fix the FinalOnly pause roll-forward epoch-hash emission.** In `crates/dh-vmm/src/runctl.rs:374–397`, the pause branch pushes `(b.icount / epoch, b.icount, chain)` into `epoch_sink` unconditionally, even when `seg.config.hash_epochs == HashEpochs::FinalOnly` (where the grid sink at :337 is correctly suppressed via `epoch_len: None`). Once y62 is wired into the live recording loop, this makes a FinalOnly log carry a `KIND_EPOCH_HASH` record and set `FLAG_EPOCH_HASHES`, contradicting the FinalOnly contract and risking a false record-vs-replay EPOCH_HASH set mismatch (a5e's "every EPOCH_HASH equal"). **Fix (preferred):** gate the pause-branch `epoch_sink.push` on epochs-on (thread the agenda's `Option<NonZeroU64>` / a `bool` into the run); keep `push_final_link` unconditional so pause stays reproducible in all modes. **Alternative:** if surfacing a pause epoch hash under FinalOnly is intended, write it into API.md §3.3 + 39w's verify contract and add a FinalOnly+pause record/replay live test. Resolve before y62 is wired into recording or before 39w starts on FinalOnly. Self-contained repro context: the chain link is fine; only the sink push is the issue.

### Suggestions

- [ ] **S1 — Pin the absolute-grid invariant in a comment** at `runctl.rs:336–338` (the epoch sink push), naming a5e and forbidding a start-relative refactor. The grid is anchored at absolute segment-start-0 multiples (`agenda.rs:157`, property test `agenda.rs:465`); `point.icount / epoch_len` is exact only because of it. See 02/S1 for the exact comment text.
- [ ] **S2 — Add a non-hardware unit test** for the `icount → (epoch_index, icount)` mapping and the pause `div_ceil` index, since the only current coverage (`epoch_hashes_flow_from_quantum_to_sealed_log`) is kvm-gated. Factor the mapping into a pure fn and table-test it (30k→1, 60k→2, u64::MAX boundary). See 02/S2.
- [ ] **S3 — (awareness only)** `run_segment` delegates with a throwaway `Vec` that *will* grow on EpochsOn runs and then be dropped each quantum. A no-op sink (callback or `Option<&mut Vec>`) would avoid it. Do not action speculatively. See 02/S3.
- [ ] **S4 — 39w readiness (informational, for whoever picks up 39w):** implement verify-as-you-go epoch comparison against `RecordBody::EpochHash`; reuse the record-side `push_final_link` chain construction; honour the absolute-grid index (S1); restrict the first acceptance to EpochsOn + no-pause runs (a5e's pad-echo) until I1 is decided. Full paragraph in 02/S4. No reader changes needed — `RecordBody::EpochHash` and `has_epoch_hashes()` are ready.
