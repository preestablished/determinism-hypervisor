# Action Items

- [ ] Replace the `0` boundary RIP in `apply_net_rx(rx_icount, 0, &frame)` with `rx_boundary.boundary.rip`.
- [ ] Assert the parsed canonical `NET_RX` record carries the captured boundary RIP.
- [ ] Re-run:

```text
cargo test -p dh-worker --test m5_net_loopback m5_net_rx_loopback_records_and_replays_bit_identically -- --nocapture
```
