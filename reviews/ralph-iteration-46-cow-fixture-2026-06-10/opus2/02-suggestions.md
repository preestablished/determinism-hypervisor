# Suggestions

## 02.S1 — Expose the byte-formula constants as named `pub const`s for the asm consumer

`tests/nanokernel/src/image.rs:51-58` (base) and `:84-91` (overlay)

The formulas use inline magic literals: base `(sector*167 + i*13 + 5) & 0xFF`,
overlay `(sector*89 + i*31 + 11) & 0xFF`. The doc claims these are "asm-cheap to
spot-check from asm" — true, and I confirmed the math (two `imul`s + adds, the `&
0xFF` is free since the result is truncated to `u8`). But the eventual M1
device-exercise guest (bead 40q) must reproduce these formulas in asm to verify reads
and produce writes. Today the implementer has to copy the literals by hand, which is
exactly the kind of drift the repo's `bootinfo.inc` discipline exists to prevent.

Suggest hoisting them to named constants so the guest references the same source of
truth (or at least so a future `bootinfo.inc`-style mirror has named anchors):

```rust
pub const BASE_MUL: u64 = 167;
pub const BASE_STRIDE: u64 = 13;
pub const BASE_BIAS: u64 = 5;
pub const OVERLAY_MUL: u64 = 89;
pub const OVERLAY_STRIDE: u64 = 31;
pub const OVERLAY_BIAS: u64 = 11;
```

Low effort, pure win for the downstream consumer. Not blocking.

## 02.S2 — Document the implicit `BASE_IMAGE_SECTORS % BATCH == 0` invariant

`crates/dh-vmm/tests/blk_fixture.rs:60` (and the post-write loop at :96)

`BATCH = 64` and `BASE_IMAGE_SECTORS = 2048` divide evenly, so every batch is full
and the inner `for sec in first..first + BATCH` loop never reads a sector past
capacity nor a guest-buffer offset past the written region. This is correct *because*
2048 % 64 == 0 — if `BASE_IMAGE_SECTORS` were ever changed to a non-multiple of
`BATCH`, the final batch's `request(... BATCH ...)` would return STATUS_BAD_REQUEST
(end_sector > capacity) and the assertion loop would also index past the data just
read. A one-line comment or a `debug_assert_eq!(image::BASE_IMAGE_SECTORS % BATCH,
0)` near the `const BATCH` would make the assumption explicit and fail loudly if the
image size changes.

## 02.S3 — Guard temp-file cleanup against mid-test panics

`crates/dh-vmm/tests/blk_fixture.rs:115,134,159` and
`tests/nanokernel/src/image.rs:170`

Cleanup is `std::fs::remove_file(&path).ok()` placed *after* all asserts. If any
assert panics mid-test the temp file leaks under `/tmp` (e.g.
`dh-ws4-<pid>-cow.img`). Not a correctness issue and the existing `blkfile.rs` tests
use the same pattern, so this is consistent — but for a fixture file up to 1 MiB,
repeated failing CI runs accumulate cruft. A tiny RAII drop-guard (or `tempfile`
crate if already a dep elsewhere) would make cleanup panic-safe. Low priority; note
only because the file is larger than the other tests' temps.

## 02.S4 — `base_image()` allocates the whole 1 MiB on every call

`tests/nanokernel/src/image.rs:60-67`

`write_base_image`, the consumption test's full-image reads, and the
`generator_is_deterministic_and_hash_gated` test each call `base_image()`, building a
fresh 1 MiB `Vec` each time. Fine for tests (millisecond-scale, and `Vec::with_capacity`
already avoids reallocs). Flagged only so it is not later mistaken for a hot path: if
a future runner streams the image, prefer a sector-at-a-time writer over materializing
the whole buffer. No change needed now.

## 02.S5 — `host_io_errors` is unobservable in the fixture path (informational)

`crates/dh-vmm/tests/blk_fixture.rs`

The fixture's `FileBase` over a healthy temp file never returns `BaseIoError`, so the
`STATUS_HOST_IO` / `host_io_errors` run-control path is not exercised here. That is
correct scoping for a CoW-contract fixture (host-fault injection is covered by
`blk.rs::dead_base_surfaces_host_io_and_counts`). No action — just noting the
boundary so a future reader does not expect host-fault coverage from this test.
