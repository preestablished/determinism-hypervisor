# Tests And Fixtures

## Fixture Strategy (the main judgment call — decided here)

Both nanokernel fixtures publish `layout_version 1` framebuffer regions that
violate the new contract (`01-current-state.md`). Recommended rework:

### capture_fixture: resize to D7 length, keep everything else

Change `CAPTURE_FIXTURE_FB_BYTES` from `0x1_0000` (64 KiB) to `229_376` in
`tests/nanokernel/src/lib.rs` and in `asm/capture_fixture.asm` (region len in
the manifest entry + the fill-loop bound; the pattern stays
`CAPTURE_FIXTURE_FB_QWORD_BASE + j`, now for 28,672 qwords). Keep
`layout_version 1` and the FRAMEBUFFER flag.

Memory map — verified by review, no surprises: channel at 0x40_0000 spans
2 MiB (`CHANNEL_PAGES 512`) ending at 0x60_0000; the FB at 0x60_0000 now
ends at 0x63_8000; nothing else lives above 0x60_0000; guest RAM is 8 MiB in
both `capture_fixture_machine_config_with_epoch_len` (service.rs:5211) and
m6 (`MEM = 8 << 20`, m6_full_api_uds.rs:38). The guest's own bounds check
(`cmp rax, FB_GPA + FB_BYTES`, capture_fixture.asm:84) auto-updates via the
`%define`.

Two traps from review:

- **CRITICAL — raise `capture_epoch_leg`'s icount budget.** The fill loop
  retires 4 instructions/qword (capture_fixture.asm:124–128): 8,192 × 4 =
  32,768 today, 28,672 × 4 = 114,688 after the resize — and the fixture
  fills the FB *before* publishing the manifest/CHANNEL_INIT.
  `capture_epoch_leg` (service.rs:5274) runs `Until::IcountBudget(100_000)`
  (service.rs:5377), which now stops mid-fill with no manifest attached; its
  `capture_at_boundary(...).unwrap()` (service.rs:5404) then panics, breaking
  `m6_accept_capture_neutrality_and_layout_precondition` (service.rs:7482,
  the C5 acceptance test) **at KVM runtime with no compile error pointing at
  it**. Raise the budget to ≥ ~500k (1M is also fine). Note epoch_len=64, so
  the epoch count grows with the budget — the neutrality assertions tolerate
  that since both legs match.
- **Write the asm size define without an underscore** (`229376` or
  `0x38000`): `capture_fixture_asm_matches_rust_constants`
  (elf_shape.rs:364–376) parses `%define` values with Rust's `parse()`,
  which rejects NASM-legal `229_376` and would panic the drift test.

Why resize rather than drop the FRAMEBUFFER flag: it preserves end-to-end
coverage of `GetFramebuffer` and `capture.framebuffer` against a real guest
region with nonzero, known pixel bytes, using the fixture the tests already
boot. The `capture.ranges` tests keep working unchanged
(`capture_fixture_bytes(8, 24)` is the same prefix pattern).

### framebuffer_fixture: delete it

It exists (bead 02r) solely to feed the descriptor parse being removed. With
capture_fixture now D7-conformant, a separate raw-pixel fixture is redundant.
Full deletion checklist (extended per review — the original inventory missed
the nanokernel crate's own tests):

- `asm/framebuffer_fixture.asm`
- the `framebuffer_fixture_elf()` embed and `FRAMEBUFFER_FIXTURE_*`
  constants (`tests/nanokernel/src/lib.rs:184–229`) and the lib.rs
  self-test referencing it (~line 375)
- the `"framebuffer_fixture"` entry in the build script's `PROGRAMS` list
  (`tests/nanokernel/build.rs:20`; `rerun-if-changed` covers the asm dir,
  so rebuild mechanics are automatic)
- `tests/nanokernel/tests/elf_shape.rs`:
  `assert_guest_shape("framebuffer_fixture", ...)` (line 61),
  `framebuffer_fixture_asm_matches_rust_constants` (lines 425–497), and the
  `"framebuffer_fixture.asm"` string entry in
  `channel_guest_asm_ring_descs_match_the_constant` (line 511) — **this
  last one produces no compile error and fails only at test time**; don't
  rely on the compiler to find it
- service.rs consumers: `framebuffer_fixture_machine_config` (line 5197),
  `framebuffer_fixture_pixels` (line 5240), test at line 7074

If you find a reason to keep a second fixture (e.g. exercising a *different*
layout_version end-to-end), repurposing it is acceptable — but don't keep a
descriptor-bearing region anywhere.

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
and 5633 — compile errors from the removed helpers point at *most* sites,
but NOT all: `capture_epoch_leg` (the `IcountBudget(100_000)` landmine
above) and the elf_shape.rs line-511 string entry fail only at test time.
Re-check every `fb_info`/`fb_lz4` expectation against the new contract.

The zeroed-region ("black frame") case is covered at unit level only: the
capture fixture always pattern-fills before publishing its manifest, so
there is no end-to-end zero-frame window. That satisfies the request's
"worker regression tests" wording — but do not claim end-to-end zeroed
coverage in the handback.

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

- `crates/dh-worker/tests/m6_full_api_uds.rs` — **resolved by review: no
  framebuffer-assertion changes needed.** `capture_spec()` sets
  `framebuffer: false` (line 135), so the fb-empty assertion (line 507) and
  the leg-digest hash of `fb_lz4` (line 563 — cross-leg comparison, not a
  pinned golden) are unaffected, and `expected_capture_bytes()` auto-adapts
  via `CAPTURE_FIXTURE_FB_BYTES`. This test is KVM + 64-core gated; if this
  host can't run it, say so explicitly in the handoff rather than claiming
  it passed.
- `tests/nanokernel/tests/capture_manifest_interop.rs` derives everything
  from `CAPTURE_FIXTURE_FB_BYTES` and should pass unedited after the resize
  — run it to confirm.
- Consumer sweep already done by both reviewers: `fb_info`/`fb_lz4`/
  `CaptureOutput` appear only in `service.rs` and `m6_full_api_uds.rs`;
  golden/hash tests (`entr_golden.rs`, `dh-snapshot`/`dh-inputlog` goldens,
  snapshot_engine, determinism-tests) reference none of this. A final
  confirming grep before declaring done is still cheap:
  `grep -rln "fb_info\|fb_lz4\|CaptureOutput" crates/ tests/`.
- Stale doc wording (review nit): `docs/ops/m6-grpcurl-metrics-smoke.md:146`
  says "The landing-loop fixture has no framebuffer descriptor" — behavior
  is unchanged (FailedPrecondition, no region) but reword away from
  "descriptor" while touching docs.

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
