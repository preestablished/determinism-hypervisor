# Positive Notes

### The three-leg freeze structure is the right design
`golden.rs` separates the BLAKE3 pin (anchor against *any* mutation), the writer re-serialization (catches writer drift specifically), and the reader parse (validates decode). The failure messages are aimed at the right culprit — `"the v1.0 freeze is violated"` vs `"writer output drifted from the frozen v1.0 fixture"` (`:195`, `:201`) tell a future maintainer whether to fix the writer or whether the fixture itself was tampered with. That diagnostic split is exactly what you want from a freeze suite.

### The fixtures are byte-correct against the normative spec — verified independently
I hexdumped both files and decoded them by hand against API.md §3:
- Header: `44 48 49 4c 4f 47` (`DHILOG`), version `00 01` = `0x0100` (v1.0), header_len `00 01 00 00` = 256, kitchen-sink flags `03` = `SEALED|HAS_AUX`, minimal flags `01` = `SEALED`. All match §3.1.
- The hand-rolled detchannel arrays are correct: `RING_PUSH` at the [2] record decodes to `device_id=0x0001, event_type=0x0001, ring_id=1 (I), new_prod=2, 8 record bytes` and `CONS_BUMP` at [3] to `ring_id=2 (A), new_cons=5` — both ring ids are spec-valid for their event type (§3.3: RING_PUSH ∈ {0,1}, CONS_BUMP ∈ {2,3}) **and** the inline `// ring 1 (I)` / `// ring 2 (A)` comments match the bytes. No comment-vs-byte drift, which is a common rot point in hand-rolled fixtures.

### Build path is provably deterministic
`grep` for `HashMap|BTreeMap|SystemTime|Instant|now()|rand|thread_rng` across `dhilog.rs` and `reader.rs` returns nothing. The fixture build is fixed inputs → fixed `Vec<u8>` → BLAKE3; no iteration-order or timestamp non-determinism can leak into the frozen bytes. This is the property that makes a golden-byte freeze meaningful at all, and it holds.

### Scope discipline on M5 kinds is correct, not an omission
`EPOCH_HASH` (`0x42`) and `NET_TX` (`0x44`) have **no** writer method (`grep` of `pub fn` in `dhilog.rs` confirms), so they *cannot* be frozen by a writer-re-serialization fixture — and the module doc (`dhilog.rs:18`) correctly scopes their emission to M5. The freeze covers exactly the writer-emittable v1 surface and honestly says so, rather than silently pretending to freeze kinds it can't produce. The probe's worry here checks out clean.

### `net_rx` lands with the freeze for the right reason
Adding `LogWriter::net_rx` (`dhilog.rs:186-196`) specifically so the kitchen sink can exercise the `NET_RX` (`0x03`) canonical kind is the correct move — it keeps the "freeze covers every canonical kind" claim true rather than leaving a canonical kind unrepresented in the golden fixture. The `MAX_NET_RX_FRAME` (2048) cap with `PayloadTooLong` mirrors the §3.3 constraint, and the doc comment cites the spec section. Clean, minimal, well-scoped addition.

### `minimal` fixture pins the genuinely degenerate semantics
`build_minimal` (`:153-170`) deliberately exercises the spec's zero-means-X rules — `entropy_seed: [0u8;32]` ("continue base PRNG stream"), `encoder_fingerprint: 0` ("no SDK digests"), `end_snapshot_id: [0u8;32]` ("no end snapshot") — and the parse test asserts `encoder_fingerprint == 0` and `has_aux() == false`. Freezing the all-zeros degenerate header alongside the rich one is good coverage instinct: the two fixtures bracket the format's sparse and dense extremes.
