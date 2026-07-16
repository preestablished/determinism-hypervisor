# Positive Notes

## P1 — The "validated sequence, not byte-concat" decision is correct and superbly documented

The module-level doc (`splice.rs:1-26`) is the best part of this change. It explains *why* a
splice is a validated sequence of independently-sealed v1 segments rather than a
concatenated file: DHILOG v1 stays frozen, and the content-addressed snapshot refs make the
induction sound (equal ref ⇒ bit-identical state, so `VerifyReplay(base=snapshot_{i-1},
log_i)` per edge is a real root-anchored proof). It even calls out the subtle point that
icount/seq axes *restart* per segment by design (§3.1), so there is no cross-segment
watermark to maintain — the per-segment watermark plus the stitch rule IS the continuity.
A reviewer (or the cw2 author) can reconstruct the entire design rationale from the doc
without reading INTEGRATION.md. This is how to document a codec-adjacent invariant.

## P2 — Total over hostile input by inheritance; matches the no_std codec research

Every segment goes through `LogReader::parse` (`splice.rs:71,89,101,118,154`), which the
reader module documents and proves is a *total* decoder (`reader.rs:1-8`: "every input
yields Ok or a ReadError, never a panic"). `splice.rs` adds no raw byte indexing of its own
— it only reads already-validated `Header` fields. So the splice layer cannot introduce a
panic on hostile bytes that the reader didn't already reject. This is exactly the discipline
`~/.claude/research/rust-nostd-wire-codecs.md` prescribes ("decoders over untrusted bytes
must be total; trace every slice index to a dominating bounds check") — splice has zero
unchecked slice indices, so there is nothing to trace.

## P3 — Continuity rules are ordered correctly and structurally preclude the dangerous shape

The validation order in `new()` — parse all → one-machine check → stitch/end check — means a
mid-lineage zero base is *structurally impossible* (the `MissingEndSnapshot` rule on inner
ends, :85, combined with the stitch equality, :88, leaves no path to a zero inner base). The
inner-must-have-end rule is the linchpin: it is what makes the induction anchored rather than
"any two logs whose refs happen to be zero". This is the correct place to enforce it.

## P4 — Error variants are forensic and carry the locating index

Every `SpliceError` variant carries the segment `index` (or `index/index+1` for stitches),
mirroring the reader's "carry the seq where applicable" philosophy (`reader.rs:27`). A
hostile or corrupt lineage faults loudly *and* points at the exact segment — exactly what a
1000-fork harness needs to triage which child broke without re-walking the batch.

## P5 — `is_empty()` honesty and the clippy-pairing comment

`is_empty()` returns a hard `false` with a comment explaining it exists only for clippy's
`len`/`is_empty` lint pairing, since `new()` rejects empty (:126). This is the right call —
a `Lineage` that exists is non-empty by construction, so `is_empty()` returning `false`
unconditionally is *true*, not a stub. The `expect("non-empty by construction")` in
`end_identity()` (:139) and `expect("validated at construction")` in `edges()` (:154) are
likewise correct invariant assertions, not papered-over panics: both are unreachable given
`new()`'s guarantees, and the messages name the guarantee.

## P6 — `edges()` returns an owning iterator with the right lifetime

`edges() -> impl Iterator<Item = Edge<'a>> + '_` (:147) correctly gives `Edge` the segment
lifetime `'a` (the bytes outlive the `Lineage`) while bounding the iterator by the
`Lineage` borrow `'_`. The re-parse-per-edge (:154) is provably infallible (validated at
construction) and keeps `Edge` self-contained — a consumer can drain the iterator and hold
each `Edge` independently. (See 02 item 3 for the one consumer-fit wrinkle: the consumer
ultimately re-parses from bytes anyway.)
