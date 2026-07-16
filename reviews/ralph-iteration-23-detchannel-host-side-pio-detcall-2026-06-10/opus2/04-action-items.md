# Action items

Self-contained checklist. File paths are absolute; line numbers are against the
review branch `ralph/iteration-23-detchannel-host-side-pio-detcall`.

### Critical

_None._

### Important

- [ ] **I-3 — Remove the duplicated IDENT constant.** In
  `/home/infra-admin/git/preestablished/determinism-hypervisor/crates/dh-devices/src/detchannel.rs:44`,
  delete `pub const IDENT_ANSWER: u32 = 0xD37E_0001;` and replace with
  `pub use detguest_wire::ports::IDENT_VALUE as IDENT_ANSWER;` (or use
  `IDENT_VALUE` directly in `pio_in`). Both currently equal `0xD37E_0001`; the
  literal is a second source of truth for an ABI value guest-sdk owns. (Cheapest
  fix; do this first.)

- [ ] **I-2 — Decide and pin the `inject_iseq` cross-exit latch semantics.** In
  `detchannel.rs:208–215` / `296–313`, an `OUT 0xD384` with no matching `IN` in
  the same exit leaks the latch to a *later unrelated* `IN 0xD384`, which then
  answers a fault decision for a query the guest did not pair with that read.
  Either (a) clear `inject_iseq` at the end of each exit that doesn't answer it
  (exit-scoped), or (b) document it as boot-scoped-and-intended. Add a test for
  the chosen behaviour: `OUT INJECT` in one `with_ctx`, then a bare `IN INJECT`
  in a later `with_ctx`, asserting the intended answer + metric.

- [ ] **I-1 — Add truncated / non-UTF-8 digest coverage.** Add a test that drains
  an `AssertViolation` with `details` of exactly `MAX_DETAILS` bytes and
  `FLAG_TRUNCATED` set on the wire; assert the surfaced `GuestEvent.truncated ==
  true` and that the `SDK_EVENT` digest+len equal a second host draining the
  byte-identical ring. Add a non-UTF-8 `NameIntern.name` case to pin that the
  raw-bytes digest path (`wire_payload`) and the lossy intern-table path do not
  diverge. This protects the one invariant the whole determinism story rests on
  (`sdk_event_digest`, detchannel.rs:489–507).

- [ ] **I-4 — File the snapshot-coverage follow-up.** Open a bead (or annotate the
  snapshot bead) enumerating the host-only fields with no save/restore path:
  `init_lo`, `init_hi`, `init_status`, `inject_iseq`, `last_quiesce_ack`,
  `channel_gpa`, `manifest`, **and especially** the channel's non-reconstructible
  ring C/I producer seqs (`Channel::producer_seqs` / `restore_producer_seqs`,
  channel.rs:206–217). Expose a passthrough on `DetChannelHost` for the producer
  seqs so the snapshot layer can restore them without breaking the read-only
  `channel()` invariant. Nothing is wrong this iteration (no restore path exists
  yet); this prevents the snapshot author from missing the seqs buried in
  `Channel`.

### Suggestions

- [ ] **S-1** — Collapse `event_kind` + `wire_payload` (detchannel.rs:389–479)
  into one match returning `Option<(EventKind, EventPayload)>` so the
  `non_exhaustive` wildcard appears once and the two stay structurally
  consistent.
- [ ] **S-2** — Document on `sdk_event_digest` that the wildcard's record/replay
  consistency assumes record and replay run the same build (no mixed-build
  replay across a guest-sdk variant addition).
- [ ] **S-3** — Preserve the specific `WireError` on drain failure
  (detchannel.rs:323–331) — per-variant counters or `last_drain_error` — instead
  of one opaque `drain_failures` count.
- [ ] **S-4** — Add ring-A coverage: a `put_ring_a` helper +
  `pio_out(PORT_DOORBELL, DOORBELL_RING_A)` test exercising
  `ring_id_byte(RingId::A) == 2` (no ring-A test exists today).
- [ ] **S-5** — Replace the open-coded DHILOG framing in the test `records()`
  helper (detchannel.rs:604–616) with a shared `dh_inputlog` test-reader so the
  256/24/8 constants have one definition.
- [ ] **S-6** — Add a `Push(RingFull)` test (fill ring C, assert the variant);
  only `NotAttached` is covered today, yet the doc promises a RingFull-retry
  contract.
- [ ] **S-7** — Document or refresh the attach-time `manifest()` snapshot
  (detchannel.rs:281–284, 98) — it goes stale after later guest region
  registrations; callers needing live resolution should use
  `channel().read_manifest()`.
