# Suggestions

Non-blocking suggestions:

- Consider tightening the epoch assertion later if the exact count proves stable enough on the supported KVM lane. The current `> 0` plus replay count equality proves epoch verification is active without making the test brittle.
- If replay surfaces serial output in a future API, checking replayed `TRX` would make the guest-side success signal explicit. The current end-state hash, reseal, and RX RAM checks already cover the important replay behavior.
- The test runtime is noticeable for a single integration test on this host. If the broader `dh-worker` suite becomes too slow on KVM runners, this test may be a candidate for a nightly/hardware lane rather than every local hardware run.
- The raw `reader.end().0 == 2` assertion is understandable because the log exposes the stop-reason byte. A local named constant would make the intent slightly clearer if this pattern spreads.
