# Review Overview

Branch: `ralph/iteration-118-dhilog-lineage-fuzz-target`

Bead: `determinism-hypervisor-6zm`

Scope reviewed:

- `.github/workflows/nightly-drift.yaml`
- `crates/dh-inputlog/fuzz/Cargo.toml`
- `crates/dh-inputlog/fuzz/fuzz_targets/dhilog_splice.rs`

Summary:

The new `dhilog_splice` target is registered in cargo-fuzz and compiles. It invokes `Lineage::new`, incremental `Lineage::extend`, and `Lineage::edges` over both a raw single segment and a length-framed segment list. That is the right API surface for this bead.

I found two issues to address before landing:

1. The workflow matrix changes the documented 24h operator dispatch from one 24h fuzz run into two full-duration matrix legs on the single `kvm-intel` runner.
2. The target supports multi-segment input, but the checked-in CI seed path only provides unframed single DHILOG files, so valid multi-segment composition is not realistically seeded.

Local checks run:

- `cargo +nightly fuzz check dhilog_splice`
- `cargo +nightly fuzz check dhilog_parse`
