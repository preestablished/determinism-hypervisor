# Positive Notes

- The test exercises the full intended path: guest TX doorbell, AUX `NET_TX`, host loopback, canonical `NET_RX`, replay application, epoch verification, end hash verification, and byte-identical reseal.
- The deferred landing at the next icount boundary is a good shape for hash-chain semantics: the first quantum stops before applying the canonical input, then the next run observes the changed guest memory.
- The assertions catch duplicate TX doorbells, non-polling IRQ delivery, frame byte drift, missing epoch hashes, wrong record counts, wrong AUX digest, wrong replay record count, wrong end hash, and reseal drift.
- The test self-skips cleanly when `/dev/kvm` is unavailable.
