# 02-suggestions.md

Suggestion: `crates/dh-worker/tests/common/mod.rs:153` ignores `DH_M9_GUEST`. If that variable is intended to be a semantic guard rather than just command documentation, add a small check that fails when it is set to anything other than `linux`.

Suggestion: `crates/dh-worker/tests/m4_transparency.rs:514` checks fork children at READY but does not run them after fork. Consider a tiny deterministic run or replay-backed post-fork check once the Linux fixture has a stable post-READY behavior.

Suggestion: `crates/dh-worker/tests/m5_net_loopback.rs:437` opens `DH_M9_GAME_IMAGE` directly for the standalone pv-blk fixture. If this path remains, prefer the verified image-cache blob keyed by hash so the fixture consumes the same artifact identity as `WorkerService`.
