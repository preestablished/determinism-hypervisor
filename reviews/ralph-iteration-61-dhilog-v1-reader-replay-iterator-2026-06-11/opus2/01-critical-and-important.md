# Critical & Important Findings

## Critical

**None.** I attempted to break the parser with 20 adversarial inputs (two ENDs,
END-first-only, last non-END record at `icount == end_icount`, empty body,
`payload_len = 4096` exactly, payload claiming past EOF, a 12-byte body that straddles
the 24-byte record header, a `payload_len` that overlaps the next record's framing,
DEV_EVENT `data_len` disagreeing with `payload_len`, NET_RX at 2049, EPOCH_HASH/flag
mismatches in both directions, AUX/canonical class violations). Every one returned the
correct, precise `ReadError` — none panicked. No memory-safety or totality bug found.

---

## Important

### I-1 — API.md §3.1 normative table is stale: `[240..248)` is no longer `reserved`

- **File:** `.agents/docs/determinism-hypervisor/API.md:520` (the §3.1 header table)
  vs `crates/dh-inputlog/src/reader.rs:351,368` and `crates/dh-inputlog/src/dhilog.rs:299`
- **Severity:** Important (documentation / normative-spec correctness — not a code bug)

API.md §3.1 still says:

```
| 240 | 16 | `reserved` | zeros; readers MUST reject nonzero (reserved-means-zero rule) |
```

But bead 4ld (closed 2026-06-10) repurposed `[240..248)` as the detguest-wire
`encoder_fingerprint`. This branch's reader reads it as a header field
(`parse_header`, reader.rs:368) and only enforces the reserved-means-zero rule on
`[248..256)` (reader.rs:351). The writer writes the fingerprint into `[240..248)`
(dhilog.rs:299) and leaves `[248..256)` zero (dhilog.rs:302). The `Header`
doc-comment (reader.rs:96–99) and the test `rejects_nonzero_reserved_but_reads_fingerprint`
(reader_validation.rs:224–238) both document and assert the new split.

So the **implementation is internally consistent and correct** — writer, reader, and
tests agree. The problem is the *normative byte-level spec disagrees with the shipped
format*. For a format whose entire value proposition is "this file IS the
replayability guarantee," a normative table that lists a 16-byte reserved field where
the code reads an 8-byte fingerprint + 8-byte reserved is a real hazard: an
independent implementer reading only API.md would (a) reject a valid log with a
nonzero fingerprint as `ReservedNonzero`, and (b) lose the encoder-skew detection 4ld
shipped. The in-repo writer is the working authority, so the code is right and the
**doc must be fixed.**

**Fix** (API.md §3.1, replace the single 16-byte reserved row with two rows):

```
| 240 | 8  | `encoder_fingerprint` | `u64` detguest-wire encoder fingerprint (bead 4ld); 0 ⇒ no SDK digests in this segment. Verifiers compare fingerprints before SDK_EVENT digests to detect encoder skew. |
| 248 | 8  | `reserved` | zeros; readers MUST reject nonzero (reserved-means-zero rule) |
```

If editing the normative spec inside this bead is out of scope, **file a doc bead**
(`-l docs -p 1`) referencing 4ld and this iteration so the divergence is tracked rather
than silently carried. Either way this should not merge without the divergence being
recorded somewhere actionable. State explicitly in the bead that the **code is
authoritative and correct**; only the table is wrong.
