# Nanokernel Build Pipeline — Review Overview

- **Branch:** `ralph/iteration-25-nanokernel-build-pipeline`
- **Base:** `main`
- **Bead:** determinism-hypervisor-22n
- **Date:** 2026-06-10
- **Reviewer:** Claude Opus
- **Commit reviewed:** `ca0bdef` (iteration 25 checkpoint)

## Summary

This change adds a new workspace member, `tests/nanokernel`, which is a host-runnable
build pipeline for tiny freestanding x86_64 test guests (ARCH §1, §2.3). A `build.rs`
assembles `asm/*.asm` with `nasm -f elf64` and links each program (shared `crt0.o` plus
`prog.o`) against `link.ld` into a static ELF in `OUT_DIR`. The linker is probed in
portability order: GNU `ld -m elf_x86_64` → `ld.lld` → `lld` → sysroot `rust-lld`
(`-flavor gnu`). The BootInfo ABI is defined once in `include/bootinfo.inc` with Rust
mirrors in `src/lib.rs`, and an integration test parses the `.inc` and fails on drift.
A `pipeline_smoke.asm` guest validates BootInfo and emits a serial status byte. ELF-shape
tests parse the ELF64 header manually and assert `ET_EXEC` / `EM_X86_64` / `e_entry` /
a PT_LOAD covering the entry / a size budget. CI host lanes gain a `nasm` install step.

## Verification performed

I built the crate from a clean state on this x86_64 host and inspected the produced ELF:

- `e_entry == 0x100000`, `ET_EXEC`, `EM_X86_64`, statically linked — all confirmed via `readelf`.
- `_start` is the first symbol at `0x100000` (crt0 `.text.start` placed first) — confirmed via `objdump`.
- Single `PT_LOAD` with `MemSiz (0x4060) > FileSiz (0x47)`, confirming bss zero-fill is the loader's job.
- BootInfo magic byte order: `int.from_bytes(b"DHBI","little") == 0x49424844`, matching the `.inc` constant.
- `cargo test -p nanokernel`: all 4 tests pass.
- **Probe correctness (the central robustness question):** `ld -m elf_BOGUS --version` exits **1** on
  this binutils, while `ld -m elf_x86_64 --version` exits **0**. The probe's `status.success()` check is
  therefore correct — a single-target `ld` lacking `elf_x86_64` falls through to lld/rust-lld.
- **Directory `rerun-if-changed`:** editing a file inside `asm/` correctly triggers a rebuild (verified).
- **`include_bytes!` propagation:** flipping a byte in `pipeline_smoke.asm` changed the embedded ELF's
  hash after rebuild — the asm→build.rs→ELF→`include_bytes!` chain propagates correctly (verified).

## Verdict

**Approve with minor follow-ups.** The pipeline is correct, portable, well-documented, and
genuinely host-runnable. The headline robustness concern (probe false-positive on aarch64
single-target ld) does **not** materialize — GNU ld rejects an unsupported `-m` with a
non-zero exit even alongside `--version`. No Critical issues. A few Important hardening items
(PATH executable-bit check, CI `sudo`/idempotency durability, orphan-section robustness in
`link.ld`, env-var rerun tracking) and several suggestions are worth addressing before this
pipeline carries the real guest beads (ehu/7yr/7ys).

## Stats

- Files added: `tests/nanokernel/{Cargo.toml, build.rs, link.ld, README.md}`,
  `include/bootinfo.inc`, `asm/{crt0.asm, pipeline_smoke.asm}`, `src/lib.rs`, `tests/elf_shape.rs`
- Files modified: root `Cargo.toml` (workspace member), `.github/workflows/ci.yaml` (nasm step)
- Diff: ~502 lines
- Findings: 0 Critical, 4 Important, 6 Suggestions, 7 Positive notes
