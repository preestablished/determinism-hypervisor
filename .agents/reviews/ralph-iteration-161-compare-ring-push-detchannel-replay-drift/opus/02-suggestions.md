# Suggestions

- Consider splitting `detchannel_exit_generated_event` into two narrower helpers
  if this area keeps growing: one for replay application/skip decisions and one
  for reseal position normalization. They are intentionally aligned today, but
  `RING_PUSH` is host-to-guest channel memory mutation while `CONS_BUMP` is a
  guest-to-host drain side effect; separate names would make a future semantic
  split harder to miss.

- Add an end-to-end replay test when a real production path starts generating
  host-to-guest `RING_PUSH` records. The current tests cover synthetic reseal
  comparison and classification, but they do not prove that a live replay run
  actually regenerates equivalent ring pushes from the same control-plane input.

- The new classifier test varies the pushed record bytes. A small companion case
  varying `new_prod` would be a cheap guard that every `RING_PUSH` payload field,
  not just the trailing record bytes, remains a canonical mismatch.
