# Review 1: Contract Correctness / Code-Claim Fidelity

Reviewer: subagent (contract-correctness lens), 2026-07-02, tree at `f58ac28`.

## Verified-correct core claims (no findings)

- All symbol/line refs in `01-current-state.md` check out against the tree:
  `FRAMEBUFFER_DESCRIPTOR_BYTES` at service.rs:108, `read_framebuffer_region_from_bus` 2644,
  `read_framebuffer_from_bus` 2682, `framebuffer_response_from_region_bytes` 2690,
  `descriptor_framebuffer_capture` 2742, `framebuffer_region_advertises_descriptor` 2763,
  `capture_at_boundary` 2784 (framebuffer branch 2856–2874, ranges layout_version check 2831),
  `get_framebuffer` 4475 (call at 4504–4505), Run capture 4047, TakeSnapshot capture 4176–4177,
  `RunWithFrameCapture` unimplemented 4681, tests at 5848/7011/7074/7159 (error assertion
  7226–7233), helpers 5197/5231–5251. Guest-sdk facts confirmed:
  `ResolvedRegion{region_id, layout_version, len, flags, extents}` and `resolve()` at
  `detguest-host/src/manifest.rs:32–47`; `RegionEntry.layout_version` at
  `detguest-wire/src/manifest.rs:137`.
- **Exact-length check is right.** `read_framebuffer_region_from_bus` reads exactly the
  manifest `region.len` bytes (`fb_len = checked_capture_len("framebuffer region",
  region.len, ...)`, `vec![0u8; fb_len]`, `read_region`, service.rs:2669–2678), so
  `region.len()` == manifest len and the plan's exact-equality check against 229,376
  matches the request's wording ("length is not 229,376 bytes").
- **Fixture collisions are real.** `asm/capture_fixture.asm`: `FB_BYTES 0x10000` (line 64),
  `REGION_FLAG_FRAMEBUFFER 1` written into entry flags (line 158), `DEFAULT_LAYOUT_VERSION 1`
  (line 69). `asm/framebuffer_fixture.asm`: `FB_BYTES 144`, 16-byte descriptor 8×4×32/format 1
  written at region start (lines 29–33, 85–88), FRAMEBUFFER flag (line 125), layout_version 1
  (line 39). Both are `layout_version 1` regions that violate the new contract exactly as
  claimed.
- **No hidden capture-output consumers.** Workspace grep: `fb_info`/`fb_lz4`/`CaptureOutput`
  appear only in `service.rs` and `crates/dh-worker/tests/m6_full_api_uds.rs`.
  `snapshot_engine.rs` and `m9_handoff.rs` never touch framebuffer data. m6's
  `capture_spec()` sets `framebuffer: false` (m6_full_api_uds.rs:135) and asserts fb outputs
  empty (line 507–512) — unaffected. `expected_capture_bytes()` slices
  `[CAPTURE_OFFSET..+CAPTURE_LEN]`, so it is length-independent as the plan says; m6
  `MEM = 8 MiB` (line 38) covers the resized region (0x60_0000 + 0x38000 = 0x638000).
