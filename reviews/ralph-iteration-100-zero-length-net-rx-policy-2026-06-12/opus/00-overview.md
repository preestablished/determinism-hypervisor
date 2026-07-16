# Review Overview — zero-length NET_RX policy

- **Branch:** `ralph/iteration-100-zero-length-net-rx-policy` vs `main`
- **Date:** 2026-06-12
- **Reviewer:** Claude Opus
- **Commit:** `e61dd63` — ralph: iteration 100 checkpoint - forbid zero-length NET_RX at the codec (206)
- **Stats:** 6 files, +74/-7, 1 commit

## Summary

Bead 206 (raised at the iteration-85 opus1 review, item I2) noted an
asymmetry: the DHILOG codec accepted a zero-length `NET_RX` record, but the
device layer (`PvNet::apply_net_rx`) rejects `len == 0` (`NetRxError::FrameTooBig`).
A recorded empty NET_RX would therefore be unreplayable. This change closes
that gap by **forbidding empty frames at the codec** — the decision recorded
in the bead was forbid-at-codec rather than inventing empty-delivery semantics
at the device.

The diff:

- **Writer** (`dhilog.rs`): new `WriteError::EmptyNetRx`; `net_rx()` rejects
  `frame.is_empty()` before the existing length-cap check. New unit test
  `net_rx_frame_bounds_at_the_writer` covers empty / 1 / 2048 / 2049.
- **Reader** (`reader.rs`): `validate_kind` for `KIND_NET_RX` tightened from
  `len <= 2048` to `(1..=2048).contains(&len)`.
- **Test** (`reader_validation.rs`): `net_rx_frame_boundaries` — the
  zero-length case flipped from accepted to rejected (`BadPayloadLayout`), a
  1-byte accepted case added.
- **Device** (`net.rs`): comment-only — documents the now-three-layer
  agreement that empty frames don't exist.
- **API.md §3.3**: `0x03` row amended from `≤ 2048` to `1–2048; zero-length is
  INVALID`.
- **Ledger**: upstream-divergences entry #19.

## Verification performed

1. **Completeness** — confirmed. The only writer caller of `net_rx` is
   `recording.rs:203`, and it is reached **only after** `apply_net_rx`
   (`recording.rs:201`) has already rejected `len == 0` at the device. The TX
   doorbell (`net.rs:105`) faults `tx_len == 0`, so loopback can't synthesize
   an empty. The replay path (`replay_engine.rs:284`) calls `apply_net_rx`,
   which rejects 0. All producers/consumers are consistent; the new writer
   check is correct defense-in-depth.
2. **Format-freeze discipline** — no violation. The golden v1 fixtures are
   untouched (kitchen-sink NET_RX is 5 bytes, `golden.rs:84`), the BLAKE3 pins
   are unchanged, and the golden test passes. A format-version bump is
   genuinely not required: this rejects a degenerate record the writer never
   produced, not a re-layout of valid bytes.
3. **Ledger #19** — format matches #1–#18 (`### #N`, **Found**/**Why**, `Old`
   / `New` quote blocks). The `Old` quote is verbatim against
   `git show main:.agents/docs/determinism-hypervisor/API.md`.
4. **Error-variant hygiene** — `WriteError` derives `PartialEq, Eq` (line 100),
   so the `==` test comparisons are valid. No exhaustive `match` on `WriteError`
   exists anywhere (all uses are `{e:?}` Debug or specific-variant compares);
   the workspace builds clean.
5. **Test quality** — writer test covers empty/1/2048/2049; reader test
   asserts the correct `ReadError::BadPayloadLayout { kind, seq }` shape and
   adds the 1-byte lower-bound case.

## Verdict

**APPROVE**

The change is correct, minimal, well-documented, and internally consistent
across all three layers. The full workspace builds and the relevant test
suites (golden, reader_validation, dhilog lib) all pass. No blocking or
important issues found.
