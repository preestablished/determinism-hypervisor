# 03-positive-notes.md
- [dhilog.rs](/home/infra-admin/git/preestablished/determinism-hypervisor/crates/dh-inputlog/src/dhilog.rs:349): The writer encodes the 56-byte checkpoint payload explicitly and uses the normal record path, so sequence numbering, padding, `HAS_AUX`, and ordering behavior remain centralized.

- [reader.rs](/home/infra-admin/git/preestablished/determinism-hypervisor/crates/dh-inputlog/src/reader.rs:667): The reader adds bounded layout checks before exposing the typed body, preserving the existing “validated bytes make `Record::body()` infallible” model.

- [reader_validation.rs](/home/infra-admin/git/preestablished/determinism-hypervisor/crates/dh-inputlog/tests/reader_validation.rs:762): Tests cover malformed checkpoint payload length, unsupported nested version, nonzero nested flags, typed decoding, and inspection-path behavior.

- [golden.rs](/home/infra-admin/git/preestablished/determinism-hypervisor/crates/dh-inputlog/tests/golden.rs:18): The existing v1.0 fixture freeze is left intact, with the additive AUX record tested through reader validation rather than rewriting frozen fixtures.
