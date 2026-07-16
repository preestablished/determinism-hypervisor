# Critical & Important Issues

## Critical

**None.** The `LogReader::parse` decode path is total over untrusted bytes. I traced every
slice index back to a dominating bounds check:

- `parse_header`: gated by the single `bytes.len() < HEADER_LEN` (256) check at the top;
  every subsequent fixed offset (`[0..6]` … `[248..256]`) is `< 256`.
- `validate_records` loop: `body.len() - offset < 24` dominates the 24-byte header reads;
  `payload_len > MAX_PAYLOAD` (4096) then `body.len() - offset < padded` dominate the
  payload + padding reads. `24 + payload_len + pad_len` maxes at 4127 — no `usize` overflow.
- `validate_kind`: runs *before* the `match kind` END block, so END's `payload[1..8]` /
  `payload[8..40]` accesses are dominated by `validate_kind`'s `payload.len() == 40` check.
- `Record::body()` on a **parsed** record: every known-kind offset is covered —
  PAD_SET(==12), DEV_EVENT(>=8), ENTROPY/SDK_EVENT/NET_TX(==16), TIMER_FIRE(==20),
  EPOCH_HASH/END(==40), FRAME_MARK(==8), NET_RX(no fixed index). DEV_EVENT's
  `data_len == payload_len - 8` invariant is enforced in `validate_kind` (line 482–485)
  using `payload[4..8]`, itself guarded by the `>= 8` check on the same line.
- `Records::next()` / `Records` is only constructible via `LogReader::records()` on an
  already-validated image (private fields, no public ctor), so its unchecked indexing
  (`b[2..4]`, `b[24..24+payload_len]`) can never desync.

Confirmed empirically: `arbitrary_truncations_never_panic` and
`single_byte_corruptions_never_panic` pass, and all 35 tests are green.

---

## Important

### I1 — `Record::body()` is documented "Infallible" but can panic on a hand-built `Record`

**File:** `crates/dh-inputlog/src/reader.rs:111–178` (struct `Record`, fn `body`)

`Record` exposes **all** fields `pub` (`kind`, `rflags`, `seq`, `icount`, `boundary_rip`,
`payload`), and `body()` is `pub` with a doc-comment that states: *"Typed view of the
payload. Infallible: `LogReader::parse` already validated every known layout…"*. That
infallibility invariant lives in `parse()`, **not** in the `Record` type. Any caller — in
this crate or a downstream replayer — can construct a `Record` directly and call `body()`:

```rust
use dh_inputlog::reader::{Record, RecordBody};
let r = Record { kind: 0x7F /*END*/, rflags: 1, seq: 0, icount: 0, boundary_rip: 0, payload: &[] };
let _ = r.body(); // PANIC: p[0] / p[8..40] out of bounds on empty payload
```

The closures `u16at`/`u32at`/`u64at` and the `try_into().unwrap()` array conversions all
assume the validated layout. The method's own contract ("Infallible") is therefore false
for the public type. No in-tree caller hits this today, so it is not a live exploit — but
it is a latent panic in a security-sensitive crate whose entire selling point is totality,
and the doc-comment actively invites the misuse.

**Fix (pick one):**

1. **Preferred — seal the invariant in the type.** Make `Record`'s fields non-`pub`
   (expose them via accessor methods) so the only construction path is the validated
   iterator. Then `body()`'s infallibility is a true type-level guarantee.

   ```rust
   pub struct Record<'a> {
       kind: u8, rflags: u8, seq: u32, icount: u64, boundary_rip: u64, payload: &'a [u8],
   }
   impl<'a> Record<'a> {
       pub fn kind(&self) -> u8 { self.kind }
       pub fn seq(&self) -> u32 { self.seq }
       // … etc; payload() -> &'a [u8]
   }
   ```

2. **Minimal — make `body()` total.** Replace direct indexing with `get(..)` and fall
   back to `RecordBody::Unknown` (or a new `Malformed`) on short payloads, so a forged
   `Record` can never panic even though parse() guarantees it never happens for real ones.

3. **Cheapest — fix the docs.** If keeping public fields is intentional (e.g. for test
   byte-surgery convenience), change the doc-comment to state the precondition explicitly:
   *"Infallible **only for records yielded by a parsed `LogReader`**; constructing a
   `Record` by hand and calling `body()` with a payload shorter than the kind's §3.3
   layout will panic."* This removes the false guarantee but keeps the hazard.

**Recommendation:** Option 1. It is the no-extra-runtime-cost choice and makes the
module's headline claim ("the iterators it hands out are infallible views over
already-validated bytes") true by construction rather than by convention.

**Research ref:** no_std wire-codec rules — *"Decoders over untrusted bytes must be
total… prefer `get(..)`/`try_into` patterns over direct indexing in decode paths."* The
`parse` path honors this; the public `body()` on a forgeable type is the one place the
guarantee leaks.
