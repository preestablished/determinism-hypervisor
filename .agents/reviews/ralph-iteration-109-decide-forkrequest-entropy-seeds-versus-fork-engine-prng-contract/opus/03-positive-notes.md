Good patterns to preserve:

- proto/hypervisor.proto:141 keeps entropy_seeds on the existing field number and changes only the semantics comment.
- crates/dh-worker/src/proto_map.rs:104 centralizes request seed normalization for future service wiring.
- crates/dh-worker/src/fork_engine.rs:135 defensively treats an explicit zero seed as continue, matching the public contract even if a caller bypasses the mapper.
- crates/dh-worker/tests/fork_engine.rs:164 verifies both non-zero reseed and explicit-zero continue behavior at the engine boundary.

