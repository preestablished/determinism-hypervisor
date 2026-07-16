# Action Items

1. Add the negative reseal-equivalence test described in `01-critical-and-important.md` so RING_PUSH payload drift cannot be accidentally accepted before the classifier runs.

2. Add a brief comment or split helper around `detchannel_exit_generated_event` to make clear which generated detchannel records should be normalized, skipped during replay, or treated as classifier-equal.

3. Defer any `channel_mutation_drift` labeling for RING_PUSH until replay applies or explicitly compares the channel-memory effects, and include an effect-level regression test when that support lands.
