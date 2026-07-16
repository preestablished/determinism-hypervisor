# Review: detchannel host side — PIO detcall handler + drains + DEV_EVENT logging

- **Branch:** `ralph/iteration-23-detchannel-host-side-pio-detcall` vs `main`
- **Beads issue:** determinism-hypervisor-nln
- **Date:** 2026-06-10
- **Reviewer:** Claude Opus
- **Scope:** `crates/dh-devices/src/detchannel.rs` (new, ~915 lines incl. tests),
  `crates/dh-devices/src/ctx.rs` (+`log_sdk_event`), `crates/dh-devices/src/lib.rs`
  (re-exports), `crates/dh-devices/Cargo.toml` (dev-deps → deps promotion).

## Summary

This change lands the host side of guest-sdk's detchannel: a `DetChannelHost<M, P>`
state machine driving the PIO "detcall" register window `0xD370–0xD39F`. It implements
IDENT, the CHANNEL_INIT GPA-latch / commit state machine, DOORBELL and pause-boundary
ring drains, the INJECT OUT-drain / IN-answer split, and QUIESCE_ACK latching. A private
`CtxSink` bridges `detguest_host::ChannelWriteSink` onto the DHILOG `DevCtx` log wrappers,
so every host mutation of channel memory (ring-C/I pushes, consumer-index bumps, and PIO
answers) becomes a canonical `DEV_EVENT` record at the exit icount. Drained guest events
are mirrored as AUX `SDK_EVENT` digests whose digest input is a canonical *re-encoding* of
the decoded payload via detguest-wire's own encoder.

I read the three normative sources (ARCHITECTURE §6.6, this repo's API.md §3.3, guest-sdk
API.md §5) and the consumed library (`detguest-host` channel/drain/inject/manifest +
`detguest-wire` ports/events/record) and the DHILOG encoder (`dh-inputlog/dhilog.rs`). The
implementation conforms to all three specs. Byte layouts of `RING_PUSH`, `CONS_BUMP`,
`PIO_ANSWER`, and `SDK_EVENT` match API.md §3.3 and the dhilog encoders exactly. Ring-id
mapping (`0=C,1=I,2=A,3=W`) is correct. Status-code mapping matches guest-sdk API.md §5.
The no-mutation-outside-the-sink invariant holds: `attach`/`read_manifest`/`drop_counters`
only *read* guest RAM, and every write path (drain `cons_bump`, push `ring_push`, inject
`pio_answer`) routes through `ChannelWriteSink` → `DevCtx::log_*`.

The design decisions flagged for scrutiny are all sound and well-justified in comments:
the pre-commit `u32::MAX` sentinel (deliberately outside the ABI's `0..=3` so a
pre-commit `IN 0xD37C` cannot read a stale OK), the doorbell mask superset-drain (legal —
rings are drained unconditionally at every pause anyway), the INJECT OUT-then-IN split
(matches the §5 sequencing rule — the query is on ring W before `OUT 0xD384`), and the
digest-by-re-encoding (deterministic across record/replay because the same code runs both
sides).

I found **no Critical or Important issues**. There are a handful of Suggestions, mostly
documentation / test-coverage hardening and one genuine forward-compat risk worth a
follow-up bead (the HEAD-wins encoder dependency for the SDK digest).

## Verdict

**APPROVE**

## Stats

- Files changed: 4 (+928 / −6)
- New production code: ~507 lines (detchannel.rs non-test) + 9 lines ctx.rs + re-exports
- New tests: 11 in-file unit tests (all passing: `cargo test -p dh-devices detchannel`)
- Critical: 0
- Important: 0
- Suggestions: 6
- Positive notes: 8
