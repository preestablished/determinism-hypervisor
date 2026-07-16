# Positive Notes

### P1 — RefCell panic/re-entrancy safety is genuinely clean (verified, not assumed)

The pre-flagged worry — `on_exit` borrows `rail.borrow_mut()` while the sink also
`borrow_mut()`s, could both fire nested in one `run_segment_with_epochs` frame —
does not hold. Traced in source: in `replay_engine.rs`, `on_exit` is
`rail.borrow_mut().service_exit(...)` (a single statement; the guard drops at the
expression end) and the sink is `rail.borrow_mut().log_epoch_hash(...)` (likewise
single-statement). `run_segment_with_epochs` calls them sequentially and
single-threaded — `on_exit` from inside `land_at`, the sink at the epoch link
point *between* exits — never nested, and each guard is released before control
returns to the caller. The `rail.borrow().irqs.is_empty()` read (line ~273) and
the `apply_pad_set`/`apply_net_rx` `borrow_mut()`s happen *outside* any
`run_segment_with_epochs` call, between records, with no other borrow live. There
is no path that holds a borrow across both. The `into_inner()` at reseal time
proves no guard outlives the run. RefCell is the right tool and it is used safely.

### P2 — ENTR v2 split is honored correctly (device regs in bus, PRNG swapped)

`restore_snapshot` restores the pv-entropy *device* registers into `rail.bus`
(the rail dispatches exits there), and the VMM-owned PRNG (`rail.entropy`) is
replaced wholesale from `restored.entropy` (the §3.1 zero-seed continue path) or
`DetEntropy::from_seed(header.entropy_seed)` for a nonzero seed. Traced the
recording side too: `take_snapshot` and `DeviceRail::new` receive the *same*
`DetEntropy` (seed 0x42, position 0 at snapshot), pad_echo never draws, so the
snapshot's ENTR position is 0 and replay restores position 0 — bit-identical. An
entropy-*drawing* guest would also replay identically: the snapshot captures the
position **at snapshot**, replay restores that and re-draws the same sequence the
recording drew from the same base. The `word_pos` granularity invariant in
`DetEntropy` makes the restore exact. Correct end to end.

### P3 — Quantization independence is real and the byte-identical reseal proves it

Recording quantizes at fixed 100k; replay quantizes by-record (one quantum per
input + the tail). Because the epoch grid is **absolute** in counter space
(`run_segment_with_epochs` grid-anchoring contract, confirmed in
`agenda`/`runctl`), the two differently-quantized runs emit the identical
`EPOCH_HASH` set, and the records interleave by `(icount, seq)` watermark to the
identical byte layout — which the `resealed == log_bytes` hammer asserts directly.
This is the strongest equality the layer can state and the test gates on it live.

### P4 — Loud-where-cut scope discipline (NotYetWired, never silent skip)

DEV_EVENT replay and any vectored input (a PAD_SET/NET_RX that queued an edge
vector) error as `ReplayError::NotYetWired` rather than silently skipping — and a
skipped input *is* a divergence, so this is the correct failure mode. The post-
apply `if !rail.borrow().irqs.is_empty()` check is good defense-in-depth: it
catches the vectored case even though `apply_pad_set`'s returned `Option<u8>` is
discarded, because pad-echo's `irq_vector` defaults to 0 (None) and any nonzero
config pushes to `irqs` and trips the check. Belt and suspenders.

### P5 — The negative test poisons RAM, not the chain seed — the *honest* mismatch

The divergence test mutates guest RAM at `0x60_0000` *after* the snapshot but
*before* recording, so the recording's hashes belong to a machine the snapshot
does not describe. I verified `push_final_link` hashes **full memory** (every page
ascending, `hash.rs` lines 129–147), so the poison perturbs the *first*
`EPOCH_HASH` and the divergence surfaces at the earliest possible point. The test
comment's insight — a poisoned chain *seed* would travel through time and stay
self-consistent, while RAM divergence is the genuine mismatch — is exactly right
and is the correct thing to test. The header-mismatch leg (wrong
`machine_config_hash`) also confirms refusal *before* any restore.

### P6 — The Vec-sink → callback conversion is well-motivated and well-documented

The iteration-87 batched-after-quantum approach regressed behind the monotone
DHILOG watermark (the writer rejects the regression); moving to a per-link sink
callback that fires *at the link point* is the correct fix, and the doc comments
in both `runctl.rs` (lines 177–194) and `recording.rs::log_epoch_hash`
(lines 274–288) explain *why* with enough specificity that a future reader will
not "optimize" it back. `run_segment`'s no-op `&mut |_, _, _| Ok(())` adapter
keeps the non-epoch callers unchanged. Clean refactor.
