# Resolution

Fixed on `main`: **`5698d7e66676cd94199d72eabe666e8edf7601ba`**
("Derive framebuffer geometry from D7 layout_version contract", 2026-07-02).
Bead: `determinism-hypervisor-ps5z`. Decision record:
`docs/decisions/framebuffer-region-geometry.md`.

## What Changed

Your suggested layout-version approach, adopted as-is. `dh-worker` now
derives framebuffer geometry from a table keyed by the manifest entry's
`layout_version` (v1 = XRGB8888, 256×224, stride 1024, 229,376 bytes — D7).
The 16-byte in-region descriptor parse is deleted from `GetFramebuffer`, and
the `framebuffer_region_advertises_descriptor` heuristic is deleted from the
`CaptureSpec.framebuffer` path used by `Run`/`TakeSnapshot` — captured
`FbInfo` now carries contract geometry deterministically, regardless of
pixel content. An all-zero region is a valid black frame; `frame_counter`
(pv-pad) and `icount` are unchanged.

## New Error Shapes (you log worker errors verbatim)

Both are `FailedPrecondition`; the prefix is `GetFramebuffer` or
`CaptureSpec.framebuffer` depending on the path:

```text
GetFramebuffer framebuffer region layout_version <v> is not supported (known: 1)
GetFramebuffer framebuffer region layout_version 1 expects 229376 bytes, got <n>
```

## Behavior Changes Beyond The Ask

- `Run`/`TakeSnapshot` with `CaptureSpec.framebuffer` against an
  unknown-version or wrong-length region now **error** instead of silently
  succeeding with zero-geometry `FbInfo`. A failed-capture `Run` has already
  executed the guest, so the caller loses the `RunResponse` even though
  guest state advanced (same shape as the pre-existing missing-region
  capture errors; the slot stays paused).

## Verification Done Here

- Acceptance criteria 1–3: unit regression test
  `framebuffer_layout_contract_is_enforced` (zeroed → black frame, nonzero
  → pixel round-trip, unknown-version and wrong-length rejections naming
  the offender, capture geometry identical for zeroed vs nonzero regions).
- End-to-end on KVM (this host): `GetFramebuffer` and
  `Run{capture.framebuffer}` against the nanokernel capture fixture
  (resized to the D7 length) return 256×224×1024 XRGB8888 with the full
  229,376 region bytes; the C5 capture-neutrality acceptance test passes.
- 3 consecutive full-workspace `--release` test runs, 639 tests each, all
  green, gated on cargo exit codes.
- **Not run**: the 64-core-gated `m6_full_api_uds` integration suite (this
  host has 2 cores). Its capture spec uses `framebuffer: false` and its
  expectations derive from the fixture constants, so no assertions there
  needed changes.
- Note: the zeroed/black-frame case is covered at unit level, not
  end-to-end (no fixture publishes a zero-filled region); your criterion 4
  check against the freshly restored READY snapshot is the end-to-end
  black-frame test.

## Deployment (yours, per your ask)

Rebuild and restart the deployed `dh-workerd` yourselves from `main` at the
SHA above, using the command-line and pid-file procedure in
`docs/ops/rom-bridge-o73-ready-snapshot.md`. We have not touched the
deployed worker, its pid file, or `/run/dh/grpc.sock`. Remember the restart
invalidates in-memory bridge leases (your `rom-operator-bridge-72o`), so
stop any active bridge session first.

One factual correction to `02-root-cause.md` for your notes: an all-zero
(black) D7 frame did not silently emit zero-FbInfo — `PfUnspecified`
(format 0) counted as a "known format", so black frames failed loudly with
"descriptor has zero dimensions"; the silent zero-FbInfo fallback required
an unknown format AND implausible dimensions. Either way the metadata was
frame-content-dependent, which is the defect that is now fixed.
