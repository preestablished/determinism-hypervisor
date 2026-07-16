Test reliability:

- The focused test target passed locally:
  `cargo test -p dh-snapshot --test snapstore_readiness -- --nocapture`
- The fixture uses a unique temp directory per test and UDS address under that directory, so parallel test execution should not collide on sockets or persisted store state.
- The server is spawned through the same `serve_for_tests` seam used by existing worker integration tests, and the client uses the blocking facade shape expected by synchronous worker code.
- The raw generated-client check opens a second UDS connection to the same in-process server. That is appropriate for the contract being tested and still stays within the tempdir-scoped fixture.
- The new dependency delta is limited to dev-dependencies already declared at workspace level; `Cargo.lock` only adds those dependency names under the `dh-snapshot` package entry.

No reliability blocker was found.

Minor risk:

- The fixture intentionally does not use the page-channel fast path. That matches the stated scope because existing worker tests cover page-channel/payload behavior, while this test is meant to pin hashes-only and typed error behavior in `dh-snapshot`.
