# Critical And Important

No blocking findings.

The acceptance checks cover the requested proof points:

- Canonical `NET_RX`: the recording parses exactly one canonical `RecordBody::NetRx`, checks its bytes against `nanokernel::net_loopback_frame()`, and replay reports `records_applied == 1`.
- AUX `NET_TX`: the recording parses exactly one AUX `RecordBody::NetTx`, checks length, checks `LogWriter::digest8(expected_frame)`, and checks the `NET_RX` landing is one icount after TX.
- Epoch hashes: the recording requires the epoch-hash flag and at least one epoch record; replay verifies the same count.
- Byte-identical reseal: replay returns `resealed`, and the test asserts `replay.resealed == rec.log` after also checking end icount and end state hash.
- Guest-visible replay effect: after replay, TX and RX guest RAM contain the expected loopback frame.

I did not find an edge case where the test can pass while skipping the canonical `NET_RX` replay path. If `NET_RX` were not applied during replay, the guest tail would not reproduce the recorded end state and resealed log.
