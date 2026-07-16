# Review Overview — capture_fixture (M6) minimal

- **Branch:** `ralph/iteration-94-nanokernel-m6-capture-fixture-minimal`
- **Base:** `main`
- **Date:** 2026-06-12
- **Reviewer:** Claude Opus (2nd reviewer)
- **Stats:** 5 files, +469/−0, 1 commit (`8ad61b8`)

## Summary

This branch adds the **M6 capture-engine fixture guest** — the only region-manifest
producer in-tree until guest-sdk lands in Phase 3. The NASM guest
(`asm/capture_fixture.asm`) fills a known-pattern 64 KiB framebuffer at GPA
`0x60_0000`, publishes a minimal detchannel region manifest with one
`FRAMEBUFFER`-flagged region named `"framebuffer"`, exposes a cmdline-bumpable
`layout_version`, runs `CHANNEL_INIT`, and emits the serial sequence `"FDX"`. New Rust
constants/accessor in `src/lib.rs` mirror the asm `%define`s; `tests/elf_shape.rs` gains a
drift-pin that cross-checks those mirrors **against the real `detguest-wire` codec**
(magic, offsets, flags) rather than re-typed literals; and `tests/capture_manifest_interop.rs`
is a new HOST-RUNNABLE interop test that builds the channel page byte-for-byte as the asm
does and drives the **real** `detguest-host` `Channel::attach` / `read_manifest` /
`read_region` over it.

I verified the change against the wire truth (`detguest-wire/src/{header,manifest}.rs`),
the host validation paths (`detguest-host/src/{channel,manifest,guestmem}.rs`), the asm
fixtures it extends (`device_exercise.asm`, `landing_loop.asm`, `crt0.asm`), the channel
header layout, the bootinfo ABI, and `dh-devices/src/detchannel.rs` channel_init. I also
**built the guest and ran the new tests**: `cargo test --test elf_shape --test
capture_manifest_interop` passes (10 + 3 tests green), and `cargo clippy --tests` is clean
(no warnings).

The diff is well-conceived and the cross-checking discipline is exemplary. The 63 zeroed
manifest slots are flags=0 ("live" with empty names), but I traced every host path
(`read_manifest` → `validate_extents`, `resolve` → name match) and confirmed none choke on
them. The `rep movsb` direction is safe (crt0 issues `cld`). The interop mock's span exactly
covers the channel base through the framebuffer end and the channel/FB regions are adjacent,
not overlapping. The only items worth raising are minor/non-blocking (scope notes and a
robustness suggestion), not defects.

## Verdict

**APPROVE**

No Critical or Important issues. A handful of non-blocking suggestions (see
`02-suggestions.md`), all of which are reasonable to defer given the explicit "minimal /
until guest-sdk lands" scope.
