# Critical And Important Issues

## Critical

No Critical issues found.

## Important

### Important: EVTC restore accepts malformed flags and can silently change channel state

- File: `crates/dh-devices/src/detchannel.rs:311`
- File: `crates/dh-devices/src/detchannel.rs:318`
- File: `crates/dh-devices/src/detchannel.rs:319`
- File: `crates/dh-devices/src/detchannel.rs:321`

`DetChannelHost::restore` validates only section version and byte length before interpreting the EVTC internals. The option flags are decoded with `bytes[n] == 1`, so any value other than `1` is treated as false. That means a single corrupted flag byte can turn a serialized attached channel into a detached restored host while restore still returns `Ok(())`.

Concrete example: a valid attached EVTC has `bytes[22] == 1` and carries `gpa`, `ring_c`, and `ring_i` in `bytes[23..39]`. If `bytes[22]` is corrupted to `2`, the restore path takes the detached branch, ignores those nonzero bytes, leaves `channel` and `channel_gpa` as `None`, but still restores `init_status` from `bytes[8..12]`. The resulting host can answer `IN PORT_INIT_GO` as `Ok` while `push_command` and drains behave as not attached. The same permissive pattern applies to `inject_iseq` at byte 12 and `last_quiesce_ack` at byte 17, and arbitrary `init_status` values are accepted as well.

This conflicts with the local restore contract: `RestoreError` means malformed device section bytes are rejected, and `restore_engine.rs` depends on loud shape strictness when applying DHSNAP sections. EVTC is now part of that generic device loop, so it needs the same canonical decode discipline as the other device sections.

Suggested fix: decode EVTC through explicit helpers that reject non-`0`/`1` flags, reject nonzero payload bytes when a flag is `0`, and reject impossible state combinations. At minimum:

- `inject_iseq`, `last_quiesce_ack`, and `channel` flags must be exactly `0` or `1`.
- False option payload bytes should be zero because the branch writer emits zeros.
- `init_status` should be limited to `INIT_STATUS_NEVER_COMMITTED` or known `InitStatus` values.
- Attached channel state should not be restorable with an impossible status such as `BadGpa` or an absent channel with `AlreadyAttached`.

Add negative tests next to `evtc_roundtrips_detached_state_and_refuses_bad_input` that mutate each flag to `2`, set nonzero payload under a false flag, and inject an invalid status value.
