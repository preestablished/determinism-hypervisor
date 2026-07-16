# Action Items

### Critical

_None._

### Important

_None._

### Suggestions

1. **File a bead for the HEAD-wins SDK_EVENT digest coupling.** The AUX SDK_EVENT digest is
   computed over a re-encoding via `detguest_wire::events::encode_event`
   (`crates/dh-devices/src/detchannel.rs:489-507`), so any change to that encoder between a
   recording build and a verification/replay build can produce a spurious divergence.
   Mitigation: record a `detguest-wire` version / wire-format fingerprint (segment header or
   one-time AUX record) so a verifier can detect encoder skew, and treat any wire-format
   bump as a DHILOG-v1 compatibility event. Self-contained: no code change required in this
   PR, just the tracking bead.

2. **Decide and document the drain-failure escalation contract.** On `WireError`,
   `drain` (`crates/dh-devices/src/detchannel.rs:323-332`) increments
   `metrics.drain_failures` and returns an empty `Vec` with no per-exit signal. Either
   return a drain-fault status the VMM can act on at the exact boundary, or add a call-site
   comment that the VMM MUST compare `metrics.drain_failures` before/after every
   drain-bearing exit (mirroring the existing `DevCtx::log_fault` "MUST check after every
   dispatch" rule in `crates/dh-devices/src/ctx.rs`).

3. **Add the missing-coverage tests** in `crates/dh-devices/src/detchannel.rs` tests:
   (a) a ring-A drain asserting `ring == RingId::A` and A-before-W ordering;
   (b) a `drain_failures` test with a corrupt `prod − cons > size` ring (the error arm is
   currently untested);
   (c) a `RegionRegister`/`RegionUpdate` SDK-digest drain;
   (d) a failed `Channel::attach` (bad-magic page) followed by a valid commit at a good GPA,
   proving no residual `self.channel` after a failed attach;
   (e) a restore / re-attach producer-seq round-trip via `Channel::producer_seqs` /
   `restore_producer_seqs` (or a bead if restore is out of scope for this iteration).

4. **Clarify the SDK_EVENT `len` semantics** at
   `crates/dh-devices/src/detchannel.rs:499-506`: `len` (and the digest input) currently
   covers the 8-byte-aligned, zero-padded re-encoded payload, not the logical field length.
   Either digest the un-padded payload (`..RECORD_HEADER_LEN + logical_len`) or add a
   one-line comment that `len` is the padded record-payload length by design.

5. **Simplify the digest buffer sizing** at `crates/dh-devices/src/detchannel.rs:499`:
   `MAX_RECORD_LEN.max(encoded_event_len(...))` always yields `MAX_RECORD_LEN`; use a plain
   `vec![0u8; MAX_RECORD_LEN]` (or size exactly to `encoded_event_len`). Cosmetic.

6. **Document the single-PIO_ANSWER-per-INJECT invariant** with a comment at
   `crates/dh-devices/src/detchannel.rs:383-385` (`CtxSink::pio_answer`) noting it is the
   sole emitter for the INJECT path while IDENT/INIT/RAZ answers log via
   `DevCtx::log_pio_answer` directly — making the two-emitter design auditable beyond the
   runtime `record_count` guard in `inject_flow`.
