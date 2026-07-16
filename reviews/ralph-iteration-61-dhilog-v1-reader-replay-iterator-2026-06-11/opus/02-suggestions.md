# Suggestions (non-blocking)

### S1 — Update the API.md §3.1 table for the `[240..248)` encoder-fingerprint split

**File:** `.agents/docs/determinism-hypervisor/API.md:520`

The §3.1 table still says `| 240 | 16 | reserved | zeros; readers MUST reject nonzero |`,
but the writer (`dhilog.rs:299`) and this reader carve `[240..248)` out as the
`encoder_fingerprint` (bead 4ld) and treat only `[248..256)` as reserved-means-zero. The
writer is the in-repo authority and the reader is consistent with it; the spec doc is the
stale party. Suggest splitting the row:

```
| 240 | 8 | encoder_fingerprint | detguest-wire encoder fingerprint (bead 4ld); zero ⇒ no SDK digests |
| 248 | 8 | reserved | zeros; readers MUST reject nonzero (reserved-means-zero rule) |
```

This is a doc fix, not a code fix — the code split is correct. Without it, a future
reader-author following the table verbatim would (wrongly) reject any log with a nonzero
fingerprint.

### S2 — Consider rejecting `clock_den == 0` at parse time

**File:** `crates/dh-inputlog/src/reader.rs:354–369` (`parse_header`)

The writer comment (`dhilog.rs:22`) notes header field validity (e.g. `clock_den != 0`) is
"the MachineConfig layer's contract," and the reader serializes/deserializes verbatim. For
a hardened reader over *untrusted* bytes, a `clock_den == 0` will become a divide-by-zero
the moment a replayer computes the virtual-ns rate. Cheap to reject here with a dedicated
`ReadError::BadClock` rather than relying on every downstream consumer to re-check. Not
strictly required by §3.1 (which states no such reader rule), hence a suggestion — but the
determinism platform's whole posture is "validate up front."

### S3 — `EndMismatch` is a single coarse variant for four distinct END violations

**File:** `crates/dh-inputlog/src/reader.rs:70–72, 427–435`

`EndMismatch` fires for any of: `boundary_rip != 0`, nonzero END pad bytes, `icount !=
end_icount`, or `end_state_hash` mismatch. For a forensic/divergence tool ("locate the
fault without re-walking the file," per the `ReadError` doc), collapsing four causes into
one variant loses signal. Consider carrying a small reason discriminant or splitting the
variant. The test `rejects_end_mismatch` exercises only two of the four causes (hash and
icount) — see S5. Non-blocking; the current behavior is correct, just low-resolution.

### S4 — `NET_RX` has no minimum-length or layout test, and no kind has a golden-bytes fixture

**File:** `crates/dh-inputlog/tests/reader_validation.rs`

The writer cannot emit NET_RX/EPOCH_HASH/NET_TX yet, so the battery can't round-trip them —
understandable. But the reader *does* validate them, and they are currently exercised by
zero positive tests. A hand-rolled byte fixture for a NET_RX (e.g. `payload_len = 2048`
boundary, and `2049` rejected via `BadPayloadLayout`) and an EPOCH_HASH (to actually drive
the `FLAG_EPOCH_HASHES` true-path through `EpochHashesFlagMismatch`, which today is only
tested in the *false-positive* direction) would close the gap. More broadly, the research
note flags that *"round-trip tests alone can't catch wrong-but-symmetric layouts; golden
fixtures pin the actual bytes"* — there is currently no pinned-bytes fixture for any kind,
so a coordinated writer+reader layout drift would pass undetected. A single
`const GOLDEN: &[u8] = &[ … ];` decode assertion would catch that class.

### S5 — `rejects_end_mismatch` does not cover `boundary_rip != 0` or nonzero END pad

**File:** `crates/dh-inputlog/tests/reader_validation.rs:407–422`

Of the four conditions that produce `EndMismatch`, only `end_state_hash` and `icount` are
tested. Add two more surgeries: set the END record's `boundary_rip` (`end_rec_off+16..+24`)
nonzero, and set an END pad byte (`payload[1..8]`) nonzero. Cheap, and pins the full END
ruling. (If S3 is adopted, these become distinct-variant assertions.)

### S6 — `validate_kind`'s `_ => true` fall-through arm is unreachable but silent

**File:** `crates/dh-inputlog/src/reader.rs:480–493`

The `layout_ok` match has a trailing `_ => true`. By the time control reaches the layout
match, `kind` is guaranteed to be one of the known kinds (the `class_aux` match above
returned `Ok`/`Err` for every other case), so `_ => true` is dead. It is harmless, but a
`#[allow(unreachable_patterns)]`-free reader is clearer if the arm is removed and the match
made exhaustive over the known kinds, or a comment notes the arm is unreachable. Minor
readability nit.
