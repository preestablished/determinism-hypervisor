# Root Cause: Two Sides Disagree On The Framebuffer Region Layout

## The Worker Side (this repo)

`crates/dh-worker/src/service.rs`:

- `FRAMEBUFFER_DESCRIPTOR_BYTES: usize = 16` (around line 108).
- `read_framebuffer_region_from_bus` (around line 2644) resolves the guest's
  framebuffer region correctly: it reads the detchannel manifest, finds the
  live entry with `REGION_FLAG_FRAMEBUFFER`, resolves it, and copies the
  region bytes. This part is fine.
- `framebuffer_response_from_region_bytes` (around line 2690) then interprets
  the region's **first 16 bytes** as `width u32 | height u32 | stride u32 |
  format u32` (little-endian), validates them, and treats
  `region[16..16+stride*height]` as pixels.

`GetFramebuffer` (around line 4475) and `frame_counter` consumers go through
this path via `read_framebuffer_from_bus` (around line 2682) and fail with a
clean `FailedPrecondition`.

**The capture path is also affected, and worse — it fails silently.**
`CaptureSpec.framebuffer`, used by both `Run` (~line 4047) and `TakeSnapshot`
(~line 4177) via `capture_at_boundary` (~line 2784), does not share
`read_framebuffer_from_bus`; it calls `descriptor_framebuffer_capture`
(~line 2742), which first runs the heuristic
`framebuffer_region_advertises_descriptor` (~line 2763: `width != 0 &&
height != 0 && stride >= width`, no format check) against the region's first
bytes. Against a D7 raw-pixel region that classification is data-dependent
per frame: a black frame reads as "no descriptor" and capture silently emits
`FbInfo { width: 0, height: 0, stride: 0, format: PfUnspecified }` alongside
the raw pixels; live pixel data can read as "descriptor present" and either
fail like `GetFramebuffer` or succeed with corrupted, non-reproducible
geometry. Fixing only `framebuffer_response_from_region_bytes` will not fix
this path — it needs the same layout-version-keyed treatment.
(`RunWithFrameCapture` is currently unimplemented, ~line 4681, so it is a
future consumer, not an active broken path.)

## The Guest Side (reference-workload + guest-sdk)

- Reference-workload ARCHITECTURE **D7** ("Emulated RAM and framebuffer
  published as named regions") specifies the `framebuffer` region as:
  "Last completed frame, XRGB8888, 256×224, row-major, stride 1024 B —
  229,376 B (56 pages)", `layout_version 1`. **No descriptor header.**
  (The doc lives in the determinism project docs tree —
  `~/.agents/projects/determinism/docs/reference-workload/ARCHITECTURE.md` —
  not in the reference-workload checkout itself; the code comments citing
  "ARCHITECTURE.md §1 D7" in `refwork-emu/src/timing.rs` and
  `core_impl.rs` refer to that document.)
- `refwork-emu/src/timing.rs`: `FB_WIDTH = 256`, `FB_HEIGHT = 224`,
  `FB_STRIDE = 1024`, `FB_BYTES = FB_STRIDE * FB_HEIGHT = 229_376`. The
  harness publishes exactly `FB_BYTES` (`refwork-harness/src/regions.rs`,
  `PublishedRegion::new("framebuffer", FB_BYTES)`), all of it pixel bytes.
- `detguest-wire`'s `RegionEntry` (`guest-sdk/crates/detguest-wire/src/manifest.rs`,
  ~line 131) carries `region_id`, `name`, `layout_version`, `flags`, `gva`,
  `len`, and extent bookkeeping — **no geometry fields**.

So the guest has no channel through which it could communicate geometry other
than the region contract itself, and the published region contains no header
to parse.

## Why The Errors Look The Way They Do

The worker reads pixel (0,0) through (3,0) as the descriptor:

- Freshly restored, the framebuffer is zeroed → `width == 0` →
  "descriptor has zero dimensions".
- After the guest runs and blits, the first pixels are nonzero → the fourth
  u32 is arbitrary pixel data → "unsupported pixel_format 496749568".

## Provenance Of The Descriptor Expectation

`git log -S FRAMEBUFFER_DESCRIPTOR_BYTES` traces the header expectation to
ralph iteration commits ("iteration 131/133 review fixes") with no
accompanying decision record in `docs/decisions/` and no matching guest-side
change. The D7 contract predates it and the shipped workload conforms to D7,
so we treat the worker as the divergent side. If there is a newer descriptor
design we missed, `03-requested-change.md` describes what we actually need —
either resolution unblocks us. The layout-version approach is worker-only;
only if you instead adopt an in-region descriptor design would guest and
worker changes need to land together (plus a regenerated READY snapshot).
