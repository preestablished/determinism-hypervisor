# Requested Change

## What We Need (Behavioral)

`GetFramebuffer` on a lease whose guest publishes a `layout_version 1`
framebuffer region (the D7 contract) must return:

```text
width         = 256
height        = 224
stride        = 1024
format        = XRGB8888
frame_counter = pv-pad FRAME_COUNTER (unchanged)
icount        = cumulative icount   (unchanged)
pixels        = the full raw region bytes (stride × height = 229,376)
```

It must succeed on a freshly restored, never-run slot (an all-zero
framebuffer is a valid black frame, not an error). "Guest has not completed a
frame yet" is expressible via `frame_counter == 0`; the bridge handles that.
We do **not** require `frame_counter > 0` after `Run{FrameBudget(1)}` for
acceptance — the counter staying 0 there is under separate investigation on
our side — only that the RPC succeeds and returns the region bytes.

For a framebuffer region with any other `layout_version`, or a
`layout_version 1` region whose length is not 229,376 bytes, return
`FailedPrecondition` with a message naming the offending layout_version or
length. (The proto's `RGB565` currently corresponds to no defined layout; no
guest publishes it.)

The same layout-version-derived geometry must apply to the
`CaptureSpec.framebuffer` path used by `Run`/`TakeSnapshot`
(`descriptor_framebuffer_capture` and its
`framebuffer_region_advertises_descriptor` heuristic — see
`02-root-cause.md`). Against a `layout_version 1` region, captured `FbInfo`
must carry the contract geometry deterministically; heuristic classification
of pixel bytes must not survive for known layout versions, since it makes
capture metadata frame-content-dependent and non-reproducible.

## Suggested Approach (Yours To Overrule)

Derive geometry from the region contract keyed by the manifest entry's
`layout_version`, not from in-region bytes:

1. Define the `layout_version 1` framebuffer geometry constants
   (256/224/1024/XRGB8888/229,376) once. Note: sharing them via
   `detguest-wire` spans a **third repo** (guest-sdk; this repo consumes it
   by path dependency). If changing guest-sdk is out of scope, defining the
   constants locally in `dh-worker` with a comment citing D7 fully satisfies
   the behavioral ask — sharing is a nice-to-have, not a requirement.
2. Select geometry by the resolved region's `layout_version`; validate
   `region.len` against the expected size; reject unknown layout versions.
   Plumbing note: `manifest.resolve(name)` already returns a
   `ResolvedRegion` carrying `layout_version` at the point
   `read_framebuffer_region_from_bus` calls it, but the function currently
   returns bare `Vec<u8>` and discards it — the layout version needs
   threading through to both `framebuffer_response_from_region_bytes` and
   the `descriptor_framebuffer_capture` caller.
3. Delete the 16-byte header parse and the capture-path descriptor heuristic
   (or keep them only behind a future layout_version that actually specifies
   an in-region header).

If you would rather keep an in-region descriptor, that is a guest-visible
contract change: it must be specified (D7 + a decision record), implemented
in the workload harness, and shipped in a new READY snapshot before it can
work — and the deployed rom-bridge-o73 snapshot would need regenerating.
That is a much longer path to the same unblock; we would ask for the
layout-version approach unless there is a strong reason otherwise.

## Acceptance Criteria

Verified by you (worker regression tests):

1. A raw-pixel `layout_version 1` region with no header: zeroed variant
   returns a black frame (not an error) and nonzero variant returns the
   pixels, both with contract geometry.
2. Unknown layout_version and wrong-length regions are rejected with a
   `FailedPrecondition` naming the offender.
3. `CaptureSpec.framebuffer` against a `layout_version 1` region emits
   contract-geometry `FbInfo` regardless of pixel content (zeroed and
   nonzero variants).

Verified by us, after the fix reaches the deployed worker:

4. Against a slot restored from the rom-bridge-o73 READY snapshot,
   `GetFramebuffer` returns `XRGB8888 256×224 stride 1024` with 229,376
   pixel bytes — both before any `Run` (black frame) and after
   `Run{FrameBudget}`.

## Deployment And Handback

Reply with (or note in this directory) the `main` commit SHA containing the
fix. **We will rebuild and restart the deployed `dh-workerd` ourselves** using
the command line and pid-file procedure from
`docs/ops/rom-bridge-o73-ready-snapshot.md` — a worker restart invalidates
in-memory bridge leases (see `04-related-slot-leak.md`), so we need to own
the timing after stopping any active bridge session. The bridge side needs no
code change: once `GetFramebuffer` succeeds, `/api/frame/current` and the UI
preview work as-is.

## How We Will Verify

From the bridge repo, with a real session:

```sh
curl -s -o /dev/null -w '%{http_code}\n' \
  -H 'Origin: http://tailrombridge.birb.homes' -b "$SESSION_COOKIE" \
  http://tailrombridge.birb.homes/api/frame/current   # want: 200
```

plus the browser preview rendering a frame. The bridge logs every worker
RPC failure (`journalctl -u rom-operator-bridge`, WARN level, includes gRPC
code + message), so any residual mismatch will be visible immediately.
