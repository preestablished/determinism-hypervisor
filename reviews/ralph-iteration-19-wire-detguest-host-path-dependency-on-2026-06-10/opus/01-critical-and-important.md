# Critical & Important Issues

## Critical

**None.** This is an additive change: two dev-only path dependencies plus one
smoke test. No production code path changes, nothing in the production dependency
graph moves, no `unsafe`, no I/O, no data-loss surface. The branch builds clean
and all 6 tests pass.

## Important

### I1 — Happy-path coverage gap for `drain_events` and `read_region` (success with real data)

- **Severity:** Important (test coverage, non-blocking for a linkage smoke test)
- **File:** `crates/dh-devices/tests/detguest_host_smoke.rs:56-64, 121-129`
- **Description:** The smoke test exercises `drain_events` and `read_region` only
  on their *empty / not-found* paths:
  - `drain_events_on_empty_rings_yields_nothing` asserts the empty-ring result.
  - `read_region_resolves_by_name` only asserts `NameNotFound` against an empty
    manifest — despite the test name implying a successful resolve, it never
    resolves a region. So the actual stitching/extent-walk contract this repo
    will lean on (`read_region` returning bytes, `drain_events` returning a
    populated event list) is unverified here.

  The research note `rust-integration-testing.md` ("Are failure paths covered,
  not just success paths?") cuts the other way for these two: the *success* path
  is the one missing. The consumed crate already covers both internally
  (`manifest.rs::read_region_stitches_three_discontiguous_extents`,
  `inject.rs::responder_answers_matched_query_via_plan_and_logs_pio`), so the
  contract *is* tested upstream — but this repo's smoke test is the thing that
  catches an API/signature drift when the sibling repo changes under a path dep
  (`cargo-workspace-path-deps.md`: "HEAD of whatever is on disk wins"), and a
  signature change to the *return* shape of `drain_events`/`read_region` would
  slip past the current empty/error-only assertions.

- **Suggested fix:** Add one positive case for each, reusing the sibling repo's
  own fixture approach (`put_region` + `add_segment`, `channel_with_w_records`).
  A minimal `read_region` success case:

  ```rust
  #[test]
  fn read_region_returns_bytes_for_a_live_region() {
      use detguest_wire::manifest::{init_manifest, Extent, RegionEntry, ManifestHeader, MANIFEST_TOTAL_SIZE};
      const SEG: u64 = 0x4000_0000;
      let mut gm = MockGuestMem::with_zeroed(BASE, CHANNEL_SIZE);
      let mut hdr = [0u8; OFF_RESERVED];
      ChannelHeader::canonical().write_to(&mut hdr).unwrap();
      gm.write(BASE, &hdr).unwrap();

      let mut area = vec![0u8; MANIFEST_TOTAL_SIZE];
      init_manifest(&mut area).unwrap();
      let e = RegionEntry {
          region_id: 0, name_id: 1, layout_version: 1, flags: 0,
          gva: 0, len: 8, extent_off: 0, extent_n: 1,
          name: RegionEntry::pack_name(b"telemetry").unwrap(),
      };
      e.write_to(&mut area, 0).unwrap();
      Extent { gpa: SEG, len: 8 }.write_to(&mut area, 0).unwrap();
      let mut h = ManifestHeader::read_from(&area).unwrap();
      h.region_count = 1; h.extent_count = 1; h.generation = 2;
      h.write_to(&mut area).unwrap();
      gm.write(BASE + OFF_MANIFEST as u64, &area).unwrap();
      gm.add_segment(SEG, (0u8..8).collect());

      let ch = Channel::attach(gm, BASE).unwrap();
      let mut buf = [0u8; 8];
      ch.read_region("telemetry", 0, &mut buf).unwrap();
      assert_eq!(buf, [0, 1, 2, 3, 4, 5, 6, 7]);
  }
  ```

  (Some of these symbols — `RegionEntry`, `Extent`, `init_manifest` — are not
  re-exported from `detguest_host`'s prelude; they come from
  `detguest_wire::manifest`, which is already a dev-dep, so the import works.)
  This is a *recommendation*, not a merge blocker: the upstream crate owns the
  contract, and this repo's smoke test is explicitly scoped as a "linkage +
  contract check," with the real consumer landing in bead `nln`. If kept as-is,
  consider renaming `read_region_resolves_by_name` to
  `read_region_rejects_unknown_name` so the test name matches what it asserts
  (see suggestion S2).

## Boundary / validation review (no issues found)

- **Seqlock livelock bound** is exercised correctly: the test forces an odd
  generation (`generation: 1`) and asserts `WireError::SeqlockLivelock`, matching
  `manifest.rs:74-106` where `g1 % 2 != 0` loops up to `SEQLOCK_RETRIES = 64`
  before returning the livelock error. This is the right boundary to pin per
  `spsc-ring-memory-ordering.md` ("is the failure deterministic and reported?").
- **Unmapped-GPA path** (`attach_validates_canonical_header`) correctly asserts
  `AttachError::Mem(_)`, matching `channel.rs:152-154` (the `gm.read` `?` converts
  `MemError -> AttachError::Mem`).
- **`push_command` sink invariant** — asserts exactly one `RingPush` on `RingId::C`
  with `new_prod > 0` and non-empty bytes, plus `producer_seqs().ring_c == 1`.
  This matches the "every host mutation reported through the sink" invariant in
  `lib.rs:31-42` and is exactly the input-log contract this repo depends on. Good.
- No decode-path slice indexing is introduced in this repo (the wire decoders
  live in the sibling crate), so `rust-nostd-wire-codecs.md`'s bounds-check
  concerns do not apply to the diff under review.
