# Current State (verified 2026-07-02, repo tip `a38a1a0`)

All line numbers below were checked against the working tree while writing
this plan. They will drift slightly as you edit; use the symbol names.

## Worker-Side Code Map (`crates/dh-worker/src/service.rs`)

### Constants (~line 100–110)

- `MAX_CAPTURE_FRAMEBUFFER_BYTES: usize = 64 * 1024 * 1024` — the D7 region
  (229,376 B) fits comfortably; no cap change needed.
- `FRAMEBUFFER_DESCRIPTOR_BYTES: usize = 16` (line 108) — the descriptor
  expectation to be deleted.

### Read path

- `read_framebuffer_region_from_bus(bus, caller) -> Result<Vec<u8>, Status>`
  (line 2644). Resolves the detchannel manifest, finds the live entry with
  `REGION_FLAG_FRAMEBUFFER`, calls `manifest.resolve(name)` which returns a
  `detguest_host::manifest::ResolvedRegion` **carrying `layout_version: u32`
  and `len: u64`** (see guest-sdk section below) — then discards both and
  returns only the bytes. **This is the plumbing gap**: the layout version is
  already in hand at line 2666–2668 and must be threaded out.
- `read_framebuffer_from_bus(bus)` (line 2682) — GetFramebuffer's entry;
  calls the above then `framebuffer_response_from_region_bytes`.
