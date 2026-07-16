# Suggestions (non-blocking)

### S1 — `Divergence.expected/got` are `[0; 32]` placeholders for non-hash cases

`crates/dh-worker/src/replay_engine.rs`. Several `Divergence` returns set
`expected`/`got` to `[0; 32]` with the real detail buried in a formatted string
that is *thrown away* (the EPOCH_HASH mismatch message at lines ~199–211 is
constructed inside the sink, then the outer `map_err` discards it and substitutes
`what: "EPOCH_HASH (see message)"` with zeroed bytes — so "see message" points at
a message no longer reachable). Same for the EPOCH_HASH-count mismatch
(lines ~284–290) and the byte-compare failure (lines ~320–326, where `got` is
`[0; 32]` "byte-compare failed; the diff is in the logs" but nothing is logged).
Consider an enum better shaped to the data: a `Divergence` variant carrying an
owned `String` detail, or per-kind variants (`EpochHashMismatch { at_icount,
epoch, expected, got }`, `EndVnsMismatch { expected: u64, got: u64 }`). The
negative test only asserts `what.contains("EPOCH_HASH")`, so the lost detail is a
debuggability cost, not a correctness one — but on a real corpus divergence you
will want the actual mismatching epoch bytes, not zeros.

### S2 — `expected: header.body_hash` on the byte-compare branch is misleading

`crates/dh-worker/src/replay_engine.rs` line ~324: the resealed-bytes divergence
sets `expected: header.body_hash, got: [0; 32]`. `body_hash` is the *input's* body
hash, not "what was expected of the reseal" — a reader will misread it as a
hash comparison when the actual failure was a full-buffer `Vec<u8>` inequality.
Either compute and compare body hashes explicitly (and report both), or drop the
field to `[0;32]`/zero with a clearer `what`. Minor, but this is the one place a
real reseal divergence will land and the operator wants an honest signal.

### S3 — The reseal hammer subsumes the per-record checks; document the cost trade

The module doc (lines 20–24) correctly states the reseal is byte-identical and
"subsuming the per-record checks (which exist for granular divergence
reporting)". Worth a one-line note that this means *every* successful replay pays
the full re-serialization + `blake3` over the whole log even after all granular
checks passed — fine for M5 segments, but a comment flagging it as a known cost
(and a candidate to gate behind a verify-mode flag for very large logs) saves a
future reader from re-deriving it. Not a change request.

### S4 — `run_to` recomputes `counter.read()` as its own start each call

`crates/dh-worker/src/replay_engine.rs` `run_to` (line ~160). Each `run_to`
re-reads the counter for `start`. This is correct and robust (it self-syncs to the
real counter rather than trusting a tracked cursor), and `run_segment_with_epochs`
re-asserts `start_icount == counter` anyway. No change needed — but a one-line
comment that the redundant read is *deliberate* (defends against a record whose
icount the rail somehow advanced past) would pre-empt a "dead read" cleanup PR.

### S5 — Test asserts `epoch_hashes_verified == 10` with a derivation that is
slightly off in the comment

`crates/dh-worker/tests/replay_engine.rs` line ~810: the comment says
"3 quanta x (100k/30k grid) = epochs 1..=10 minus none: 300k/30k=10". The
per-quantum framing (100k/30k ≈ 3.33) doesn't cleanly yield 10; the honest
derivation is the *absolute* grid: epochs at 30k,60k,…,300k = `300_000 / 30_000`
= 10, with the 300k epoch coinciding with the final budget stop (still hashed
before the `final_stop` return — confirmed in `run_segment_with_epochs`
lines 344–351 firing before line 368). The assertion is correct; only the
comment's intermediate arithmetic is muddled. Tighten it so a future reader
trusts the number.
