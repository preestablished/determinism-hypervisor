# Positive Notes

## P-1 — Genuinely total decoder, with the length checks placed before every slice

`validate_records` (reader.rs:375–465) checks `body.len() - offset < 24` (line 388)
before reading the record header, computes `padded` and checks
`body.len() - offset < padded` (line 403) before touching the payload, and only then
slices. The module-level comment "Every slice index below is dominated by the explicit
length checks at the top of the loop body" (reader.rs:373–374) is *true*, which is rare
to be able to say. I confirmed it empirically: the in-tree `arbitrary_truncations_never_panic`
(every prefix length) and `single_byte_corruptions_never_panic` (every byte flipped,
with and without reseal) both pass, and my 20 hand-built adversarial logs never panicked.

## P-2 — The `u16` `payload_len` overflow reasoning holds on 32-bit too

`payload_len` is a `u16` (≤ 65535) and is checked against `MAX_PAYLOAD` (4096) at
reader.rs:399 *before* `padded = 24 + payload_len + pad_len(...)` is computed
(reader.rs:402). Max `padded = 24 + 4096 + 7 = 4127`, far below `usize::MAX` even on a
32-bit target. No silent wrap is reachable. The infallible `Records::next` iterator
(reader.rs:306–322) trusts the already-validated body, so its unchecked `24 + payload_len + pad_len`
is safe by construction — a good separation of the validating walk from the cheap
re-walk.

## P-3 — Defense-in-depth: corrupting `payload_len` is caught by the seq watermark

Probe PLEN-OVERLAP (a record whose inflated `payload_len` overlaps the next record's
framing, resealed so the hash gate passes) was caught by `SeqMismatch` — the misaligned
walk lands on the wrong seq. The reader does not rely on any single check; framing,
seq, and body_hash form overlapping nets. This is exactly the property you want in an
adversarial parser.

## P-4 — END semantics fully and correctly enforced

The END block (reader.rs:426–437) enforces every clause of the §3.3 END ruling:
AUX-flagged (via `validate_kind` class check), `boundary_rip == 0`, the 7 pad bytes
zero, `icount == header.end_icount`, and `end_state_hash == header.end_state_hash`.
Crucially, `validate_kind` runs *first* (reader.rs:424) and pins END's payload to
exactly 40 bytes, so the subsequent `payload[1..8]` and `payload[8..40]` accesses
(reader.rs:430,432) are provably in-bounds — the ordering is correct, not lucky. EndNotLast
fires on: a non-END last record at `end_icount` (P1), two ENDs (P2), END-then-record
(P12), and empty body (P4). END-first-only (P3) correctly parses.

## P-5 — The HAS_AUX / EPOCH_HASHES folds match the writer's "END is not AUX" ruling

The fold at reader.rs:457–463 mirrors the writer's snapshot-has_aux-before-END trick
(dhilog.rs:278–280): END is AUX-flagged but does **not** count toward `HAS_AUX`, while
EPOCH_HASH does (it is folded into the `has_aux` expectation at reader.rs:458 *and*
drives the separate EPOCH_HASHES flag at 461). I verified both directions: an
EPOCH-only log with `HAS_AUX` clear is rejected `HasAuxFlagMismatch` (P10), EPOCH
present with the EPOCH flag clear is rejected (P9a), the EPOCH flag set with no EPOCH
records is rejected (P9b), and the correctly-flagged EPOCH-only log parses (P10b). The
`empty_log_parses_with_just_end` test plus the writer's `aux_flag_set_only_by_real_aux_records`
nail the END-alone case from both sides. This is the subtlest invariant in the format
and it is correct on both the read and write sides.

## P-6 — Forward-compatibility split is spec-correct and tested

Unknown **canonical** kinds are rejected (`UnknownCanonicalKind`, a replayer cannot
apply them) while unknown **AUX** kinds are accepted and surface as
`RecordBody::Unknown` (reader.rs:474, 176). This is exactly the §3.4 minor-version
extension contract. I confirmed `body()` on a 4096-byte unknown-AUX payload stays
in-bounds (probe P5). The `rejects_unknown_canonical_kind_accepts_unknown_aux` test
covers both arms and locates the AUX record by walking the framing rather than
hard-coding an offset — robust against future layout changes.

## P-7 — Test battery is honest and well-constructed

The `reseal` helper (reader_validation.rs:53–56) recomputes `body_hash` after byte
surgery so the *targeted* check fails rather than the hash gate in front of it — a
common way for negative tests to silently test the wrong thing, avoided here
deliberately and documented in the file header. Coverage is one negative per
`ReadError` variant plus the two totality smokes. DEV_EVENT's `data_len`-must-equal-
`payload_len-8` invariant (reader.rs:482–486) is correctly enforced — I verified a
`data_len` lie is rejected `BadPayloadLayout` (P7) and a minimal 8-byte/`data_len=0`
DEV_EVENT is accepted (P6).
