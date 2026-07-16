# Critical & Important findings

## Critical

**None.** I walked every replay-divergence angle in the prompt against the
consumed library sources and the DHILOG framing. The record/replay byte streams
this module produces are deterministic functions of guest state in every path I
traced. The closest call (the truncated-event re-encode digest) is verified safe
below under I-1.

---

## Important

### I-1. Truncated / over-cap re-encode digest is correct, but the safety argument is subtle and untested — lock it down

`sdk_event_digest` (detchannel.rs:489–507) does **not** reconstruct
`FLAG_TRUNCATED` when it re-encodes a drained payload:

```rust
let extra_flags = match &ev.payload {
    OwnedPayload::NameIntern { reachable_decl: true, .. } => FLAG_REACHABLE_DECL,
    _ => 0,
};
...
let n = encode_event(&mut buf, ev.seq, ev.vnanos, extra_flags, &payload).ok()?;
let payload_bytes = &buf[RECORD_HEADER_LEN..n];
```

`GuestEvent.truncated` (drain.rs:301, the header's `FLAG_TRUNCATED`) is dropped.
The prompt's worry is that a guest which emitted a truncated `AssertViolation` /
`LogLine` would re-encode to *different* bytes (or fail) on the host side,
diverging record from replay. **It does not, for two reasons I verified:**

1. **The flag lives in the record header, not the payload.** `encode_event`
   (events.rs:343–357, 442–456) sets `FLAG_TRUNCATED` in the `RecordHeader.flags`
   byte. The digest is taken over `buf[RECORD_HEADER_LEN..n]` — the header
   (bytes 0..16) is excluded. So whether or not the flag is reconstructed cannot
   change the digested bytes.
2. **An already-clipped payload re-clips to itself.** `drain → to_owned`
   (drain.rs:153–161) copies `details: details.to_vec()` from the *decoded*
   record, which `decode_event` already bounded to `MAX_DETAILS`/`MAX_LOG_MSG`
   (events.rs:543–544, 617–618 reject `len > cap`). So `wire_payload` hands back
   bytes that are already ≤ cap, and `encode_event`'s
   `clipped = &details[..min(details.len(), MAX_DETAILS)]` is a no-op. The
   payload length and bytes are identical on record and replay.

This is genuinely correct. But it is **load-bearing and rests on two invariants
that future refactors could break** (digest range moving to include the header;
the cap moving below what `decode_event` accepts). There is no test exercising
it: the suite only digests `NameIntern`, `FrameMark`, `InjectQuery`, `Beacon`.

**Action:** add a test that drains an `AssertViolation` whose `details` is
exactly `MAX_DETAILS` bytes with `FLAG_TRUNCATED` set on the wire, asserts the
event surfaces with `truncated == true`, and asserts the `SDK_EVENT` digest +
len match a second host that drains the byte-identical ring. Also add a
non-UTF-8 `NameIntern.name` case (the intern table stores a *lossy* string —
channel.rs:275 — but `wire_payload` re-encodes the **raw** bytes, so the digest
is over the raw bytes and is fine; a test should pin that the lossy table path
and the raw-digest path don't accidentally converge). Cheap insurance for the
one thing in this file that the whole determinism story depends on.

---

### I-2. `inject_iseq` latch leaks across exits to a later unrelated `IN 0xD384`

The INJECT latch is an `Option<u32>` set on `OUT 0xD384` and `take()`-n on
`IN 0xD384` (detchannel.rs:208–215, 296–313). Consider a guest that does
`OUT 0xD384(iseq=5)` inside one exit but never issues the matching `IN` (it
faulted, was killed, or the SDK has a bug). The latch stays `Some(5)`. A
*later, unrelated* `IN 0xD384` — possibly many instructions later, possibly
after an unrelated `OUT 0xD384` was never issued — will consume `iseq=5` and
answer for it:

```rust
let Some(iseq) = self.inject_iseq.take() else { ... no-latch path ... };
let Some(channel) = self.channel.as_mut() else { ... };
self.responder.answer(channel, iseq, &mut sink)   // answers for the STALE iseq
```

If iseq 5's `InjectQuery` is still in `pending_injects` (it was drained but never
answered), `responder.answer` will match it and return a *fault decision* to a
guest read that the guest did not pair with that query. This is **deterministic**
(same on record and replay, so not a divergence bug) but it is a **correctness /
spec-conformance** hazard: the ABI's sequencing rule (inject.rs:23–25, API.md §5)
is "the SDK release-stores the producer index *before* `OUT 0xD384`, then issues
`IN 0xD384`". A bare `IN` with a stale latch from a prior exit is outside that
contract.

The `OUT,OUT,IN` (second OUT overwrites) and `OUT,IN,IN` (second IN hits the
no-latch path) cases the prompt asks about are both fine and deterministic. The
leak is specifically `OUT` (exit A) … `IN` (exit B, no intervening `OUT`).

**Action:** decide the intended semantics and either (a) clear `inject_iseq` at
the end of every exit that doesn't answer it (treat the latch as exit-scoped, the
truest reading of "drain inside this exit, answer with the next IN"), or (b)
document explicitly that the latch is boot-scoped and a cross-exit `IN` is a
defined (if degenerate) path. A 2-line test would pin whichever is chosen.

