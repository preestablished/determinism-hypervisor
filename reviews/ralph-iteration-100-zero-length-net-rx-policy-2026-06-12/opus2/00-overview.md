# Review: Zero-length NET_RX policy (forbid at the codec)

- **Branch:** `ralph/iteration-100-zero-length-net-rx-policy` vs `main`
- **Date:** 2026-06-12
- **Reviewer:** Claude Opus (2nd reviewer)
- **Stats:** 6 files, +74/-7, 1 commit (`e61dd63`)
- **Bead:** 206 — Cross-layer zero-length NET_RX policy

## Summary

The change closes a cross-layer asymmetry flagged in iteration 85 (opus1 I2): `dh-inputlog`'s
codec accepted zero-length NET_RX records while `PvNet::apply_net_rx` rejects `len == 0`
(`FrameTooBig`), so a recorded empty NET_RX would have been unreplayable. Bead 206 chose
*forbid-at-codec* over *invent-empty-delivery-semantics*:

- **Writer** (`dhilog.rs`): new `WriteError::EmptyNetRx`; `net_rx` returns it for `frame.is_empty()`.
- **Reader** (`reader.rs`): `validate_kind` for `KIND_NET_RX` tightened from `len <= 2048` to
  `(1..=2048).contains(&len)`.
- **Device** (`net.rs`): unchanged behaviour; a doc-comment was added to `NetRxError` noting the
  three layers now agree.
- **Spec** (`API.md §3.3`): `0x03` row amended to `1–2048; zero-length is INVALID`.
- **Ledger** (`upstream-divergences.md`): divergence #19 added with the old/new API.md text.
- **Tests:** writer-bounds test added; reader test flipped (empty now rejected, 1 byte accepted).

The implementation is correct, well-tested, and the chosen direction (forbid at codec) is the
right one — empty delivery has no meaning for a loopback NIC whose only RX source is a TX
doorbell that already faults on `tx_len == 0`. The divergence ledger entry is honest about the
upstream wording it overrides.

## Verdict

**Approve with one Important follow-up.** No correctness defects. One Important maintainability
issue: the change implements bead 206 but leaves behind a now-false comment in
`net.rs` (lines 154–157) that asserts the codec *accepts* empty NET_RX and that "the policy is
its own bead… until it lands" — the bead has now landed, and the comment directly contradicts
the new invariant. Two non-blocking suggestions (a cross-crate const-pin for the frame cap, and
a forward-looking note on the `lyu` inspection bead).

## Probes run

- **Backward compat:** verified the only persisted v1 log carrying a NET_RX
  (`tests/fixtures/v1_kitchen_sink.dhilog`) uses a 5-byte frame — it still parses; all 50
  `dh-inputlog` tests green. No-version-bump verdict: **defensible** (reasoning in 02).
- **Splice:** confirmed `Lineage::{new,extend}` re-run `LogReader::parse` per segment, so a
  hypothetical empty-NET_RX segment would now fail at splice level too — consistent, not surprising.
- **Pin test:** confirmed `dh-devices` already depends on `dh-inputlog` but `MAX_FRAME` is a bare
  `2048` with only a prose "mirrors" claim — no compile-time pin (suggestion in 02).
- **Fuzz:** target only drives `parse`; corpus is opaque hashed blobs, no NET_RX seed in source —
  the tighter lower bound only shrinks the accepted set, so no corpus breakage possible.
- **Ledger #19 placement:** correct section ("Divergences with a local amendment") given the local
  API.md edit; numbering after #18 is consistent with established practice (see 03).
