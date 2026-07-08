# 02 — The Capture-Engine Proof: Harness, Surfaces, Checks

> **POST-EXECUTION CORRECTION (2026-07-08, dual review):** Step 2's
> independence plan was over-optimistic in two ways. (1) The proposed
> "second independent read path" (snapshot-file section read) was NOT
> implemented — the test has no out-of-band source for a region GPA
> (there is no manifest RPC), so a raw-GPA read would need fragile
> hardcoded addresses; this was disclosed in the evidence per the "if
> neither second path is practical, say so" clause below. (2) The
> fallback of "rely on the framebuffer semantic check as the common-mode
> canary" is WRONG: `GetFramebuffer`/the capture framebuffer read ALSO
> go through `Channel::read_region`, so the framebuffer check is not
> independent of `read_region` either. The delivered proof is therefore
> a *common-mode* proof that the engine correctly USES `read_region`
> (packing, offsets, layout gating, geometry+lz4, surface equivalence,
> restore identity); `read_region`'s own correctness rests on
> detguest-host's tests. This is stated plainly in the test docstring
> and evidence README.

The deliverable is a lab-lane integration test (plus its evidence
output) proving the Phase-3 capture engine end-to-end against the real
workload image. The engine already has *fixture-level* unit coverage in
`crates/dh-worker/src/service.rs` tests (`capture_fixture_spec`,
`take_snapshot_capture_checks_layout_version_and_returns_features`
~:8558, `capture_size_limits_reject_oversized_lengths` ~:6689,
`capture_neutrality_leg` ~:6495) and a real-image *streaming* test
(`frame_capture_stream.rs:153`,
`linux_streaming_capture_is_neutral_complete_and_backpressure_safe`).
What has never happened — the gap this closes — is a compiled
extraction list from the demo feature map run against the **real
image's** region manifest with output cross-checked against independent
reads.

## Code Map (Verified 2026-07-08 — Re-Verify Line Numbers At Execution)

- **Engine core:** `capture_at_boundary`
  (`crates/dh-worker/src/service.rs:3231`) — shared by both surfaces.
  Per-range `layout_version` check vs the manifest at `:3278`
  (FAILED_PRECONDITION, message names the range index and versions —
  the negative-case assertion target). Framebuffer geometry is keyed by
  the manifest's `layout_version` (`framebuffer_layout`, `:3081–3104`);
  version 1 is the D7 contract (raw `xrgb8888` pixels, 229,376 bytes).
- **Surface 1 — Run-with-capture:** `RunRequest.capture = 7`
  (`proto/hypervisor.proto:202`, "extract at the stop");
  `RunResponse.feature_bytes = 7` / `fb_lz4 = 8` (`:238–239`). Service
  call site around `service.rs:3921`.
- **Surface 2 — TakeSnapshot-with-capture:**
  `TakeSnapshotRequest.capture = 3` (`proto:260`);
  `TakeSnapshotResponse.feature_bytes = 9` / `fb_lz4 = 10` (`:276–277`).
  Handler `take_snapshot` (`service.rs:4796`), capture at `:4828`.
- **Independent read surface:** `ReadGuestMemory` RPC
  (`service.rs:5010`; `proto:290–295` `RegionRange {region,
  layout_version, offset, len}` — the proto comment explicitly frames
  it as the non-capture read path, resolved through the same guest-sdk
  region manifest). It performs its own `layout_version` check
  (`service.rs:5079`).
- **Fork/restore:** `restore_snapshot` RPC (`service.rs:4426`), `fork`
  RPC (`service.rs:4530`; proto `ForkRequest` `:138`, tier-A CoW
  children). The manifest is guest RAM, so it is immediately valid and
  re-resolved after restore (`detguest-host/src/manifest.rs:68`) — a
  CaptureSpec re-resolves cleanly on a restored child. Closest existing
  template for check (c): `capture_neutrality_leg` (`service.rs:6495`),
  which already restores/forks and asserts capture doesn't perturb the
  child; driving patterns also in `crates/dh-worker/tests/fork_engine.rs`,
  `restore_engine.rs`, `m7_fork_verify.rs`.
