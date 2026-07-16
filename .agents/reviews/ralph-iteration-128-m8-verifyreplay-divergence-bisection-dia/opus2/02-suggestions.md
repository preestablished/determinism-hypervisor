## Suggestions

- [crates/dh-worker/src/service.rs:692] `reg_diff` mixes real RIP data with hash words named `state_hash_wordN`; use a separate hash-diff diagnostic or stronger schema name so consumers do not treat hash chunks as CPU registers.

- [crates/dh-worker/src/service.rs:3551] Add a test that proves the reported interval contains a known earlier divergence, not just that it is narrow.

- [crates/dh-worker/src/service.rs:3409] The test helper mutates DHILOG bytes using hard-coded offsets; prefer named header constants or a test-only parser/writer helper.

- [crates/dh-verify/src/verify.rs:6] Update stale comments only once the M8 scope really lands.

- [crates/dh-worker/src/verify_replay.rs:4] Update stale comments only once the M8 scope really lands.
