# Suggestions

### S-1. `event_kind` and `wire_payload` are two near-identical 14-arm matches that must stay in lockstep

`event_kind` (detchannel.rs:389–409) and `wire_payload` (412–479) each match all
14 `OwnedPayload` variants plus the `non_exhaustive` wildcard. They are only ever
called together (in `sdk_event_digest`, both `?`-propagating). Splitting one
mapping into two parallel matches means a future variant must be added in two
places, and a mismatch (one updated, one not) silently routes through the
`_ => None` wildcard → `sdk_digest_failures` metric instead of a compile error.
Consider collapsing into a single function that returns
`Option<(EventKind, EventPayload<'_>)>` so the `non_exhaustive` wildcard appears
once and the two are structurally guaranteed consistent.

### S-2. `non_exhaustive` wildcard behaviour across build skew — document the snapshot/replay implication

The wildcard arms return `None` → event still forwarded, `sdk_digest_failures`
incremented (detchannel.rs:405–407, 477). The prompt's build-skew question:
a snapshot recorded with an *older* build (variant unknown → counted as digest
failure, no SDK_EVENT record emitted) replayed with a *newer* build (variant
known → digest computed, SDK_EVENT record emitted) would produce **different
record streams**. Since the project's stated model is HEAD-wins rebuild of both
record and replay, this is consistent in practice. But that invariant is implicit
here. Add a one-line note to `sdk_event_digest`'s doc-comment that the wildcard's
record/replay consistency depends on record and replay running the *same* build
(no mixed-build replay across a guest-sdk variant addition). Cheap and it
documents a real constraint the code silently assumes.

### S-3. `drain_failures` swallows the specific `WireError` — losing forensic signal

`drain` (detchannel.rs:323–331) collapses every `drain_events` error into a bare
counter:

```rust
Err(_) => { self.metrics.drain_failures += 1; return Vec::new(); }
```

`WireError` distinguishes `CorruptIndices { ring }`, `Decode(BadLen)`,
`SeqlockLivelock`, etc. (drain.rs:235, 254, 262). Collapsing them loses the
ability to tell "guest published a corrupt producer index" from "wire framing
bug". Determinism is unaffected (the metric is deterministic either way), but
when this fires in the field the operator gets a count and no cause. Consider
either per-variant counters or stashing `last_drain_error: Option<WireError>`.

### S-4. `DOORBELL_RING_A` / `DOORBELL_RING_W` are defined but only used to test "any bit set"

detchannel.rs:52–53 define the two mask bits, but `pio_out`'s DOORBELL arm only
checks `value & (A | W) == 0` and then drains *both* rings unconditionally
(247–251). The individual bits are never used to drain selectively. That is a
defensible simplification (the comment explains it: drains are a superset and
always legal), but it means a guest that rings `DOORBELL_RING_A` when only ring W
has data still triggers a W drain — which is correct, just worth a test. There is
**no ring-A coverage anywhere in the suite** (all `put_ring_w`); a single
`put_ring_a` + `pio_out(PORT_DOORBELL, DOORBELL_RING_A)` test would exercise the
`ring_id_byte(RingId::A) == 2` path and confirm both-rings-drained-on-any-mask.

### S-5. Test `records()` parser hard-codes framing offsets — brittle against a DHILOG header/record-size change

The test helper (detchannel.rs:604–616) open-codes the DHILOG framing: `at = 256`
(HEADER_LEN), `24`-byte record header, `plen` at `[at+2..at+4]`, payload at
`[at+24..]`, 8-byte pad. I re-derived this from `dhilog.rs`: HEADER_LEN = 256
(dhilog.rs:27), record header = kind(1) + rflags(1) + len(2) + seq(4) +
icount(8) + rip(8) = 24 bytes (dhilog.rs:312–318), 8-byte pad (321–322). **The
parser is correct.** But it duplicates framing constants the inputlog crate owns;
if `dh-inputlog` adds a field to the record header, these tests parse garbage
silently rather than failing to compile. Consider exposing a minimal
`dh_inputlog::dhilog` test-reader (even `#[cfg(test)]`/`pub(crate)`) and using it
here, so the framing has one definition. Low priority — the constants are stable —
but it is a latent maintainability trap noted for the record.

### S-6. `PushChannelError` doc says `RingFull` "retries at the next pause" but nothing here implements or tests that loop

The type doc (detchannel.rs:82, 148–149) describes the retry contract
("`Push(RingFull)` is retried by the caller at the next pause, never spun on"),
but the retry lives entirely in the (not-yet-written) caller. No test asserts that
`push_command` actually returns `Push(RingFull)` on a full ring — only the
`NotAttached` path is covered (`drain_before_attach_is_empty_and_push_errors`).
A test that fills ring C and asserts the `RingFull` variant surfaces would lock
the contract the doc promises. (This depends on `Channel::push_command`'s
`PushError` surface; verify `RingFull` is reachable from the host side.)

### S-7. `manifest()` returns a snapshot taken only at attach — staleness is silent

`manifest` is read once at `channel_init` (detchannel.rs:281–284) and never
refreshed. The guest can re-register regions (bumping the manifest generation)
long after attach, and `manifest()` will return the attach-time snapshot. The
doc-comment (detchannel.rs:98) calls it a "snapshot from attach", which is
honest, but a caller reaching for region resolution via `manifest()` gets stale
data with no signal. Since `read_manifest` is cheap and seqlock-consistent
(manifest.rs:70), consider either a `refresh_manifest(&mut self)` method or
documenting at the call sites that resolution must go through a fresh
`channel().read_manifest()` rather than the cached snapshot. Not a determinism
issue (the snapshot is read-only and reproducible), purely an API-correctness
footgun.
