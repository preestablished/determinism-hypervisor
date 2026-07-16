# Suggestions

## Non-Blocking

- `tools/dh-cli/src/linux.rs:268` Consider including `ready_payload_len` in the Linux gate fingerprint. The current fingerprint includes `ready_event_kind`, `icount`, `vns`, `state_hash`, `config_hash`, `game_image_hash`, and `base_image_hash`, which is enough for bead `4s9.22` because `run_to_ready` only stops after Ready EventKind 14. Adding the payload length would make the gate artifact more diagnostic and align better with the later M9 gates that care about Ready payload stability.

