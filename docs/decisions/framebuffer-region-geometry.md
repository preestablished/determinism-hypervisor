# Framebuffer Region Geometry Is Keyed By layout_version, Not In-Region Bytes

Status: accepted, 2026-07-02.
Context: `.agents/requests/rom-bridge-getframebuffer-region-contract/` and
plan `.agents/plans/rom-bridge-getframebuffer-region-contract/`.

## Context

The reference-workload D7 contract (determinism docs,
`reference-workload/ARCHITECTURE.md` §1 D7) defines the published
`framebuffer` region as raw pixels only: XRGB8888, 256×224, row-major,
stride 1024 B — 229,376 bytes, `layout_version 1`, with **no in-region
header**. The detguest-wire manifest entry carries no geometry fields, so
the layout version is the only channel through which geometry is
communicated.

`dh-worker` instead parsed the region's first 16 bytes as a
`width|height|stride|format` descriptor (`GetFramebuffer`) and, on the
`CaptureSpec.framebuffer` path, ran a heuristic
(`framebuffer_region_advertises_descriptor`) over those bytes to decide
whether a descriptor was present. That expectation traces (`git log -S
FRAMEBUFFER_DESCRIPTOR_BYTES`) to ralph iteration 131/133 review fixes with
no decision record and no guest-side counterpart; D7 predates it and every
shipped guest (reference workload, the rom-bridge-o73 READY snapshot)
conforms to D7. Against a conforming guest, `GetFramebuffer` failed on
every call (pixel bytes parsed as a descriptor), and capture metadata was
frame-content-dependent: whether the heuristic classified the region as
descriptor-bearing depended on the pixel values of the current frame,
making `FbInfo` non-reproducible.

## Decision

- Framebuffer geometry is a worker-side table keyed by the manifest
  entry's `layout_version` (`framebuffer_layout` in
  `crates/dh-worker/src/service.rs`). `layout_version 1` = XRGB8888,
  256×224, stride 1024, 229,376 bytes.
- Unknown layout versions, and known versions whose region length differs
  from the contract length, are rejected with `FailedPrecondition` naming
  the offending layout_version or the expected/actual lengths — on both
  the `GetFramebuffer` and `CaptureSpec.framebuffer` paths.
- No known layout has an in-region descriptor; the 16-byte header parse
  and the capture-path heuristic are deleted. A future layout that
  specifies an in-region header would be introduced as a new
  layout_version with its own table entry.
- An all-zero region is a valid black frame, not an error; "no frame
  completed yet" is expressed by `frame_counter == 0` (pv-pad), which is
  unchanged.
- The geometry constants live in `dh-worker` (citing D7), not in
  `detguest-wire`: sharing them would span the guest-sdk repo and is not
  required by the behavioral contract.
- `proto::PixelFormat::Rgb565` remains proto-only until some layout
  version defines it; no guest publishes it.

## Consequences

- Adding a framebuffer layout means adding a table entry (and only that).
- `Run`/`TakeSnapshot` with `CaptureSpec.framebuffer` against a
  non-conforming region now fail with `FailedPrecondition` instead of
  silently emitting zero-geometry `FbInfo` + raw bytes. A failed-capture
  `Run` has already executed the guest, so the caller loses the
  `RunResponse` even though guest state advanced — the same shape as the
  pre-existing missing-region capture errors. Capture output is
  RPC-payload-only; snapshot artifacts and their hashes are unaffected.
- The nanokernel `capture_fixture` framebuffer region was resized to the
  D7 length (229,376 B) so it remains accepted; the descriptor-bearing
  `framebuffer_fixture` (bead 02r), which existed solely to feed the
  deleted parse, was removed.
