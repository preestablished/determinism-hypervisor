# Critical And Important Issues

No critical or important issues found.

The restore ordering remains RAM first, then device restore, then vCPU restore in `crates/dh-worker/src/restore_engine.rs:156` and `crates/dh-worker/src/restore_engine.rs:319`. The new EVTC KVM test in `crates/dh-worker/tests/restore_engine.rs:232` exercises the intended ordering by restoring into a fresh slot whose detchannel page is populated only by `restore_snapshot`, then checking the restored host reattached and re-read the manifest.
