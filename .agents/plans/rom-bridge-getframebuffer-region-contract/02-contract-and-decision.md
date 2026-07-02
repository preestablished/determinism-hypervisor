# The New Contract And Why

## Decision

Adopt the request's layout-version approach: framebuffer geometry is a pure
function of the manifest entry's `layout_version`, defined in a table in
`dh-worker`, citing reference-workload ARCHITECTURE §1 D7. Do **not** adopt an
in-region descriptor: that would be a guest-visible contract change requiring
a D7 spec revision, a workload harness change, a regenerated READY snapshot,
and coordinated deployment — a much longer path to the same unblock, with no
benefit (the guest has nothing to say that the layout version doesn't already
encode). Do **not** move the constants into `detguest-wire`: that spans a
third repo (guest-sdk); the request explicitly says local constants with a D7
citation fully satisfy the ask. If sharing later proves useful, file a
follow-up bead — do not block on it.

## The Geometry Table

Exactly one known layout version today:

| layout_version | width | height | stride | format | expected len |
|---|---|---|---|---|---|
| 1 | 256 | 224 | 1024 | XRGB8888 | 229,376 (= stride × height) |

Suggested constants in `service.rs` (names yours to adjust to house style):

```rust
/// Framebuffer geometry keyed by the region's manifest layout_version.
/// layout_version 1 is the reference-workload D7 contract: raw pixels only,
/// XRGB8888, 256x224, row-major, stride 1024 — no in-region header.
/// (determinism docs, reference-workload/ARCHITECTURE.md §1 D7)
struct FramebufferLayout {
    width: u32,
    height: u32,
    stride: u32,
    format: proto::PixelFormat,
}

const FRAMEBUFFER_LAYOUT_V1: FramebufferLayout = FramebufferLayout {
    width: 256,
    height: 224,
    stride: 1024,
    format: proto::PixelFormat::Xrgb8888,
};

fn framebuffer_layout(layout_version: u32) -> Option<&'static FramebufferLayout> {
    match layout_version {
        1 => Some(&FRAMEBUFFER_LAYOUT_V1),
        _ => None,
    }
}
// expected region len = stride as u64 * height as u64 (229_376 for v1)
```

`proto::PixelFormat::Rgb565` remains in the proto but maps to no layout — no
guest publishes it. Do not invent a layout version for it.

## Behavioral Contract (from the request, normative)

### GetFramebuffer, layout_version 1, len == 229,376

Returns `width=256, height=224, stride=1024, format=XRGB8888`,
`pixels` = the full raw region bytes (all 229,376 — stride padding included,
as today's semantics already return `stride*height` bytes),
`frame_counter` = pv-pad FRAME_COUNTER (unchanged), `icount` unchanged.

**Must succeed on a freshly restored, never-run slot.** An all-zero region is
a valid black frame, not an error. `frame_counter == 0` is how "no frame
completed yet" is expressed; the bridge handles it. Acceptance does NOT
require `frame_counter > 0` after `Run{FrameBudget(1)}` — that is under
separate investigation on the bridge side.

### Rejections (both read and capture paths)

`FailedPrecondition`, message **naming the offending value**:

- Unknown layout version, e.g.
  `"framebuffer region layout_version 7 is not supported (known: 1)"`.
- Known layout version, wrong length, e.g.
  `"framebuffer region layout_version 1 expects 229376 bytes, got 65536"`.

Keep the existing caller-context prefixes if house style wants them
(GetFramebuffer vs CaptureSpec — see `FramebufferCaller`); exact wording is
yours, but the layout_version or length figure must appear literally, and
tests should assert on the stable substrings.

### CaptureSpec.framebuffer (Run / TakeSnapshot)

Against a layout_version-1 region: `fb_info` carries the contract geometry
**deterministically, regardless of pixel content** (zeroed and nonzero frames
identical metadata), `fb_lz4 = lz4_flex::compress_prepend_size(pixels)` where
pixels = the full region bytes. Against unknown layout versions or wrong
lengths: the same `FailedPrecondition` as above — capture must not silently
emit zero-geometry `FbInfo` anymore. The heuristic
(`framebuffer_region_advertises_descriptor`) and the `None` fallback branch
in `capture_at_boundary` are deleted, not kept behind a flag: a "future
layout_version with an in-region header" can reintroduce header parsing
keyed by that version if it ever gets specified.

### What deliberately does not change

- Region discovery (`REGION_FLAG_FRAMEBUFFER`, manifest resolve, seqlock
  read) — already correct.
- All non-framebuffer errors (missing device/channel/region, UTF-8 name,
  size cap via `MAX_CAPTURE_FRAMEBUFFER_BYTES`) — keep as-is.
- `capture.ranges` handling, including its existing
  `layout_version` mismatch check at ~line 2831.
- The proto definitions and the `GetFramebufferResponse` shape.
- `frame_counter` sourcing (pv-pad) and `icount`.

## Compatibility Note For The Reviewer

There is no known guest publishing a descriptor-bearing framebuffer region
except the in-repo `framebuffer_fixture` nanokernel (built for bead 02r
precisely to feed the parse being deleted). The deployed READY snapshot, the
reference workload, and the bridge all conform to D7. So this is a
worker-only bugfix converging on the pre-existing contract, not a
compatibility break. Record exactly that in the decision doc
(`05-docs-beads-closeout.md`).
