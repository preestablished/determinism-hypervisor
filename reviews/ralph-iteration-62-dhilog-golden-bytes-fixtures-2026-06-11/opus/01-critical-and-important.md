# Critical and Important Findings

**None.**

I went looking specifically for ways the format could drift without a test
failing, and for correctness bugs in `net_rx` and the fixture bytes. None hold up:

## Freeze discipline — verified airtight

The triple assertion is real protection, not theater:

1. **Hash pin** (`KITCHEN_SINK_BLAKE3` / `MINIMAL_BLAKE3`) — hardcoded constants
   compared against the checked-in bytes.
2. **Byte-identical re-serialization** — `build_kitchen_sink()` output must equal
   the fixture byte-for-byte.
3. **Structural parse** — `LogReader::parse` must succeed and decode to the
   expected header/kind sequence.

I simulated writer drift (changed `end_vns: 1500 → 1501` in the builder) and ran
both with and without regen:

- **Without regen (normal CI):** assertion (2) fails — `writer output drifted
  from the frozen v1.0 fixture`.
- **With `DHILOG_REGEN_GOLDEN=1` (the footgun — regen "to fix" a red test):**
  regen overwrites the on-disk fixtures with the drifted bytes, so assertion (2)
  now passes — BUT assertion (1) fails: `checked-in fixture changed — the v1.0
  freeze is violated`, printing the new vs. pinned hash. The hardcoded constant
  is the anchor that regen cannot launder past.

So the only way to land silent drift is to **regenerate AND edit the hardcoded
hash constant in the same PR** — a deliberate two-step act, not an accident. The
module doc explicitly forbids it ("If a test fails here, the WRITER drifted; fix
the writer, do not regenerate"). This is the right design. The residual risk
(careless reviewer waves through a PR that touches both the `.dhilog` binaries
and the hash constants) is real but is a process gap, addressed as a non-blocking
suggestion (CI grep guard) in `02-suggestions.md`, not an Important finding.

## net_rx — correct vs §3.3

`net_rx` (`dhilog.rs:186-196`) is correct:

- Payload IS the frame bytes, no preamble — `self.record(KIND_NET_RX, 0, ...,
  frame)` passes `frame` directly. ✓ (§3.3: "raw frame bytes")
- `rflags = 0` (canonical, not AUX). ✓
- Cap at `MAX_NET_RX_FRAME = 2048`, checked before `record()`. ✓ (§3.3:
  "payload_len ≤ 2048")

**Bound precedence is correct.** `net_rx` checks `> MAX_NET_RX_FRAME` (2048)
first; `record()` independently checks `> MAX_PAYLOAD` (4096). Since 2048 < 4096,
the tighter `net_rx` check always trips first for an oversized frame, so the
`record()` check is an unreachable backstop for this path — which is fine and
defensive. Both return `WriteError::PayloadTooLong`, so the error is identical
regardless of which fires. No ambiguity, no ordering bug.

## Fixture bytes — verified against API.md §3.1 / §3.2 / §3.3

Hand-decoded the kitchen-sink header at every offset: magic `DHILOG`, version
`0x0100` (LE `00 01`), header_len 256, flags `0x03` (SEALED|HAS_AUX),
base/end/seed/config hashes, clock 3/2, record_count 11, end_icount 1000,
end_vns 1500, end_state_hash 0x55×32, body_hash present (nonzero),
encoder_fingerprint `ef be fe ca ce fa ed fe` = 0xFEEDFACECAFEBEEF, reserved
[248..256) zeros. All match.

Walked all 11 records to EOF: kinds `01 01 02 02 02 03 40 41 43 45 7F`, seq
monotonic 0–10, icounts ascending 100…1000, padding correct (e.g. NET_RX
plen=5→pad=3, TIMER_FIRE plen=20→pad=4), END at rflags.AUX=1 / rip=0 / plen=40,
final offset 720 == file length. No stray bytes, no misalignment.