- Error-message examples in `02-contract-and-decision.md` ("layout_version 7 is not
  supported (known: 1)", "layout_version 1 expects 229376 bytes, got 65536") literally name
  the offending value — satisfies the request.
- Acceptance-criteria mapping is faithful: plan unit tests 1–5 in `04-tests-and-fixtures.md`
  cover criteria 1–3 (zeroed/nonzero, unknown version, wrong length, capture determinism);
  criterion 4 is correctly deferred to the bridge team with a SHA handback. Nothing dropped
  or weakened; the pv-pad `frame_counter > 0` non-requirement is correctly carried over.

## Findings

**1. Important — `04-tests-and-fixtures.md` (fixture inventory) / `01-current-state.md`
("No other test file references … verified by grep across `crates/`")**
The grep was scoped to `crates/` and missed two consumers under `tests/nanokernel/tests/`:
- `tests/nanokernel/tests/elf_shape.rs` — drift-pin tests parsing the asm `%define`s:
  `capture_fixture_asm_matches_rust_constants` (lines 357–418) pins `FB_BYTES`/`FB_QWORDS`
  and must stay in sync with the resize, and `framebuffer_fixture_asm_matches_rust_constants`
  (lines 425–497) plus the shape assert at line 61 and the file list at line 511 all
  reference the fixture the plan deletes — these must be deleted/updated too.
- `tests/nanokernel/tests/capture_manifest_interop.rs` — uses `CAPTURE_FIXTURE_FB_BYTES`
  throughout (lines 18, 30, 64–157) to build synthetic guest memory and read the region.
Mitigation exists (deleting the constants breaks compilation of these test targets; the
drift tests exist precisely to catch asm/Rust skew, and interop derives everything from the
constant), so nothing fails silently — but the plan's inventory claim is wrong and "most of
the actual work" is the fixture rework, so the implementer should have this list.

**2. Minor — `00-summary.md` ("they are accurate; every code claim in them was re-verified")
endorses a request error**
Request `02-root-cause.md` describes the heuristic as "`width != 0 && height != 0 &&
stride >= width`, no format check" and claims "a black frame reads as 'no descriptor' and
capture silently emits `FbInfo{0,...}`". The actual heuristic (service.rs:2763–2781) is
`known_format || plausible_dimensions`, and `known_format` includes `PfUnspecified` (format
value 0). An all-zero (black) D7 frame therefore has format bytes = 0 → `known_format =
true` → the descriptor parse is attempted → capture **fails loudly** with "descriptor has
zero dimensions", it does not silently emit zero-FbInfo. The zero-FbInfo fallback fires only
when the first 16 bytes have an unknown format *and* implausible dimensions (which the
capture_fixture pattern `0xFB00…` happens to hit — that is why test 7011 sees the fallback).
Plan `01-current-state.md` quotes the heuristic correctly but never flags the request's
misstatement, and the blanket "re-verified, accurate" claim in 00 is false. No
implementation impact (the heuristic is deleted either way), but do not copy the "silently
emits" narrative for black frames into the decision record (`05-docs-beads-closeout.md`
currently frames it only as "frame-content-dependent", which is correct).

**3. Minor — `03-implementation-sequence.md` step 4: "Errors now propagate out of
Run/TakeSnapshot as FailedPrecondition, which is the request's explicit ask"**
Overstated. The request explicitly demands FailedPrecondition rejections for GetFramebuffer
and deterministic FbInfo for known layout versions on the capture path; it never explicitly
says Run/TakeSnapshot must *fail* on unknown-version/wrong-length capture. The plan's choice
is defensible (consistent with "no silent wrong geometry", and no consumer breaks), and the
failure shape is pre-existing (missing-region errors already propagate from the same spot:
service.rs:4047 `?`, no `mark_faulted`, slot stays paused). But note the subtlety for the
decision record/handback: a Run whose capture fails has already executed the guest — the
client loses the `RunResponse` (icount, state_hash, sdk_event) even though guest state
advanced. Callers combining `Run` with a bad `CaptureSpec.framebuffer` see a behavior change
from "silent zero-FbInfo success" to "error after execution". Worth a sentence in
`05-resolution.md`.

**4. Minor — `03-implementation-sequence.md` Notes/Traps: "capture_at_boundary output …
feeds snapshot capture in TakeSnapshot. Changing FbInfo … can change snapshot artifact
bytes"**
Overstated: `take_snapshot_with_lapic` (service.rs:4193) does not consume `CaptureOutput`;
capture output goes only into the `TakeSnapshotResponse`/`RunResponse` RPC payloads, never
into the snapshot artifact, so snapshot refs/hashes are unaffected. The prescribed 3× full-
workspace runs are harmless belt-and-braces, but the implementer should not go hunting for
snapshot-hash regressions that cannot exist via this path. (m6 does hash `snapshot.fb_lz4`
into its transcript at m6_full_api_uds.rs:563, but with `framebuffer: false` it is empty and
stays empty.)

**5. Nit — `02-contract-and-decision.md` ("Keep the existing caller-context prefixes … see
`FramebufferCaller`") vs `03` step 3 signature**
The proposed `framebuffer_response_from_region(layout_version, region)` takes no caller, so
caller-prefixed messages ("GetFramebuffer …" vs "CaptureSpec …") are impossible without
adding a `FramebufferCaller` parameter. Either add the param or accept unprefixed messages;
the plan already tells tests to assert stable substrings, so both work — just resolve the
inconsistency deliberately.

**6. Note — zeroed-region coverage is unit-level only**
Criterion 1's "zeroed variant" is exercised only through the pure-function unit test; the
runtime tests use capture_fixture, whose region is always pattern-filled at boot. That
satisfies the request's "worker regression tests" wording, but if end-to-end black-frame
coverage is wanted, the fixture's cmdline knob only varies layout_version, not fill — a
fresh-boot-before-fill window doesn't exist (fill happens before manifest publish).
Acceptable as planned; just don't claim end-to-end zeroed coverage in the handback.

## Verdict

**Implementable as written.** The plan's code map, contract, error-message shapes,
exact-length semantics, fixture-collision analysis, and acceptance-criteria mapping are all
accurate against the tree, and the fixture strategy (resize capture_fixture to 229,376 B,
delete framebuffer_fixture) is sound. Before starting: (a) extend the fixture-rework
inventory in `04-tests-and-fixtures.md` with `tests/nanokernel/tests/elf_shape.rs` and
`tests/nanokernel/tests/capture_manifest_interop.rs` (finding 1); (b) when writing the
decision record and `05-resolution.md`, use the plan's own "frame-content-dependent" framing
rather than the request's incorrect "black frame silently emits zero FbInfo" claim, and
mention that Run/TakeSnapshot with a bad framebuffer capture now error instead of falling
back (findings 2–3); (c) decide the `FramebufferCaller` question in the new builder
signature (finding 5).
