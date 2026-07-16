# Critical & Important Findings

## Critical

None. The byte format is genuinely frozen by the BLAKE3 pin + writer re-serialization, the fixtures are byte-correct against §3, the tests pass, and the build path is deterministic.

---

## Important

### I-1 — The "reader freeze" leg only spot-pins 3 of 10 record bodies; a reader-decode regression can hide in the unpinned records

**File:** `crates/dh-inputlog/tests/golden.rs:220-284` (`kitchen_sink_fixture_parses_to_expected_structure`) and the module doc at `:69-74`.

**What the freeze actually binds.** Legs (1) hash pin and (2) byte-identical re-serialization are *complete* over the writer: they cover every byte of the fixture, so a writer regression on **any** record — including the records the probe flagged as unpinned ([0],[2],[3],[4],[6],[8],[9]) — necessarily changes the hash and fails. So the writer side is fully frozen. Good.

The **reader** side is not. `LogReader::body()` (`reader.rs:157-205`) is what turns frozen bytes into typed fields, and it is the layer M6 consumers actually call. The parse test exercises it only partially:

- It collects `kinds` for all 11 records (kind byte only — offset 0 of each record header).
- It spot-pins exactly three bodies through the typed view: `records[1]` (`PadSet`), `records[5]` (`NetRx`), `records[7]` (`TimerFire`), plus `r.end()`.
- It **never** asserts the typed decode of:
  - `records[2]` / `records[3]` — the two detchannel `DEV_EVENT`s. `RecordBody::DevEvent` exposes `device_id: u16at(0)`, `event_type: u16at(2)`, `data: &p[8..]` (`reader.rs:168-172`). None of `device_id`, `event_type`, or the `data` slice start is checked through the reader. The ring_id/new_prod/new_cons inner layout is not decoded by the reader at all (it hands back a raw `data` slice), so the spec's §3.3 detchannel sub-encoding has **zero** reader-side assertions.
  - `records[4]` — `PIO_ANSWER` (also a `DEV_EVENT`; `port`/`value` only live inside the raw `data` slice).
  - `records[6]` — `ENTROPY` (`len`, `digest8`).
  - `records[8]` — `SDK_EVENT` (`stream: u16at(0)`, `len: u32at(4)`, `digest8: u64at(8)`).
  - `records[9]` — `FRAME_MARK` (`frame_index`).
  - `records[0]` — the first `PadSet` (only [1] is pinned).

**Why it matters.** Consider a future edit that swaps the field offsets inside `RecordBody::SdkEvent` (say `len: u32at(8)`, `digest8: u64at(4)`) or mis-slices `DevEvent` data as `&p[4..]`. The fixture bytes are unchanged, so leg (1) passes; the writer is unchanged, so leg (2) passes; and the parse test never touches those fields, so leg (3) passes too. **All three legs stay green on a real reader regression.** The module doc's claim "Any layout change breaks all three" (`:69-72`) is therefore not true for reader-side layout changes — it is true only for *writer/byte* layout changes. Since `bik`/`pee` (M6 acceptance) consume the reader, the regression net they inherit is weaker than the bead advertises.

**Fix.** Pin every record body through the typed view — it is cheap and self-documenting. Add to `kitchen_sink_fixture_parses_to_expected_structure`:

```rust
let records: Vec<_> = r.records().collect();

// [0] first PadSet
match records[0].body() {
    RecordBody::PadSet { port, buttons, frame_hint } =>
        assert_eq!((port, buttons, frame_hint), (1, 0x0000_00F0, FRAME_HINT_NONE)),
    other => panic!("expected PadSet, got {other:?}"),
}
// [2] RING_PUSH detchannel DEV_EVENT — pin the full payload incl. ring sub-encoding
match records[2].body() {
    RecordBody::DevEvent { device_id, event_type, data } => {
        assert_eq!((device_id, event_type), (DEVICE_ID_DETCHANNEL, EVENT_RING_PUSH));
        assert_eq!(data, &[1, 0, 0, 0, 2, 0, 0, 0, 0xDE, 0xAD, 0xBE, 0xEF, 1, 2, 3, 4]);
    }
    other => panic!("expected DevEvent, got {other:?}"),
}
// [3] CONS_BUMP
match records[3].body() {
    RecordBody::DevEvent { device_id, event_type, data } => {
        assert_eq!((device_id, event_type), (DEVICE_ID_DETCHANNEL, EVENT_CONS_BUMP));
        assert_eq!(data, &[2, 0, 0, 0, 5, 0, 0, 0]); // ring_id=2 (A), new_cons=5
    }
    other => panic!("expected DevEvent, got {other:?}"),
}
// [4] PIO_ANSWER (DEV_EVENT, port 0xD370, value 0x1234_5678)
match records[4].body() {
    RecordBody::DevEvent { device_id, event_type, data } => {
        assert_eq!((device_id, event_type), (DEVICE_ID_DETCHANNEL, EVENT_PIO_ANSWER));
        assert_eq!(data, &[0x70, 0xD3, 0x00, 0x00, 0x78, 0x56, 0x34, 0x12]);
    }
    other => panic!("expected DevEvent, got {other:?}"),
}
// [6] ENTROPY, [8] SDK_EVENT, [9] FRAME_MARK
match records[6].body() {
    RecordBody::Entropy { len, digest8 } =>
        assert_eq!((len, digest8), (64, 0x0102_0304_0506_0708)),
    other => panic!("expected Entropy, got {other:?}"),
}
match records[8].body() {
    RecordBody::SdkEvent { stream, len, digest8 } =>
        assert_eq!((stream, len, digest8), (5, 256, 0x1111_2222_3333_4444)),
    other => panic!("expected SdkEvent, got {other:?}"),
}
match records[9].body() {
    RecordBody::FrameMark { frame_index } => assert_eq!(frame_index, 42),
    other => panic!("expected FrameMark, got {other:?}"),
}
```

(Adjust the `EVENT_PIO_ANSWER` / `EVENT_*` constant names to whatever `dhilog.rs` exports — verify against the writer.) This closes the asymmetry so a reader-decode regression on *any* canonical or AUX kind fails the freeze, matching the doc's claim.

---

### I-2 — Module-doc freeze claim overstates reader coverage (normative-doc accuracy)

**File:** `crates/dh-inputlog/tests/golden.rs:69-72`.

```
//! THE V1.0 FORMAT FREEZES HERE: these tests assert (1) ... (2) ... and (3) the
//! reader parses the fixtures to the expected structure. Any layout change breaks
//! all three ...
```

As shown in I-1, "the expected structure" is in fact a *partial* structure (3 of 10 bodies), and "Any layout change breaks all three" is false for reader-side layout changes. Because this file is the human-readable contract for the freeze that two downstream beads depend on, the overstatement is itself a maintainability hazard: a future maintainer reading this doc will (reasonably) trust that the reader is fully pinned and may refactor `body()` offsets believing the golden suite guards them.

**Fix (preferred):** adopt I-1 so the claim becomes true. **Fallback** (if I-1 is deferred to a follow-up bead): soften the doc to match reality, e.g. "(3) the reader parses the fixtures and a representative body of each record *class* decodes to the expected fields; full per-record reader pinning is tracked in `<bead>`." Do not leave a true-sounding claim that the tests do not back.
