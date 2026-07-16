# Suggestions (non-blocking)

## S1 — `digest8` 8-byte truncation is fine for a skew detector, but document the residual collision odds

`LogWriter::digest8` is the first 8 bytes of BLAKE3, LE u64
(`dhilog.rs:121`). For the encoder fingerprint its only job is to *detect
skew* between two encoder versions — it is a tripwire, not a security or
content hash. An 8-byte width is appropriate: 64-bit space, no adversary
(the inputs are a fixed const probe set, not attacker-controlled), and a
collision merely means two *different* encoders happen to produce the same
fingerprint over the 4 probes.

The failure mode is precise and worth a one-line comment: a wire-format
change that (a) alters SDK-digest output but (b) leaves all four probe
encodings byte-identical would slip past the guard — and even if the probes
*do* change, two distinct encoders collide on the truncated digest with
probability ~1/2^64. That is a silent false-agreement: the guard says
"encoders match" when they differ. 1/2^64 is negligible, so this is a "note
it, don't fix it" item — but the doc comment on `wire_encoder_fingerprint`
currently implies the guard is total ("Any wire-format change ... flips this
value"). Soften that to "any change to the probe-set encodings" and note the
2^-64 residual, so a future reader doesn't over-trust it.

## S2 — broaden the determinism of the fingerprint test toward its actual contract

`encoder_fingerprint_is_deterministic_and_logged_at_attach` only asserts
`wire_encoder_fingerprint() == wire_encoder_fingerprint()` (purity). The test
name promises "...and_logged_at_attach" but nothing in the body exercises the
attach path or checks a record was emitted. Either:

- rename it to `encoder_fingerprint_is_a_pure_function`, or
- add the assertion the name claims: drive `pio_out(PORT_INIT_GO)` to a
  successful attach and assert the log gained exactly one `KIND_ENCODER_FP`
  record (and — see C1 — add the companion test that `restore` does **not**,
  which is the bug-catching test that is currently missing).

A test that asserts the restore path's behavior would have surfaced C1.

## S3 — the `cpuid_leaves_hash` capacity hint is correct; consider a const for the per-leaf width

`cpuid_leaves_hash` reserves `leaves.len() * 28` and `encode_into` emits
exactly 7 × 4 = 28 bytes. They agree today, but they are two separate literal
`28`s (one here, one was the old `entries.len() * 28` in cpuid.rs that the
refactor removed). A `const LEAF_ENCODED_LEN: usize = 28;` (or
`= 7 * size_of::<u32>()`) referenced by both `encode_into`'s contract and the
capacity hint would keep them from drifting if a future leaf field lands. Pure
hygiene; the current code is correct.

## S4 — `wire_encoder_fingerprint` allocates two `Vec`s per call

`bytes` (growing) plus `buf` (`vec![0u8; MAX_RECORD_LEN]` = 4096 B) are
allocated on every call, and the function is invoked on every attach. This is
negligible (attach is rare, the result is tiny), so no action needed — but if
C1 moves emission to `LogWriter::new`, note that segment creation is also not
hot, so it stays fine there too. Mentioned only so it is not mistaken for a hot
path later.
