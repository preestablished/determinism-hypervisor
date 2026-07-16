# Positive Notes

### P-1 — Genuinely atomic restore (assign-after-validate)

`restore()` performs *all* validation and fallible work — version/length gate,
`Channel::attach(...)?`, `read_manifest()`, seq decode — before mutating a single
`self.*` field. The `attach` failure returns early via `?` with `self` untouched,
so a failed restore leaves the host exactly as it was. This is the correct shape for
a snapshot restore and the diff implements it without the common partial-mutation
trap. The local bindings (`init_lo`, `channel`, `manifest`, …) followed by one
assignment block make the atomicity obvious to a reader.

### P-2 — Internally consistent, fixed-length wire layout

The writer and reader were walked byte-for-byte and agree exactly: flags at 12/17/22,
u32s at 13/18, gpa at 23..31, seqs at 31/35, `EVTC_LEN = 4+4+4+5+5+1+16 = 39`. The
detached case still writes the full 16 trailing bytes (`[0u8; 16]`) so the section is
always exactly `EVTC_LEN` — the length gate is a meaningful invariant, not an
approximation.

### P-3 — Faithful to the normative reconstructible-vs-not contract

The change correctly identifies the ring C/I producer seqs as the *one* piece of
non-reconstructible host state (guest-sdk `channel.rs`: "the host is the producer
there and never drains those rings") and serializes exactly those via
`producer_seqs()` / `restore_producer_seqs()`. It correctly *excludes* the intern and
pending-inject caches (orchestrator-reconstructible) and the manifest (guest RAM,
re-read at attach per ARCHITECTURE §8.3 step ordering). The split matches the spec
rather than over- or under-serializing.

### P-4 — The seq-non-reuse test targets the actual hazard

`evtc_roundtrips_attached_state_and_seqs` does not merely assert
`producer_seqs() == before`; it then issues a *new* push post-restore and asserts
`ring_c == seqs_before.ring_c + 1`. That is precisely the failure
`restore_producer_seqs` exists to prevent (re-emitting an already-used seq), and the
test would catch a regression where the seqs were saved but not actually re-fed into
the channel. Strong, intent-revealing coverage.

### P-5 — Negative-path coverage is thorough

`evtc_roundtrips_detached_state_and_refuses_bad_input` covers the detached
roundtrip, wrong-version refusal, truncation refusal, and — best of the set — a
hand-corrupted attached section whose GPA points at zeroed RAM, asserting the
re-attach refuses loudly rather than attaching garbage. The bad-header case exercises
the real `Channel::attach` validation path (magic/proto/ring descriptors), not a
mock.
