# Suggestions (non-blocking)

### S1 — Unused `GuestMem` import is actually load-bearing; leave a note or drop the explicit name

- **File:** `crates/dh-devices/tests/detguest_host_smoke.rs:11`
- **Observation:** `GuestMem` is imported and *is* used — `gm.write(...)` /
  `gm.read(...)` are trait methods, so the trait must be in scope. The build
  produces no `unused_imports` warning (verified). No change required; this note
  exists only to preempt a future reader "cleaning up" the import and breaking
  the build. If you want to make the dependency obvious, a one-word comment
  `// GuestMem: brings write()/read() into scope` would help, but it is optional.

### S2 — Test name `read_region_resolves_by_name` over-promises

- **File:** `crates/dh-devices/tests/detguest_host_smoke.rs:121`
- **What/why:** The function only asserts the `NameNotFound` error against an
  empty manifest — it never resolves a region by name. The name reads like a
  success-path test. Rename to reflect the assertion:

  ```rust
  fn read_region_rejects_unknown_name() {
  ```

  (Or add the positive case from finding I1 and keep the name.)

### S3 — Duplicated `ManifestHeader` construction across two tests

- **File:** `crates/dh-devices/tests/detguest_host_smoke.rs:30-40` (in
  `fresh_channel_mem`) and `:103-113` (in
  `read_manifest_snapshots_and_retries_seqlock`)
- **What/why:** The seqlock test rebuilds an almost-identical `ManifestHeader`
  just to flip `generation: 0 -> 1`. A tiny helper removes the copy-paste and
  makes the *only* meaningful difference (the odd generation) obvious:

  ```rust
  fn write_manifest(gm: &mut MockGuestMem, generation: u64) {
      let manifest = ManifestHeader {
          magic: MANIFEST_MAGIC,
          manifest_version: MANIFEST_VERSION,
          region_capacity: REGION_CAPACITY as u16,
          generation,
          region_count: 0,
          extent_count: 0,
      };
      let mut m = [0u8; 32];
      manifest.write_to(&mut m).unwrap();
      gm.write(BASE + OFF_MANIFEST as u64, &m).unwrap();
  }
  ```

  `fresh_channel_mem` then calls `write_manifest(&mut gm, 0)`, and the seqlock
  test does `let mut gm = fresh_channel_mem(); write_manifest(&mut gm, 1);`.
  Aligns with `rust-integration-testing.md` ("shared fixture code deduplicated
  rather than copy-pasted").

### S4 — Magic-number buffer size `[0u8; 32]`

- **File:** `crates/dh-devices/tests/detguest_host_smoke.rs:38, 106`
- **What/why:** The 32 is exactly `detguest_wire::manifest::OFF_ENTRIES` (0x20) —
  the minimum buffer `ManifestHeader::write_to` accepts. It works, but the literal
  is opaque. The wire crate does not export a `MANIFEST_HEADER_SIZE` constant, so
  the cleanest in-repo fix is a local `const`:

  ```rust
  // Min buffer ManifestHeader::write_to accepts (== detguest_wire OFF_ENTRIES).
  const MANIFEST_HDR_BYTES: usize = 32;
  ```

  Optional; a future upstream rename of the header size would be caught by the
  smoke test's compile/run regardless. (A stronger fix — exporting
  `MANIFEST_HEADER_SIZE` from `detguest-wire` — belongs in the sibling repo, not
  this branch.)

### S5 — Cargo.toml comments are excellent; consider linking the bead IDs to a tracking note

- **File:** `Cargo.toml:23-28`, `crates/dh-devices/Cargo.toml:12-13`
- **What/why:** The comments already explain *why* `detguest-wire` rides along
  (the `ChannelWriteSink` signature takes `detguest_wire::RingId`) and that the
  dev-deps get promoted by bead `nln`. This is genuinely good. Per
  `cargo-workspace-path-deps.md` ("CI must check out the sibling at a compatible
  revision; builds are not reproducible from this repo alone"), the one thing not
  captured anywhere in the diff is the **CI sibling-checkout story**. If CI does
  not already clone `../guest-sdk`, this branch will build locally but fail in CI.
  Recommend confirming the CI workflow checks out `guest-sdk` (and recording that
  in the bead) — no code change needed in this PR if it already does.
