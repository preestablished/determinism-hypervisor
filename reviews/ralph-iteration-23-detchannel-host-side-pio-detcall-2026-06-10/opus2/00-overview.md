# detchannel host-side PIO/detcall — second-reviewer overview

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-23-detchannel-host-side-pio-detcall` vs `main`
- **Bead:** determinism-hypervisor-nln
- **Change:** new `crates/dh-devices/src/detchannel.rs` (915 lines incl. tests), `DevCtx::log_sdk_event`, `detguest-host`/`-wire` promoted to regular deps, `lib.rs` re-exports.

## Summary

`DetChannelHost` implements the guest-sdk detcall PIO ABI (ports 0xD370–0xD39F):
IDENT, INIT latch/commit (size + 2 MiB alignment + attach + manifest snapshot),
DOORBELL/pause drains, INJECT (OUT latches iseq + drains ring W; IN answers via
`InjectResponder`), QUIESCE_ACK latch, and RAZ/WI for the rest of the window.
A `CtxSink` bridges `detguest_host::ChannelWriteSink` onto the `DevCtx` DHILOG
log wrappers so every host mutation of channel memory (`RING_PUSH`, `CONS_BUMP`,
`PIO_ANSWER`) and every drained event (`SDK_EVENT` digest) becomes a canonical
record at the exit's icount.

I focused on replay-divergence hazards and verified every claim against the
consumed guest-sdk sources (`detguest-host/{drain,inject,channel,manifest}.rs`,
`detguest-wire/{events,record,ports}.rs`) and the DHILOG framing
(`dh-inputlog/src/dhilog.rs`). **The headline replay-divergence worry — that a
truncated event re-encodes to a different SDK_EVENT digest — does not hold:**
`encode_event` puts `FLAG_TRUNCATED` in the *record header*, and the digest is
taken over `buf[RECORD_HEADER_LEN..n]` (payload only), so the dropped flag never
enters the digested bytes, and an already-clipped payload re-clips to itself.
The digest is record/replay-identical for every in-cap field. This is correct
but **load-bearing and undertested** — there is no test that drives a truncated
or non-UTF-8 payload through `sdk_event_digest`.

The genuinely actionable items are smaller: a duplicated IDENT constant that
already exists canonically in `detguest-wire` (`ports::IDENT_VALUE`), an
`inject_iseq` latch that leaks across exits to an unrelated later `IN`, and a
set of host-only state fields (`init_lo/hi`, `init_status`, `inject_iseq`,
`last_quiesce_ack`, and the channel's non-reconstructible producer seqs) that
are not yet covered by any snapshot/restore — deferred to the snapshot bead, but
worth pinning down before that bead lands.

## Verdict

**Approve with minor changes.** No Critical defects. The module is correct,
well-documented, and the determinism reasoning holds up under scrutiny. Address
the duplicated IDENT constant and the inject-latch-leak semantics (or document
them as intended), and add the missing truncated/non-UTF-8 digest test, before
the follow-on snapshot work depends on this surface.

## Stats

| Category | Count |
|---|---|
| Critical | 0 |
| Important | 4 |
| Suggestions | 7 |
| Positive notes | 6 |

- Tests: `cargo test -p dh-devices --lib detchannel` → **11 passed** (prompt said 9; two extra).
- Clippy: `cargo clippy -p dh-devices --all-targets` → **clean**.
- Deny-list gate: `no_host_ambient_authority` → **passes** (new file names no host APIs).
