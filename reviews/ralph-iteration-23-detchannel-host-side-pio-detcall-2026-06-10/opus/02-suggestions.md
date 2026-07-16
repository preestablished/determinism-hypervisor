# Suggestions (non-blocking)

## 1. Pin / fingerprint the detguest-wire encoder used for SDK_EVENT digests (forward-compat)

**File:** `crates/dh-devices/src/detchannel.rs:481-507` (`sdk_event_digest`)

The SDK_EVENT digest is computed over a *re-encoding* of the decoded payload via
`detguest_wire::events::encode_event`, not over the original ring bytes (which the drain
consumes). This is deterministic *within a single build*, but it makes the AUX digest a
function of the **HEAD-wins** `detguest-wire` encoder rather than of the bytes the guest
actually wrote. If `detguest-wire`'s encoding for any variant changes between the recording
build and a later verification/replay build (field order, a new optional field, padding),
verification would report a spurious SDK_EVENT mismatch even though guest behavior is
unchanged. The comment at detchannel.rs:483-488 acknowledges the re-encoding but not the
cross-build risk.

Two cheaper mitigations than "digest the raw bytes" (which would require threading the
pre-decode bytes out of `drain_ring`, an upstream change):
- Record the `detguest-wire` crate version / a wire-format fingerprint into the segment
  header or a one-time AUX record, so a verifier can detect an encoder skew and downgrade
  the SDK_EVENT compare to a warning instead of a divergence.
- File a bead noting that the canonical AUX digest is coupled to the wire encoder and that
  any wire-format bump is a DHILOG-v1 compatibility event.

Suggested follow-up: a `bd create` capturing this coupling so it is not lost.

## 2. Make the drain-failure metric reachable by run control (escalation path)

**File:** `crates/dh-devices/src/detchannel.rs:323-332`

On `drain_events` error the handler increments `metrics.drain_failures` and returns an
empty `Vec`. The comment correctly says "the caller escalates via metrics (FAULTED is run
control's call)" — but `DetChannelMetrics` is exposed only as the public `metrics` field,
and there is no return signal on the `pio_out`/`drain_at_pause` path telling the caller a
drain just failed *this exit*. A caller that only samples `metrics` periodically could let
the vCPU run on past a corrupt-ring exit before noticing. Consider either returning a small
status alongside the events (e.g. `Result<Vec<GuestEvent>, DrainFault>` or an enum) so the
VMM can fault the slot at the exact boundary, or document at the call site that the VMM
MUST compare `metrics.drain_failures` before/after every drain-bearing exit. This mirrors
the `DevCtx::log_fault` "MUST check after every dispatch" contract already in ctx.rs.

## 3. Add the missing-coverage tests

**File:** `crates/dh-devices/src/detchannel.rs` tests module

All present tests are correct; these would close real gaps:
- **Ring-A drain.** Every drain test publishes only ring W. A `put_ring_a` helper +
  a drain test asserting `events[0].ring == RingId::A` and that A is drained *before* W
  (ordering is load-bearing for seq ordering).
- **`drain_failures` metric.** Write a `prod − cons > size` (or a `BadLen`) ring W and
  assert `pio_out(DOORBELL, ...)` returns empty and bumps `drain_failures` — currently no
  test exercises the error arm at all.
- **`RegionRegister` / `RegionUpdate` SDK digest.** `event_kind`/`wire_payload` map these,
  but no test drains one; a regression in `RegionEvent` round-tripping would go unnoticed.
- **INIT after a *failed* attach then success.** `init_status_codes` does failed-then-OK
  via *size/alignment* rejections (which never call `attach`). Add a case where
  `Channel::attach` itself fails (e.g. bad-magic page) and a *subsequent* valid commit at
  a good GPA still attaches — this is the path that proves a failed `attach` leaves no
  residual `self.channel`.
- **Restore / re-attach producer-seq path.** `Channel::producer_seqs` /
  `restore_producer_seqs` exist precisely so a snapshot restore does not re-emit a used
  seq, but `DetChannelHost` exposes neither and no test covers re-attach. If restore
  support is out of scope for this iteration, file a bead; otherwise add a save/re-attach/
  restore round-trip asserting the next push uses the restored seq.
- **`inject_in_without_out` when *attached*.** The `inject_answer` no-latch branch
  (detchannel.rs:297) is only hit in `inject_flow` *after* attach via the take()=None
  path, but the channel-present-but-no-iseq branch and the no-channel branch
  (detchannel.rs:305) are not separately asserted.

## 4. SDK_EVENT `len` reports the padded re-encoded length, not the logical payload length

**File:** `crates/dh-devices/src/detchannel.rs:499-506`

`encode_event` returns `n = RECORD_HEADER_LEN + pad8(payload_len)` (record.rs:49-51) and
zero-fills the buffer first (events.rs:272), so `payload_bytes = &buf[RECORD_HEADER_LEN..n]`
includes the 0..7 trailing zero-pad bytes. Consequently the `len` field written into
SDK_EVENT (and the digest input) is the **8-byte-aligned** payload length, not the logical
field length API.md §3.3 nominally describes (`len: u32` "payload length"). This is
internally consistent (verifier recomputes the same way) and harmless for the digest, but
the `len` value is slightly surprising for any analytics consumer reading SDK_EVENT.len.
Either digest `&buf[RECORD_HEADER_LEN..RECORD_HEADER_LEN + logical_len]` (the un-padded
payload), or add a one-line comment that `len` is the padded record-payload length by
design. Low priority — pick whichever the §3.3 author intends.

## 5. `sdk_event_digest` buffer sizing reads slightly oddly

**File:** `crates/dh-devices/src/detchannel.rs:499`

`vec![0u8; MAX_RECORD_LEN.max(encoded_event_len(&payload))]` — `encoded_event_len` is
always `<= MAX_RECORD_LEN` for valid events (the encoder debug-asserts it, events.rs:261),
so the `.max(...)` only ever yields `MAX_RECORD_LEN`. It is harmless (and defensive), but a
plain `vec![0u8; MAX_RECORD_LEN]` is clearer, or drop the `MAX_RECORD_LEN` term and trust
`encoded_event_len` (then the buffer is exactly right and `encode_event`'s
`BufferTooSmall` path becomes truly unreachable). Cosmetic.

## 6. Document the `pio_in(INJECT)` early-return invariant near `pio_answer`

**File:** `crates/dh-devices/src/detchannel.rs:239-244` and `383-385`

The "do not double-log" coupling is split across two sites: `pio_in` returns early for
`PORT_INJECT` because the responder logs through `CtxSink::pio_answer`, and the no-latch /
no-channel branches in `inject_answer` log `pio_answer` *themselves*. This is correct, but
the invariant "exactly one PIO_ANSWER per INJECT IN, logged by exactly one of {responder,
inject_answer}" lives only in prose. A single assertion-style comment at the `CtxSink::
pio_answer` impl ("the only PIO_ANSWER emitter for INJECT; the IDENT/INIT/RAZ path logs via
DevCtx::log_pio_answer directly") would make the two-emitter design auditable. The
`inject_flow` test's `record_count == before + 4` already guards it at runtime — good.
