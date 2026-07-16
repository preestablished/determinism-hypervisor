# Action Items

### Critical

_None._

### Important

_None._

### Suggestions

- [ ] [tests/nanokernel/src/image.rs:51-58,84-91] Hoist the byte-formula magic
  literals (base 167/13/5, overlay 89/31/11) to named `pub const`s
  (`BASE_MUL`/`BASE_STRIDE`/`BASE_BIAS`, `OVERLAY_*`) so the eventual M1
  device-exercise guest (bead 40q) can reference the same source of truth instead of
  copying literals into asm.

- [ ] [crates/dh-vmm/tests/blk_fixture.rs:60] Make the implicit
  `BASE_IMAGE_SECTORS % BATCH == 0` invariant explicit — add a comment or
  `debug_assert_eq!(image::BASE_IMAGE_SECTORS % BATCH, 0)` near `const BATCH = 64` so
  a future image-size change cannot silently turn the final batch into a
  STATUS_BAD_REQUEST / out-of-range index.

- [ ] [crates/dh-vmm/tests/blk_fixture.rs:115,134,159; tests/nanokernel/src/image.rs:170]
  (Optional) Make temp-file cleanup panic-safe with an RAII drop-guard so a failing
  assert does not leak up-to-1-MiB fixture files under /tmp across CI runs.

- [ ] [tests/nanokernel/src/image.rs — follow-up bead] File a bead for a host-runnable
  production entry point (e.g. a `dh-cli image` subcommand calling
  `image::write_base_image`). Bead ws4 says "script/build step producing the image"
  and "HOST-RUNNABLE production"; today production is a lib fn with no CLI/script
  entry. Acceptable for now because the drift-gated `BASE_IMAGE_BLAKE3` constant is
  the actual MachineConfig input, but the dedicated-runner flow will likely want the
  file materialized by a runnable step. (NEEDS_DISCUSSION — see below.)

- [ ] [bead 40q] Add a note to determinism-hypervisor-40q (M1 acceptance) that the
  read-verification patterns and write set now live in `nanokernel::image`
  (`base_sector`, `overlay_sector`, `OVERLAY_WRITES`, `expected_sector_after_writes`)
  so the device-exercise guest consumes these fixtures rather than inventing its own.

### Needs Discussion

- [ ] Lib-fn production vs build-artifact: confirm with the team that a
  `write_base_image()` lib fn (no build.rs artifact, no CLI) satisfies ws4's
  "script/build step" intent. The hash constant is what MachineConfig consumes, so
  correctness does not depend on an on-disk artifact — but if the dedicated-runner /
  worker flow expects the image file produced by a discrete CLI step, that gap should
  be tracked as a follow-up bead (see suggestion above) rather than left implicit.
