# Test Reliability

The fixture is isolated:
- `TempDir` provides a per-test data root and UDS socket path.
- TCP/HTTP addresses use `127.0.0.1:0`.
- `page_channel_path` is `None`, so the test avoids Linux-only SEQPACKET fixture complexity.
- `serve_for_tests` returns after the service is marked serving, and the blocking client connects over the returned UDS path.
- The raw generated gRPC helper connects to the same UDS endpoint and drains the server stream synchronously inside the fixture runtime.

Shutdown/lifecycle notes:
- `LiveStore::drop` sends the explicit server shutdown signal at `crates/dh-snapshot/tests/snapstore_readiness.rs:174`.
- The temp directory is held for the duration of the client/server fixture.
- There is no join/wait on the spawned server tasks, but the runtime is owned by the fixture and is dropped after test work is complete. I did not see this as a blocker for the current short, synchronous tests.

Dev-dependencies:
- `snapstore-client` was already present.
- `snapstore-manifest`, `snapstore-server`, `snapstore-types`, `tempfile`, and `tokio` are all directly used by `snapstore_readiness.rs`.
- The raw generated gRPC coverage uses `snapstore_client::snapstore_proto`, so no additional proto dev-dependency is needed.
- `Cargo.lock` lines `466`-`477` reflect the new `dh-snapshot` dev-dependency closure only; no unrelated lockfile churn was observed in the scoped diff.

Verification:
- `cargo test -p dh-snapshot --test snapstore_readiness -- --nocapture`: 3 passed.
- `cargo fmt --check --package dh-snapshot`: passed.
- `git diff --check -- crates/dh-snapshot/Cargo.toml crates/dh-snapshot/tests/snapstore_readiness.rs Cargo.lock`: passed.