- `framebuffer_response_from_region_bytes(region) -> Result<(u32, u32, u32, i32, Vec<u8>), Status>`
  (line 2690) — parses `region[..16]` as LE `width|height|stride|format`,
  validates, returns `region[16..16+stride*height]` as pixels. Produces the
  two errors the bridge observed ("zero dimensions", "unsupported
  pixel_format"). To be replaced.

### Capture path

- `descriptor_framebuffer_capture(region, frame_counter) -> Result<Option<(Vec<u8>, FbInfo)>, Status>`
  (line 2742) — gates on the heuristic below; `None` means "raw region, no
  geometry".
- `framebuffer_region_advertises_descriptor(region) -> bool` (line 2763) —
  `known_format || (width != 0 && height != 0 && stride >= width)` over the
  first 16 bytes. **Data-dependent per frame** against a raw-pixel region.
  To be deleted.
- `capture_at_boundary(bus, capture, frame_counter)` (line 2784) — for
  `capture.framebuffer == true` (line 2856): reads the region, calls
  `descriptor_framebuffer_capture`; on `Some` emits parsed geometry, on
  `None` emits `lz4(region)` + zero `FbInfo` (`width/height/stride = 0`,
  `PfUnspecified`). Also note: for `capture.ranges` it already validates
  `region.layout_version != range.layout_version` (line 2831) — precedent
  for layout-version-keyed error messages.

### RPC handlers

- `get_framebuffer` (line 4475) — drains detchannel at pause, reads
  `frame_counter_from_bus`, calls `read_framebuffer_from_bus` (line 4505),
  builds `GetFramebufferResponse{width,height,stride,format,frame_counter,icount,pixels}`.
  Handler shape is fine; only the geometry source changes underneath it.
- `Run` capture (~line 4047) and `TakeSnapshot` capture (~line 4177) both go
  through `capture_at_boundary`.
- `RunWithFrameCapture` (~line 4681) is `unimplemented` — future consumer,
  ignore.

## Guest-Sdk Facts (path dependency, read-only for this change)

- `detguest-host/src/manifest.rs:47` — `RegionManifest::resolve(&self, name)
  -> Option<ResolvedRegion>`; `ResolvedRegion` (line ~32) has `region_id`,
  `layout_version: u32`, `len: u64`, `flags`, `extents`. No geometry fields.
- `detguest-wire/src/manifest.rs:137` — `RegionEntry.layout_version: u32`.
  No geometry fields. Confirms the request: layout_version is the only
  channel for geometry.

## Reference-Workload Facts (the D7 side, read-only)

- `reference-workload/crates/refwork-emu/src/timing.rs:34–40`:
  `FB_WIDTH = 256`, `FB_HEIGHT = 224` (line 36), `FB_STRIDE = 1024`,
  `FB_BYTES = FB_STRIDE * FB_HEIGHT` (= 229,376).
- `reference-workload/crates/refwork-harness/src/regions.rs:276`:
  `PublishedRegion::new("framebuffer", FB_BYTES)` — raw pixels, no header.
- Authority doc: `~/.agents/projects/determinism/docs/reference-workload/ARCHITECTURE.md`
  §1 D7 (lives in the determinism docs tree, not in this repo or in the
  reference-workload checkout).

## Existing Tests And Fixtures That This Change Breaks

### Nanokernel fixtures (`tests/nanokernel/`, consumed via
`crates/dh-worker/Cargo.toml:47` path dep; asm in `tests/nanokernel/asm/`,
assembled by the crate's build into `OUT_DIR` ELFs)

- **capture_fixture** (`asm/capture_fixture.asm`; constants in
  `tests/nanokernel/src/lib.rs:206–217`): publishes region `"framebuffer"`,
  `CAPTURE_FIXTURE_FB_BYTES = 0x1_0000` (64 KiB) of raw qword pattern
  (`CAPTURE_FIXTURE_FB_QWORD_BASE + j`), with the FRAMEBUFFER flag set and
  `CAPTURE_FIXTURE_DEFAULT_LAYOUT_VERSION: u32 = 1`. Under the new contract:
  layout_version 1 with len 65,536 ≠ 229,376 → rejected.
- **framebuffer_fixture** (`asm/framebuffer_fixture.asm`; constants at
  `lib.rs:219–229`): publishes a 144-byte region = 16-byte descriptor
  (8×4, stride 32, XRGB8888) + 128 pattern bytes,
  `FRAMEBUFFER_FIXTURE_DEFAULT_LAYOUT_VERSION: u32 = 1`. This fixture was
  built (bead 02r) specifically to exercise the descriptor parse being
  deleted. Under the new contract: rejected (wrong length).

### service.rs unit tests (in-file `#[cfg(test)]`)

- `capture_fixture_bytes` / `framebuffer_fixture_pixels` helpers
  (lines 5231–5246) and `capture_fixture_spec` (line 5251).
- `framebuffer_descriptor_shape_is_enforced` (line 5848) — pure-function
  test of the descriptor parse; to be rewritten for the layout table.
- `capture_at_boundary` unit test around line 5403 and run-capture test
  around line 5633 use the capture fixture pattern for `capture.ranges`
  (feature bytes) — the ranges path is untouched, but any
  `capture.framebuffer` assertions in them must be re-checked.

### service.rs runtime tests (KVM-gated via `runtime_tests_available()`)

- `run_capture_spec_reads_manifest_ranges_and_lz4_framebuffer` (line 7011) —
  asserts the current `None`-heuristic behavior: raw 64 KiB region → zero
  `FbInfo` + `lz4(region)`.
- `descriptor_framebuffer_fixture_feeds_capture_and_get_framebuffer`
  (line 7074) — boots framebuffer_fixture, asserts descriptor geometry
  (8×4×32) from both capture and GetFramebuffer.
- `introspection_rpcs_read_memory_framebuffer_and_stream_guest_events`
  (line 7159) — boots capture_fixture; asserts GetFramebuffer fails with
  `FailedPrecondition` containing "framebuffer descriptor" (line 7226–7233).
  The `region_ranges` part (layout_version-checked reads) is unaffected.

### Integration tests (`crates/dh-worker/tests/`)

- `m6_full_api_uds.rs` — uses `capture_fixture` and
  `CAPTURE_FIXTURE_DEFAULT_LAYOUT_VERSION` for `capture.ranges`
  (lines 105–142, 483, 498). Check whether its `capture_spec()` sets
  `framebuffer: true`; if so its snapshot-capture assertions change too.
  KVM-gated (`--ignored`).
- No other test file references `GetFramebuffer` or the framebuffer fixtures
  (verified by grep across `crates/`).

## Provenance Note

`git log -S FRAMEBUFFER_DESCRIPTOR_BYTES` traces the descriptor expectation
to ralph iteration commits with no decision record and no guest-side
counterpart. D7 predates it; the worker is the divergent side. This is why
the fix includes a decision record (`05-docs-beads-closeout.md`).