- **Packing/encoding facts:** `feature_bytes` total is capped at
  `MAX_CAPTURE_FEATURE_BYTES` = 16 MiB (`service.rs:102`); the
  framebuffer is `lz4_flex::compress_prepend_size` — decode with
  `lz4_flex::decompress_size_prepended` (as the fixture test
  `run_capture_spec_reads_manifest_ranges_and_lz4_framebuffer`
  ~`:7945` does). `RunWithFrameCapture` has NO CaptureSpec surface —
  it is the separate per-FRAME_MARK framebuffer stream; don't conflate
  the two.
- **Lab-lane scaffolding:** `crates/dh-worker/tests/common/mod.rs` —
  `m9_artifacts` (:174), `assert_m9_real_emulator_initramfs` (:248, the
  old-initramfs rejection), `m9_linux_ready_snapshot` (:609, boots the
  real image to READY and hands back a live worker + snapshot),
  `populate_m9_image_cache` (:533). `VmMem: detguest_host::GuestMem`
  (:934) lets the test read guest memory directly through the
  `detguest-host` crate when it holds the VM — the second, deeper
  independence option.

## The Test: `crates/dh-worker/tests/capture_engine_real_image.rs`

New `--ignored` release-mode lab-lane test (M9 pattern — skip cleanly
with the standard message when `DH_M9_*` staging is absent; follow
`frame_capture_stream.rs`'s structure). One test file, legs as separate
`#[test]`s or one sequential fn per the existing suite's style. Boot
once to the READY snapshot via `m9_linux_ready_snapshot` and reuse it
across legs — booting per-leg wastes minutes.

### Step 1 — Compile The Extraction List

Inputs: `reference-workload/feature-maps/demo-game.yaml` (the demo
feature map — offsets are declared placeholders; irrelevant, we prove
byte plumbing, not game semantics) over the real manifest
(`expected-regions.toml`: wram 131072 / framebuffer 229376 / meta 4096,
all `layout_version = 1`).

"Compile" = map each feature's `(region, offset, type width)` to an
`ExtractRange{region, layout_version: 1, offset, len}` in a fixed
order. Hardcode the compiled list in the test as a const table with a
comment citing the map file + rev (do NOT add a YAML parser dependency
to dh-worker for this — compilation is refwork's exporter's job; the
engine proof needs one representative compiled list, not a compiler).
Include the map-derived ranges (`room_id` 0x079B len 2, `area_id`
0x079F len 1, `player_x` 0x0AF6 len 2, `player_y` 0x0AFA len 2,
`health` 0x09C2 len 2 — re-read the YAML at execution time for the full
set) plus edge probes: (wram, 0, 1), a wram tail range ending exactly
at 131072, one ≥256-byte wram range, and one `meta` range. Set
`CaptureSpec{ranges, framebuffer: true}` (`bool framebuffer = 2`,
`proto:100`).

### Step 2 — Check (a): Run-Surface Capture, Cross-Checked

1. From READY, issue `Run` with a modest icount budget and
   `capture` set. Collect `feature_bytes` + `fb_lz4` from the response.
2. Assert `feature_bytes.len() == Σ range.len` and that packing is
   request order (`proto:99`): permute the range order in a second
   capture at the same boundary (e.g. via TakeSnapshot at the same
   paused point) and verify bytes re-order accordingly — this is the
   cheap packing-order proof.
3. Cross-check every range against `ReadGuestMemory` of the same
   `(region, layout_version, offset, len)` at the same paused boundary
   — bit-for-bit. The guest is paused between the Run stop and the
   read, and all three regions are `writable = false` guest-published
   regions; still, do the read immediately after the stop with no
   intervening Run.
4. **Independence caveat, handle honestly:** `ReadGuestMemory`
   explicitly delegates to the same primitive the capture engine uses —
   `detguest_host::Channel::read_region` (proto comment at `:295`) — so
   it alone can't rule out a common-mode reader bug. For at least one
   range, ALSO read through a second path: take a snapshot at the same
   boundary and read the range out of the snapshot file directly
   (`snapshot_section`, common/mod.rs:791), or construct a test-side
   `detguest_host::Channel` over the guest memory via the
   `detguest_host::GuestMem` impl pattern (common/mod.rs:934) with a
   byte-offset read that bypasses `read_region`'s extent walking. If
   neither second path is practical for a live slot, say so in the
   evidence and rely on the framebuffer semantic check (step 3's
   decoded-frame properties) as the common-mode canary. Multi-extent
   note: `read_region` logically concatenates discontiguous extents
   (`detguest-host/src/manifest.rs`, see
   `read_region_stitches_three_discontiguous_extents`) — if the real
   image publishes multi-extent regions, the snapshot-file comparison
   must account for that.

### Step 3 — Check (b): The Framebuffer

Decode `fb_lz4` (lz4-frame or block per the encoder used in
`service.rs` — match the existing decode in `frame_capture_stream.rs`)
and assert exactly 229,376 bytes. Semantic sanity, not just length:
after the boot-to-READY + a short Run, the frame should be non-trivial
— assert it is not all-zero (the fixture tests note all-zero is a
*valid* black frame, so run far enough that the emulator has drawn;
the first-room content from refwork's `vm-first-room` gate guarantees
non-black output once the harness is running). Cross-check the
captured framebuffer bytes against a `ReadGuestMemory` of
(framebuffer, 0, 229376) — decoded fb must equal the raw region read.

### Step 4 — Surface 2: TakeSnapshot-With-Capture

Same spec via `TakeSnapshotRequest.capture` at a paused point;
assert `feature_bytes`/`fb_lz4` in the snapshot response equal the
Run-surface capture taken at the same boundary (pause, capture via
TakeSnapshot, compare against an immediately-preceding Run-stop capture
with zero intervening instructions — or simply cross-check
TakeSnapshot's output against `ReadGuestMemory` the same way as
step 2). Both surfaces must be proven; if one leg is impossible for a
real reason, the resolution must say which and why (the request demands
this explicitly).

### Step 5 — Check (c): Restored/Forked Child Identity

Restore (or `Fork`) a child from the snapshot taken in step 4 — follow
`fork_engine.rs`/`restore_engine.rs` driving patterns. Without running
the child (unchanged state), issue the same capture spec
(TakeSnapshot-with-capture on the child, or Run with zero/minimal
budget if a capture needs a boundary — prefer the path with zero
executed instructions). Assert `feature_bytes` and decoded framebuffer
are bit-identical to the parent's step-4 output.

### Step 6 — Check (d): The Negative Case

Send the same spec with one range's `layout_version` set to 2 on both
surfaces. Assert gRPC `FAILED_PRECONDITION` and that the message names
the offending index and both versions (`service.rs:3278` format). Two
already-proven-on-fixtures semantics to re-assert on the real image:
on the Run surface a capture error is *post-run validation* — the slot
position is committed even though capture failed
(`run_capture_layout_mismatch_commits_successful_run_boundary`
~`:8492`); on the TakeSnapshot surface a failed capture publishes *no
snapshot* (`take_snapshot_capture_checks_layout_version_and_returns_features`
~`:8558`). Record
the proven good version (1) in the evidence — this is the guard that
protects scorer M1 from decoding a stale layout. Optionally also record
the out-of-bounds rejection (`InvalidArgument`, `:2875`) for one
past-end range — cheap and useful, but not an AC.

### Step 7 — Per-Capture Cost

Loop ≥100 captures at a paused boundary (TakeSnapshot-with-capture or
repeated Run-stop captures), timing spec-validate + extract + pack +
lz4 as one number per capture (service-side timing if a metric already
exists; otherwise client-side RPC time minus a no-capture baseline RPC
is acceptable — state the method). Report p50/p95 with and without the
framebuffer. Print to the test log and copy into the evidence per
`03-evidence-and-samples.md`. Not a gate — but if p50 lands near or
above scorer M4's 1.5 ms budget, file the follow-up bead
(`04-closeout.md` §4).

## Invocation And Gates

```bash
DH_M9_BZIMAGE=... DH_M9_INITRAMFS=... DH_M9_BASE_IMAGE=... \
DH_M9_GAME_IMAGE=... DH_M9_IMAGE_CACHE=... \
cargo test -p dh-worker --release --test capture_engine_real_image -- --ignored --nocapture
```

Document the invocation in the module doc (the `rss_regression.rs`
convention). This test is additive: no engine code should need to
change. If it does (a defect), the change is hash-path-adjacent only if
it touches Run/replay flow — apply the 3×-full-workspace-runs rule from
`00-overview.md` ground rule 3 in that case, and re-run the existing
capture-neutrality and record/replay gates regardless before closing.
