# Overview

Branch: `ralph/iteration-120-dhilog-unsealed-inspect`

Bead: `determinism-hypervisor-lyu` - DHILOG inspection-only entry point for unsealed crash artifacts.

Reviewed files:

- `crates/dh-inputlog/src/reader.rs`
- `crates/dh-inputlog/tests/reader_validation.rs`
- `crates/dh-inputlog/fuzz/fuzz_targets/dhilog_parse.rs`

Verdict: no blocking correctness findings. The change adds a separate `LogInspection` entry point that intentionally skips replay-only gates while preserving strict record prefix scanning before exposing typed record bodies. Existing replay and verification paths continue to enter through `LogReader::parse`.

Validation run:

- `cargo test -p dh-inputlog`
- `cargo fmt --check --package dh-inputlog`
- `git diff --check`
- `cargo +nightly fuzz check dhilog_parse`

