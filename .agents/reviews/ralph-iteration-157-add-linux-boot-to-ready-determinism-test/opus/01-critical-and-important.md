# 01-critical-and-important.md

No critical findings.

**Important - `tests/determinism/tests/common/mod.rs:454` and `tests/determinism/tests/common/mod.rs:479` - detchannel PIO branches ignore `DevCtx::log_fault()` - risk: detchannel `DEV_EVENT`/PIO-answer records can fail to log without failing the test, weakening the pre-Ready host-input assertion because the sealed log may be missing the exact records it relies on. Production `DeviceRail::service_exit` and `dh-worker` service paths check `ctx.log_fault()` after device handling. Recommended fix: in both detchannel `IoOut` and `IoIn` arms, after the anomaly check and before returning, call `ctx.log_fault()` and convert any fault into `BoundaryError::Exit(format!("log fault: {e:?}"))`.**
