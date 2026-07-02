# Tests And Fixtures

## Fixture Strategy (the main judgment call — decided here)

Both nanokernel fixtures publish `layout_version 1` framebuffer regions that
violate the new contract (`01-current-state.md`). Recommended rework:

### capture_fixture: resize to D7 length, keep everything else

Change `CAPTURE_FIXTURE_FB_BYTES` from `0x1_0000` (64 KiB) to `229_376` in
`tests/nanokernel/src/lib.rs` and in `asm/capture_fixture.asm` (region len in
the manifest entry + the fill-loop bound; the pattern stays
`CAPTURE_FIXTURE_FB_QWORD_BASE + j`, now for 28,672 qwords). The region sits
at `CAPTURE_FIXTURE_FB_GPA = 0x60_0000`; 229,376 B (56 pages) ends well below
any neighboring structure, but **verify against the asm's memory map while
editing** — check what the fixture places above 0x60_0000 and confirm the
guest RAM size in `capture_fixture_machine_config` covers
`0x60_0000 + 229_376`. Keep `layout_version 1` and the FRAMEBUFFER flag.

Why resize rather than drop the FRAMEBUFFER flag: it preserves end-to-end
coverage of `GetFramebuffer` and `capture.framebuffer` against a real guest
region with nonzero, known pixel bytes, using the fixture the tests already
boot. The `capture.ranges` tests keep working unchanged
(`capture_fixture_bytes(8, 24)` is the same prefix pattern).

### framebuffer_fixture: delete it

It exists (bead 02r) solely to feed the descriptor parse being removed. With
capture_fixture now D7-conformant, a separate raw-pixel fixture is redundant.
Delete `asm/framebuffer_fixture.asm`, the `framebuffer_fixture_elf()` embed,
the `FRAMEBUFFER_FIXTURE_*` constants (`lib.rs:184–229`), the build-script
entry that assembles it, the `lib.rs` self-test referencing it (~line 375),
and the service.rs helpers/tests that consume it
(`framebuffer_fixture_machine_config` line 5197, `framebuffer_fixture_pixels`
line 5240, test at line 7074). If you find a reason to keep a second fixture
(e.g. exercising a *different* layout_version end-to-end), repurposing it is
acceptable — but don't keep a descriptor-bearing region anywhere.

## Unit Tests (pure functions, no KVM — these are acceptance criteria 1–3)

Rewrite `framebuffer_descriptor_shape_is_enforced` (line 5848) as e.g.
`framebuffer_layout_contract_is_enforced`:

1. **Zeroed v1 region** (`vec![0u8; 229_376]`, layout_version 1) →
   `Ok`: width 256, height 224, stride 1024, XRGB8888, pixels = the zeros,
   len 229,376. (Criterion 1, black-frame half.)
2. **Nonzero v1 region** (pattern bytes, right length) → `Ok`, pixels
   round-trip exactly. (Criterion 1, nonzero half.)
3. **Unknown layout_version** (e.g. 0 and 2, right length) →
   `FailedPrecondition`, message contains the offending version number.
   (Criterion 2.)
4. **v1 wrong length** (e.g. 65,536 and 229,377) → `FailedPrecondition`,
   message contains both `229376` and the actual length. (Criterion 2.)
5. **Capture builder determinism**: the capture-path builder on a zeroed and
   a nonzero v1 region returns identical `FbInfo` geometry (only pixels
   differ), `frame_counter` passes through; unknown version / wrong length
   error rather than returning `None`/zero-geometry. (Criterion 3.)

Also update the `capture_at_boundary` unit/runtime tests around lines 5403
and 5633 — compile errors from the removed helpers will point at every site;
re-check any `fb_info`/`fb_lz4` expectation against the new contract.

## Runtime Tests (KVM-gated, `runtime_tests_available()`)

- `run_capture_spec_reads_manifest_ranges_and_lz4_framebuffer` (line 7011):
  was asserting the heuristic's raw fallback (zero `FbInfo` + `lz4(region)`).
  Now assert `FbInfo{256, 224, 1024, XRGB8888, frame_counter}` and
  `fb_lz4` decompressing to the 229,376-byte capture-fixture pattern.
- `introspection_rpcs_read_memory_framebuffer_and_stream_guest_events`
  (line 7159): the GetFramebuffer-fails block (lines 7226–7233) flips to
  success: assert contract geometry, `pixels.len() == 229_376`, and
  `pixels == capture_fixture_bytes(0, 229_376)`. The `region_ranges` and
  guest-events parts are untouched.
- `descriptor_framebuffer_fixture_feeds_capture_and_get_framebuffer`
  (line 7074): delete with its fixture (above). Its useful coverage now
  lives in the two tests above.

## Integration Tests

- `crates/dh-worker/tests/m6_full_api_uds.rs`: update
  `expected_capture_bytes()` (line 139) for the new
  `CAPTURE_FIXTURE_FB_BYTES` **only if** it derives the full region; its
  `capture.ranges` expectations (offset 8, len 24) are length-independent.
  Check whether `capture_spec()` (line 127) sets `framebuffer: true` — if it
  does, its snapshot assertions (line 498 area) move from zero-FbInfo to
  contract geometry. This test is KVM + 64-core gated; if this host can't
  run it, say so explicitly in the handoff rather than claiming it passed.
- Grep the workspace for other `capture.framebuffer` / `fb_info` / `fb_lz4`
  consumers before declaring done:
  `grep -rn "fb_info\|fb_lz4\|framebuffer" crates/ --include='*.rs' -l`
  and re-check anything outside service.rs (e.g. snapshot_engine,
  determinism-tests) that stores or hashes capture output.

## Validation Runbook

```sh
# fast loop while developing
cargo test -p dh-worker
cargo clippy -p dh-worker --all-targets

# KVM-gated runtime tests (this host has KVM; runtime_tests_available() self-gates)
cargo test -p dh-worker --release -- --ignored --nocapture

# determinism gate before merge: 3+ consecutive FULL workspace runs,
# each gated on exit code — never `test ; commit`
cargo test --workspace --release && \
cargo test --workspace --release && \
cargo test --workspace --release
```

If any run flakes, treat it as real (process memory: determinism flakes show
up under parallel-suite load) — do not merge until three consecutive clean
runs.
