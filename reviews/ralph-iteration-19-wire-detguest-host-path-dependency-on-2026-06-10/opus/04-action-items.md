# Action Items

## Action Items

### Critical
- [ ] None.

### Important
- [ ] [crates/dh-devices/tests/detguest_host_smoke.rs:56-64, 121-129] Add success-path coverage: `drain_events` returning a populated event list and `read_region` returning bytes for a live region. Reuse the sibling repo's fixture style (`RegionEntry`/`Extent`/`init_manifest` from `detguest_wire::manifest`, already a dev-dep). Catches return-shape drift the current empty/error-only asserts miss. Non-blocking — upstream owns the contract and bead `nln` lands the real consumer. Snippet in `01-critical-and-important.md` §I1.
- [ ] [CI workflow — not in this diff] Confirm CI checks out the sibling repo `../guest-sdk` at a compatible revision; sibling path deps build locally but break CI if the sibling isn't cloned. Record the checkout story in bead determinism-hypervisor-2w8/nln. (`cargo-workspace-path-deps.md`)

### Suggestions
- [ ] [crates/dh-devices/tests/detguest_host_smoke.rs:121] Rename `read_region_resolves_by_name` → `read_region_rejects_unknown_name` (or add the positive case) so the name matches the assertion.
- [ ] [crates/dh-devices/tests/detguest_host_smoke.rs:30-40, 103-113] Extract a `write_manifest(gm, generation)` helper to remove the duplicated `ManifestHeader` construction; the seqlock test then differs only by the odd generation.
- [ ] [crates/dh-devices/tests/detguest_host_smoke.rs:38, 106] Replace the magic `[0u8; 32]` with a named local `const MANIFEST_HDR_BYTES: usize = 32;` (== `detguest_wire OFF_ENTRIES`).
- [ ] [crates/dh-devices/tests/detguest_host_smoke.rs:11] Optional: add a one-line comment that `GuestMem` is imported to bring `write()`/`read()` trait methods into scope, so a future cleanup pass doesn't drop the (load-bearing) import.
- [ ] [Cargo.toml:23-28] Optional: the comments are already strong; just ensure the CI sibling-checkout note from the Important section lands somewhere durable (bead or CI yaml).
