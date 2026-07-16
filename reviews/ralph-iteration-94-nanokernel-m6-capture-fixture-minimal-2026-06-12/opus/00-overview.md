# Code Review — Overview

- **Branch:** `ralph/iteration-94-nanokernel-m6-capture-fixture-minimal`
- **Base:** `main`
- **Date:** 2026-06-12
- **Reviewer:** Claude Opus
- **Commit:** `8ad61b8` — "ralph: iteration 94 checkpoint - M6 capture fixture guest (4ws)"
- **Stats:** 5 files changed, +469 / -0, 1 commit

## Summary

This iteration adds `capture_fixture`, the M6 capture-engine fixture guest — the only
region-manifest producer until the guest-SDK lands in Phase 3. The x86-64 NASM guest
(`capture_fixture.asm`) fills a 64 KiB framebuffer at GPA 0x60_0000 with a known
`FB_QWORD_BASE + j` qword pattern, publishes a minimal detchannel region manifest at
channel offset 0x1000 carrying one FRAMEBUFFER-flagged region, parses a bumpable
`layout_version` from the BootInfo cmdline's leading decimal digits (same parse contract
as `landing_loop`), then `CHANNEL_INIT`s via PIO detcalls and emits serial `FDX` on full
success. The change is well-scaffolded: a new `capture_fixture_elf()` accessor plus mirror
constants in `src/lib.rs`, a drift-pin test (`capture_fixture_asm_matches_rust_constants`)
that ties the asm `%define`s back to `detguest-wire` truth rather than re-typed literals,
and a genuinely valuable host-runnable interop test (`capture_manifest_interop.rs`) that
rebuilds the channel page byte-for-byte and runs the *real* `detguest-host`
`Channel::attach` / `read_manifest` / `resolve` / `read_region` over it. I verified the asm
correctness end-to-end (register clobbers across `rep movsb`, the `cld` in crt0 guaranteeing
DF=0, NASM imm32 sign-extension for the qword stores, the `0x00400001` version/capacity
pack, the `loop` rcx usage, and all manifest field offsets against `detguest-wire`), ran the
new tests (3 interop + the drift pin, all pass), and confirmed the framebuffer is clear of
the channel page. The wire bytes are correct and the interop test faithfully mirrors the
asm. The only finding of substance is a non-fatal drift-pin gap on the channel *header*
ring-desc literals (a pre-existing pattern shared with `device_exercise`), plus a couple of
minor consistency nits versus `landing_loop`.

## Verdict

**APPROVE**

No critical or blocking issues. The framebuffer pattern, manifest layout, and PIO sequence
are correct; the interop test exercises the real host codec and passes. The findings below
are one Important hardening gap (header drift pin) and minor suggestions, none of which
block merge for a hardware-gated fixture.