---

### I-3. `IDENT_ANSWER` duplicates `detguest_wire::ports::IDENT_VALUE` — two sources of truth for an ABI constant

detchannel.rs:44 defines:

```rust
pub const IDENT_ANSWER: u32 = 0xD37E_0001;
```

The wire crate already owns this exact value (ports.rs:28):

```rust
pub const IDENT_VALUE: u32 = 0xD37E_0001;
```

The module's own doc-comment header insists the ABI "is OWNED BY guest-sdk …
and never restated here normatively" — yet this is a normative ABI value
restated. If guest-sdk ever bumps the proto version, the wire constant changes
and this copy silently does not, and `pio_in(PORT_IDENT)` answers a stale magic
that the guest's `IN 0xD370` validation rejects — a hard-to-spot attach failure.
`ports.rs` already imports from the same crate (the file imports `InitStatus`,
`PORT_*` from `detguest_wire::ports`).

**Action:** re-export or alias the wire constant —
`pub use detguest_wire::ports::IDENT_VALUE as IDENT_ANSWER;` (or just use
`IDENT_VALUE` directly in `pio_in`) — and delete the literal. Same single-source
principle the module already follows for `PORT_*` and `InitStatus`.

---

### I-4. Host-only state not yet covered by any snapshot/restore — flag the gap before the snapshot bead builds on it

The prompt asks whether anything here is *actively wrong* vs merely deferred.
Conclusion: **deferred, not wrong** — but the gap is broader than the struct's
own latches and is worth pinning now because the snapshot bead will inherit it.

Fields that must be checkpointed for a correct restore but have no save/restore
path yet:

- `init_lo`, `init_hi`, `init_status`, `inject_iseq`, `last_quiesce_ack` — pure
  host state, trivially serializable, currently nowhere.
- `channel_gpa`, `manifest` — `channel_gpa` is needed to re-attach;
  the doc-comment (detchannel.rs:95–96) already says "INTEGRATION.md restore
  re-attaches at this GPA", so the intent is captured.
- **The non-obvious one:** `Channel::producer_seqs()` (channel.rs:206–217). The
  guest-sdk authors flagged this explicitly (channel.rs:96–101): ring C/I
  producer seqs are **not reconstructible** from the drained stream — the host
  is the producer and never drains those rings. `DetChannelHost` exposes
  `channel()` read-only but provides **no** `restore_producer_seqs` passthrough.
  After a snapshot restore, the first `push_command`/`push_workload_ctrl` would
  re-emit `seq = 0`, colliding with already-published records. This is the one
  piece a future snapshot author could *miss* because it lives inside the opaque
  `Channel`, not on `DetChannelHost`.

Nothing here is wrong in this iteration (no restore path exists to be wrong).
**Action:** file a follow-up bead (or note on the snapshot bead) enumerating
exactly these fields, and call out the producer-seqs passthrough specifically —
expose `pub fn restore_producer_seqs(&mut self, ProducerSeqs)` and
`pub fn producer_seqs(&self) -> Option<ProducerSeqs>` on `DetChannelHost` so the
snapshot layer can reach it without breaking the "mutation only through logged
methods" invariant.
