# Review Summary

Branch: `ralph/iteration-129-m6-accept-capture-neutrality-layout-version`
Base: `origin/main` (`f982527a482543f835b312c73a7c218ceef6c7b8`)
Commit reviewed: `e2b4875c3d7aa17c1b3b8b30700b2effec2278a9`
Bead: `determinism-hypervisor-pee`

Scope reviewed:

- `crates/dh-worker/src/service.rs`
- New M6 acceptance helper/test coverage for capture neutrality and `layout_version` `FAILED_PRECONDITION`
- Relevant production run/snapshot paths and `dh-vmm` run control epoch callback wiring

Result: one blocking finding.

The branch adds useful coverage for capture/no-capture child snapshot ref equality and for bad `layout_version` returning `FAILED_PRECONDITION` on both `Run` and `TakeSnapshot`. However, the acceptance currently has a false-positive hole for epoch hashes: the service-level comparison can pass with zero service DHILOG `EPOCH_HASH` records, and the production service `Run` path currently uses the runctl wrapper that discards epoch callbacks.

