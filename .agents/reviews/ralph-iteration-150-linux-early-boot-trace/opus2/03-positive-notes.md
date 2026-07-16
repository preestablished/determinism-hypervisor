# Positive Notes

- `tests/determinism/tests/linux_boot_trace.rs:134` uses `BTreeMap` and `BTreeSet` for trace fields, which keeps emitted JSON ordering stable across runs and hosts.

- `tests/determinism/tests/linux_boot_trace.rs:333` gives every `VcpuExit` variant an explicit raw-kind label. That is good long-term pressure: new or changed KVM exit variants will be handled deliberately instead of disappearing into a catch-all.

- `tests/determinism/tests/linux_boot_trace.rs:239` re-reads the instruction counter after `EINTR` instead of assuming a kick means the target was reached. That matches the spurious-kick contract in the run module.

- `tests/determinism/tests/linux_boot_trace.rs:607` escapes JSON strings, including control characters, instead of directly interpolating terminal reasons into the artifact.

- `tests/determinism/tests/linux_boot_trace.rs:628` adds a focused unit test for the trace schema fields, so at least the non-KVM serialization path is covered by normal `cargo test`.
