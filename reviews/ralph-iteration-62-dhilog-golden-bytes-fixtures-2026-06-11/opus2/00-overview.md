# DHILOG v1.0 Golden-Bytes Freeze — 2nd Independent Review

- **Branch:** `ralph/iteration-62-dhilog-golden-bytes-fixtures` vs `main`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus (2nd reviewer)
- **Bead:** bp9 — freeze DHILOG v1.0 byte format; blocks M6 acceptance beads `bik`, `pee`.

## Summary

This change freezes the DHILOG v1.0 wire format (normative: `.agents/docs/determinism-hypervisor/API.md` §3) by checking in two binary golden fixtures and a `tests/golden.rs` harness, plus a new `LogWriter::net_rx` so the kitchen-sink fixture can exercise every writer-emittable canonical kind.

The freeze rests on three legs per fixture:
1. **BLAKE3 pin** of the checked-in bytes (the anchor).
2. **Byte-identical re-serialization** — today's writer reproduces the frozen bytes from fixed inputs.
3. **Reader parse** — `LogReader::parse` decodes the fixture to an asserted structure.

I verified the work hands-on: ran `cargo test -p dh-inputlog --test golden` (4/4 pass), hexdumped both fixtures and checked the first 16 bytes against §3.1 (magic `DHILOG`, version `0x0100`, header_len 256, flags), and decoded the hand-rolled detchannel `RING_PUSH`/`CONS_BUMP` data arrays byte-for-byte against §3.3. All byte-level encodings are correct and the ring-id choices (1=I for RING_PUSH, 2=A for CONS_BUMP) are spec-valid and match their inline comments.

The headline finding is **not** a byte error — the bytes are right. It is a **coverage asymmetry**: legs (1) and (2) freeze the *writer* completely (every byte of every record, pinned and unpinned alike, is covered by the hash + re-serialize). Leg (3), the *reader* freeze, is only partial — it spot-pins three record bodies (PadSet[1], NetRx[5], TimerFire[7]) and END, but never asserts the typed-view decode of the two detchannel `DEV_EVENT` records, `PIO_ANSWER`, `ENTROPY`, `SDK_EVENT`, or `FRAME_MARK`. A *reader* regression (e.g. swapped field offsets in `SdkEvent` or a wrong `DevEvent` data slice start) would slip through these golden tests, because the hash pin freezes writer output, not reader interpretation. The module doc's strong claim — that any layout change "breaks all three" — overstates leg (3).

This is an Important (not Critical) gap: it does not block the freeze of the *byte format* itself (which is genuinely frozen by legs 1+2), but it weakens the regression net the bead advertises, and the two M6 acceptance beads that build on the reader inherit that weaker net.

## Verdict

**APPROVE WITH CHANGES.** The format freeze is sound and the fixtures are byte-correct; merge is not blocked on a correctness defect. The reader-coverage gap (Important) and the overstated module-doc claim should be addressed either here or as an immediate follow-up bead before `bik`/`pee` lean on the reader.

## Stats

| Severity | Count |
|---|---:|
| Critical | 0 |
| Important | 2 |
| Suggestions | 5 |

Files reviewed:
- `crates/dh-inputlog/tests/golden.rs` (new, 235 lines)
- `crates/dh-inputlog/tests/fixtures/v1_kitchen_sink.dhilog` (new, 720 B binary)
- `crates/dh-inputlog/tests/fixtures/v1_minimal.dhilog` (new, 320 B binary)
- `crates/dh-inputlog/src/dhilog.rs` (+22/-5; new `net_rx`, doc update)
- Cross-referenced: `crates/dh-inputlog/src/reader.rs`, `.gitattributes`, `.agents/docs/determinism-hypervisor/API.md` §3.
