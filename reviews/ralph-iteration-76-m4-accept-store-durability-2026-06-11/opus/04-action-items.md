# Action Items

### Critical

_None._

### Important

_None._

### Suggestions

- [ ] **Scope the module doc to ack-time durability.** In `crates/dh-worker/tests/store_durability.rs:1-17`, add one sentence clarifying this proves "pages + manifest are fdatasync'd before the ref returns, and a fresh process serves them back" — and that crash-consistency under kill-mid-write / power loss is the fault-injection bead **v1n** (which drives the store's existing `fail_point!`s: `manifest-fsync`, `manifest-rename`, `manifest-dirsync`, `pack-fdatasync`). Prevents a reader from thinking v1n is already covered. (02-#1)

- [ ] **State that the graceful shutdown is incidental, optionally prove it.** At `crates/dh-worker/tests/store_durability.rs:174-178`, `ServerHandle::shutdown` only sends a oneshot — no flush, no wait — and there is no flush-on-Drop in the pagestore/store. Either (a) sharpen the "KILL the server" comment to note durability was already complete at ack and does not depend on graceful shutdown, or (b) before `shutdown()`, assert the `<hex>.spm` for `delta.snapshot_ref` already exists under `<data_root>/store/manifests/` — nailing "durable before any shutdown opportunity" in-process. (02-#2)

- [ ] **Annotate the instance-2 lifetime binding.** At `crates/dh-worker/tests/store_durability.rs:181`, add a short comment that `_rt2`/`_handle2` are kept alive deliberately (`store2` bridges into `rt2`), so a future cleanup doesn't collapse them and break the test. Mirror the LIFETIME NOTE convention already in `tests/determinism/tests/store_joint.rs`. (02-#3)

- [ ] **Clarify the ref-identity assertion message.** At `crates/dh-worker/tests/store_durability.rs:281-302`, reword the `assert_eq!(ref_b2, ref_b1, ...)` message to spell out that a FULL re-snapshot of the restored machine is content-addressed and the restarted store must reproduce instance 1's exact ref. Cosmetic. (02-#4)
