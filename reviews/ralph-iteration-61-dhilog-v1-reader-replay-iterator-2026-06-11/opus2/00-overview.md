# DHILOG v1 Reader — Second-Reviewer Overview

- **Branch:** `ralph/iteration-61-dhilog-v1-reader-replay-iterator` vs `main`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus (2nd reviewer)
- **Bead:** ecv (read side of the DHILOG v1 codec; API.md §3 normative)
- **Scope:** `crates/dh-inputlog/src/reader.rs` (new, 498 LOC), `crates/dh-inputlog/src/dhilog.rs` (writer — spec constants added: `KIND_NET_RX/EPOCH_HASH/NET_TX/FRAME_MARK`, `MAX_NET_RX_FRAME`, `FLAG_EPOCH_HASHES`), `crates/dh-inputlog/tests/reader_validation.rs` (new, 25 tests).

## Summary

`LogReader::parse` is a genuinely **total** decoder over untrusted bytes: I ran 14
hand-assembled adversarial logs plus the in-tree `arbitrary_truncations_never_panic`
and `single_byte_corruptions_never_panic` smoke tests — no panic on any input, every
input resolves to `Ok` or a precise `ReadError`. The framing walk does its length
checks before every slice, the `u16` `payload_len` is bounded by `MAX_PAYLOAD` (4096)
before the padded length is computed (so `24 + payload + pad ≤ 4127` — no `usize`
overflow even on a 32-bit target), and the typed `body()` accessor only runs over
payloads whose exact length `validate_kind` already enforced. END semantics
(AUX-flagged, last, present, `boundary_rip = 0`, zero pad, header cross-check) are all
enforced, and the `(icount, seq)` watermark plus the `record_count` / `HAS_AUX` /
`EPOCH_HASHES` consistency folds all fire on the right inputs.

The one substantive finding is **not in the code** — it is a **stale normative spec**:
API.md §3.1 still describes bytes `[240..256)` as 16 bytes of `reserved` ("readers MUST
reject nonzero"), but bead 4ld (closed 2026-06-10) repurposed `[240..248)` as the
`encoder_fingerprint`, and this branch's reader, writer, and the header doc-comment all
implement that. The byte-level normative table now disagrees with the working
implementation. This needs an API.md edit (Important — see 01).

Everything else is either a non-blocking suggestion (spec-acceptable gaps the prompt
asked me to weigh: unvalidated END `stop_reason`, zero-length NET_RX frame,
wholesale unsealed rejection) or a cosmetic note (saturated `SeqMismatch.expected`
after an unreachable 2^32-record count; `end()` re-walks the body).

## Verdict

**APPROVE (with one Important doc fix).** The parser is correct, total, and
well-tested. No code change blocks the merge; the API.md §3.1 reserved-bytes row must
be corrected (or a doc bead filed) so the normative spec matches the shipped wire
format. The reader is sufficient to support bead bp9's byte-identical re-serialization
assertion — the original buffer *is* the validated bytes, and `body`/`Header` together
expose everything needed.

## Stats

| Metric | Value |
|---|---|
| Files reviewed | 3 (reader.rs, dhilog.rs, reader_validation.rs) |
| `cargo test -p dh-inputlog` | 25 passed, 0 failed |
| Adversarial probes run | 20 (14 semantic + 6 boundary), all clean |
| Critical findings | 0 |
| Important findings | 1 (API.md §3.1 stale vs implementation) |
| Suggestions | 5 |
| Positive notes | 7 |
