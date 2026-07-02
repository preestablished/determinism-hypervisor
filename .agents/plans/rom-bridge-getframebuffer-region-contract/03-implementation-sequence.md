# Implementation Sequence (`crates/dh-worker/src/service.rs`)

Work in this order; each step compiles (tests may be red until step 6 /
file 04 lands — that is expected, land it all as one logical unit).

## Step 1 — Add the layout table

Add `FramebufferLayout`, `FRAMEBUFFER_LAYOUT_V1`, and
`framebuffer_layout(layout_version)` per `02-contract-and-decision.md`, near
the other framebuffer constants (~line 100). Delete
`FRAMEBUFFER_DESCRIPTOR_BYTES` at the end (step 5) once nothing references it.

## Step 2 — Thread `layout_version` out of region resolution

`read_framebuffer_region_from_bus` (line 2644) already holds the resolved
region at line 2666. Change its return type from `Result<Vec<u8>, Status>` to
carry the layout version, e.g.:

```rust
struct FramebufferRegionBytes {
    layout_version: u32,
    bytes: Vec<u8>,
}
fn read_framebuffer_region_from_bus(
    bus: &mut dh_devices::MmioBus,
    caller: FramebufferCaller,
) -> Result<FramebufferRegionBytes, Status>
```

(A named struct beats a tuple here; two callers.) Everything else in the
function — flag scan, resolve, `checked_capture_len` against
`MAX_CAPTURE_FRAMEBUFFER_BYTES`, `read_region` — stays.

Callers to update: `read_framebuffer_from_bus` (line 2682) and the
`capture.framebuffer` branch of `capture_at_boundary` (line 2857).

## Step 3 — Replace the response builder

Replace `framebuffer_response_from_region_bytes(region)` (line 2690) with a
layout-keyed version, keeping the return shape so `get_framebuffer` (line
4504) barely changes:

```rust
fn framebuffer_response_from_region(
    layout_version: u32,
    region: &[u8],
) -> Result<(u32, u32, u32, i32, Vec<u8>), Status>
```

Logic:
1. `framebuffer_layout(layout_version)` → on `None`, FailedPrecondition
   naming the version (and the known set).
2. `expected = layout.stride as u64 * layout.height as u64`; if
   `region.len() as u64 != expected`, FailedPrecondition naming
   layout_version, expected, and actual. Note the check is on the **full
   region length**, exact equality — D7 publishes exactly stride×height and
   the manifest `len` drove the read, so `region.len()` is `region.len` from
   the manifest. (Deliberate choice: exact match, not `>=`; the request
   words it as "length is not 229,376 bytes".)
3. Return `(layout.width, layout.height, layout.stride,
   proto_pixel_format(layout.format), region.to_vec())`.

No zero-dimension / stride / truncation branches remain — the table is
statically valid. Keep the function pure (no bus access) so unit tests can
feed byte vectors, as today's tests do.

## Step 4 — Fix the capture path

- Delete `framebuffer_region_advertises_descriptor` (line 2763).
- Replace `descriptor_framebuffer_capture` (line 2742) with a non-optional
  builder, e.g.
  `framebuffer_capture(layout_version, region, frame_counter) ->
  Result<(Vec<u8>, proto::FbInfo), Status>` that calls
  `framebuffer_response_from_region` and wraps the geometry in `FbInfo`
  (keeping the passed-in `frame_counter`).
- In `capture_at_boundary` (line 2856–2874): the `match .. { Some | None }`
  collapses — always compress the returned pixels and set the returned
  `fb_info`. The zero-`FbInfo` fallback branch is deleted. Errors now
  propagate out of `Run`/`TakeSnapshot` as FailedPrecondition, which is the
  request's explicit ask (no silent wrong geometry).

## Step 5 — Sweep dead code

Remove `FRAMEBUFFER_DESCRIPTOR_BYTES` and any now-unused helpers. `cargo
clippy -p dh-worker` should be clean. `grep -n descriptor` through the
framebuffer section of service.rs to make sure no stale comments describe
the old parse; also update the comment in
`tests/nanokernel/asm/framebuffer_fixture.asm` / its lib.rs doc comments as
part of the fixture rework (file 04).

## Step 6 — Tests and fixtures

See `04-tests-and-fixtures.md`. Land code + fixture + test changes as one
logical unit (single commit or small stack) so the tree is never red.

## Step 7 — Docs, beads, handback

See `05-docs-beads-closeout.md`.

## Notes / Traps

- `get_framebuffer` handler (line 4475): only the call at 4504–4505 changes
  (unpack the new struct from step 2, pass `layout_version` through). The
  pause-drain, frame_counter, and response assembly are untouched.
- `m9_handoff.rs` has uncommitted local modifications in the working tree
  (`git status` shows `M crates/dh-worker/src/m9_handoff.rs` and
  `Cargo.lock`) that are unrelated to this request. Do not revert or absorb
  them blindly — `git diff` first; keep your commits scoped to the
  framebuffer change, and leave those hunks out unless they turn out to be
  yours to finish (check `bd list --status=in_progress` for context).
- Determinism sensitivity: `capture_at_boundary` output (`feature_bytes`,
  `fb_lz4`, `fb_info`) feeds snapshot capture in `TakeSnapshot`. Changing
  `FbInfo` for raw regions from zeros to contract geometry can change
  snapshot artifact bytes wherever `capture.framebuffer` is used. Nothing in
  the repo's golden/entr tests is known to pin the old zero-FbInfo bytes,
  but per process memory: 3+ consecutive full workspace runs before merge,
  and investigate any hash mismatch rather than re-rolling goldens blindly.
