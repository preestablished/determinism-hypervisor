# Positive Notes

### P1 — The "validate up front, iterate infallibly" architecture is exactly right

`LogReader::parse` (reader.rs:247–257) runs the full battery — `parse_header`, body-hash,
`validate_records` (framing + watermark + seq + layouts + flag consistency + END ruling) —
*before* handing out any iterator. The `records()`/`canonical()`/`aux()` accessors then do
zero re-validation. This is the correct shape for a security-sensitive codec: the untrusted
surface is one function, and everything downstream operates on a proven-good image. The
module doc-comment (reader.rs:1–18) articulates this contract precisely.

### P2 — Genuine panic-freedom on the parse path, with self-checking tests

Every slice index in the decode path is dominated by an explicit length check
(`body.len() - offset < 24`, `payload_len > MAX_PAYLOAD`, `body.len() - offset < padded`),
and `validate_kind` runs before any kind-specific payload access so the END
`payload[1..8]`/`[8..40]` reads and all `body()` offsets are guaranteed. The two totality
smoke tests (`arbitrary_truncations_never_panic`, `single_byte_corruptions_never_panic`,
reader_validation.rs:451–469) are an excellent precursor to the planned 1j4 fuzz target and
already sweep every truncation length and every single-byte flip (both with and without
reseal). This directly satisfies the research rule *"Decoders over untrusted bytes must be
total… lock this in with a fuzz target whose harness treats panics as crashes."*

### P3 — The reseal helper makes the negative tests actually pin their target

`reseal()` (reader_validation.rs:53–56) recomputes `body_hash` after record surgery so the
body-hash gate passes and the *targeted* validation rule is what fails — not the hash gate
in front of it. Without this, half the record-level negatives would trivially fail on
`BodyHashMismatch` and prove nothing. The battery uses it consistently, and
`rejects_body_hash_mismatch` (which deliberately *skips* reseal) confirms the gate itself
still fires. This is the difference between tests that assert a contract and tests that
pass for the wrong reason.

### P4 — AUX-skipping / canonical-vs-AUX contract is implemented and tested coherently

The known/unknown × canonical/AUX matrix in `validate_kind` (reader.rs:469–497) correctly
encodes §3.4: unknown **AUX** kinds are accepted (forward-compat minor extension, surface
as `RecordBody::Unknown`), unknown **canonical** kinds are rejected
(`UnknownCanonicalKind` — a replayer cannot apply them), and a known kind whose AUX flag
contradicts its §3.3 class is rejected (`KindAuxMismatch`).
`rejects_unknown_canonical_kind_accepts_unknown_aux` (reader_validation.rs:304–336) drives
*both* sides of that fork, and notably locates the ENTROPY record by **walking the framing**
rather than hard-coding an offset — robust against layout changes.

### P5 — END semantics fully cross-checked against the header

The END ruling (reader.rs:426–437) verifies all four §3.3 invariants — `rflags.AUX = 1`
(via the class table), `boundary_rip = 0`, zero pad bytes, and the payload's
`icount`/`end_state_hash` matching `header.end_icount`/`header.end_state_hash` — plus
`EndNotLast` for both "no END" and "records after END," and the subtle `HAS_AUX` rule that
END alone does not set the flag (`empty_log_parses_with_just_end`, reader_validation.rs:160).
This is a faithful, complete encoding of the writer's sealing contract (dhilog.rs:270–304).

### P6 — Clean spec-fidelity on the fingerprint/reserved split, matching the writer

Despite the stale API.md table (see S1), the reader correctly mirrors the writer: read
`encoder_fingerprint` from `[240..248)`, enforce reserved-means-zero only on `[248..256)`
(reader.rs:351, 368). `rejects_nonzero_reserved_but_reads_fingerprint`
(reader_validation.rs:223–238) pins exactly this split. Choosing the writer as the
authority over the doc table is the right call and is called out in-code.

### P7 — Error variants carry locating context

Nearly every `ReadError` variant carries `seq` (and `kind`/`flags`/`found` where relevant),
so a verification/divergence tool can locate the fault without re-walking the file — stated
intent in the `ReadError` doc-comment (reader.rs:26–27) and honored throughout. The
`seq_for_err = u32::try_from(count).unwrap_or(u32::MAX)` pattern (reader.rs:384) even keeps
the error-reporting path itself panic-free for pathological counts.
