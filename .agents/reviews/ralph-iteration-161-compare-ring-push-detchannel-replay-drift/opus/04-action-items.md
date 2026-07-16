# Action Items

No blocking action items.

Recommended follow-up before adding the larger `RING_PUSH` mutation test:

- Add or extend replay coverage so a real generated `RING_PUSH` at a shifted
  `icount` is accepted only when the payload matches exactly.
- Keep `RING_PUSH` out of `channel_mutation_drift` until replay either applies
  the channel-memory mutation or compares the affected channel memory directly.
- Re-run at least `cargo test -p dh-worker reseal_ -- --nocapture` after the
  follow-up test lands.
