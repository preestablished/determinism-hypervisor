# Positive Notes

- `DetChannelDevice` is a narrow adapter: it serves bus magic/version through the existing `DetDevice` convention, leaves live detchannel behavior on the PIO path, and makes device-specific MMIO RAZ/WI. That keeps the new snapshot seam from inventing a second live ABI.

- The restore path correctly re-attaches after RAM load and restores the non-reconstructible C/I producer seqs after `Channel::attach`. That matches the guest-sdk `Channel` contract and avoids replaying host-to-guest ring sequence numbers from zero.

- The fresh restore-plan factory is the right direction for reused slots. Resetting `InjectResponder` on EVTC restore avoids leaking occurrence counters from a previous segment into replay decisions.

- The branch adds useful integration coverage: the new restore-engine test exercises `take_snapshot` plus `restore_snapshot` with an EVTC-carrying bus, not just direct unit-level host roundtrip.

- The existing DHSNAP tag table already maps device id `0x0001` to `EVTC`, so the adapter composes with the current snapshot/restore section machinery without additional container changes.

- Focused and broader checks passed locally:
  - `cargo test -p dh-devices`
  - `cargo test -p dh-worker --test restore_engine -- --nocapture`
  - `cargo test -p dh-worker --test fork_engine -- --nocapture`
