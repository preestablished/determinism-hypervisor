# Tests

Ran:

```text
cargo test -p dh-vmm linux_cpu_compat -- --nocapture
```

Result: passed, 7 tests.

```text
cargo test -p determinism-tests --test linux_boot_trace trace_json_reports_required_m9_fields -- --nocapture
```

Result: passed, 1 test.

```text
cargo test -p dh-worker proto_map -- --nocapture
```

Result: passed, 10 tests.

```text
cargo fmt --check
```

Result: failed. Rustfmt wants import/order and wrapping changes in this branch's touched files (`crates/dh-vmm/src/inject.rs`, `crates/dh-vmm/src/kvm.rs`, `crates/dh-vmm/src/msr.rs`, `crates/dh-vmm/src/runctl.rs`, `tests/determinism/tests/linux_boot_trace.rs`) plus pre-existing-looking formatting diffs in `crates/dh-vmm/src/agenda.rs` and `tests/nanokernel/tests/capture_manifest_interop.rs`. I did not run `cargo fmt` or edit production code.

Not run:

```text
DH_M9_TRACE_BOOT=1 cargo test -p determinism-tests --test linux_boot_trace -- --ignored --nocapture
```

Reason: external Linux artifacts/env are required. I did inspect `target/m9/linux_boot_trace.json`; it shows empty unclassified MSR/MMIO/IRQ buckets, but the test does not assert that.
