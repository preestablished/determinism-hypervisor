# Positive notes

## Backward-compat was actually de-risked, not just asserted

The commit's "no writer ever produced one" claim is stronger than it first reads, and the diff
holds up under the harder version of the question — *the writer COULD produce an empty pre-change
(`net_rx(&[])` succeeded), so the relevant risk is persisted bytes, not the writer guard*. I
checked the only place persisted v1 logs live in-repo: `tests/fixtures/v1_kitchen_sink.dhilog`
carries a NET_RX of `[0xAA,0xBB,0xCC,0xDD,0xEE]` (5 bytes, asserted at `golden.rs:255`) and
`v1_minimal.dhilog` carries none. Both still parse and every `dh-inputlog` test passes. The single
real caller, `dh-vmm::recording::apply_net_rx`, is fed from TX-doorbell drains that fault on
`tx_len == 0`, so a len-0 frame was never reachable in production recording. The no-version-bump
decision is therefore well-founded: the tightened bytes were never actually producible by the
recording rail, only by a synthetic direct call that would already have minted an unreplayable log.

## The reader and writer guards are symmetric and minimal

The reader change is a clean range check (`(1..=MAX_NET_RX_FRAME).contains(&payload.len())`) rather
than a bolted-on `!= 0` special case, and the writer's early `frame.is_empty()` return with a
dedicated `EmptyNetRx` variant keeps the two oversize/undersize errors distinct
(`PayloadTooLong` vs `EmptyNetRx`) — good for diagnosis. The test edits mirror this precisely: the
old "accepted by design" assertion is *replaced* (not merely deleted) with both a rejection
assertion and a new 1-byte-accepted assertion, so the boundary is pinned from both sides.

## Splice consistency falls out for free — and is genuinely consistent

`Lineage::new` and `Lineage::extend` (`splice.rs:73, 104`) both route every segment through
`LogReader::parse`, so the tightened validation propagates to the lineage layer automatically: a
segment containing an empty NET_RX would now fail with `SpliceError::Segment { err:
BadPayloadLayout, .. }`, indexed to the offending segment. That is the *right* failure mode (loud,
located) and requires no splice-layer change — the module's "every segment parses as a SEALED v1
log" contract already carries the new rule.

## The divergence ledger entry is honest and complete

Ledger #19 lives in the correct section ("Divergences with a local amendment") because this commit
made a real local edit to the vendored `API.md`. It records both the old (`≤ 2048`) and new
(`1–2048; zero-length is INVALID`) table rows verbatim, names the code authority files, and is
candid that upstream "deliberately accepted zero-length frames" — i.e. it documents an intentional
override of upstream rather than papering over it. Numbering #19 after #18 is consistent with the
established append-only convention even though #11–#18 sit in a different section; the entries are
globally sequenced by discovery order, not per-section.

## The added `NetRxError` doc-comment is the right kind of cross-layer breadcrumb

The new doc on `NetRxError` (`net.rs:61-64`) ties the device's `FrameTooBig` to the codec policy
and names bead 206 — exactly the sort of "why do all three layers agree" note a future maintainer
needs. (Its only flaw is that the *other* comment 90 lines down was not brought into agreement —
see I1.)
