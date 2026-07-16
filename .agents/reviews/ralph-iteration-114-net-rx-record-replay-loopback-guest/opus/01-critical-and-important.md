# Critical And Important Findings

## I1. `NET_RX` is recorded with a placeholder boundary RIP

File: `crates/dh-worker/tests/m5_net_loopback.rs:206`

The test applies the canonical RX record with:

```rust
.apply_net_rx(rx_icount, 0, &frame)
```

That second argument is the canonical record's `boundary_rip`. In the rest of the record/replay tests, canonical inputs are recorded with the actual boundary RIP from the `SegmentOutcome`, for example pad input uses `out.boundary.rip`.

This matters because `replay_segment` reads `rec.boundary_rip()` and passes it straight back into `apply_net_rx` during replay. If the recording writes `0`, replay reseals `0` too, so byte-identical reseal cannot catch that the acceptance test failed to preserve the real landing metadata. The test still proves the frame bytes replay, but it weakens the canonical NET_RX record contract and does not fully exercise the recorded boundary identity.

Recommended fix: pass `rx_boundary.boundary.rip` into `apply_net_rx`, and add an assertion after parsing the DHILOG that the single `NET_RX` record carries that RIP. If zero is not a valid expected RIP for this guest boundary, assert it is non-zero as well.
