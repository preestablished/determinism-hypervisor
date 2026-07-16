# Iteration 25 — Nanokernel Build Pipeline — Second Review (Opus)

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-25-nanokernel-build-pipeline` vs `main`
- **Bead:** determinism-hypervisor-22n
- **Scope:** `tests/nanokernel/` (new workspace member: `build.rs`, `crt0.asm`,
  `pipeline_smoke.asm`, `include/bootinfo.inc`, `src/lib.rs`, `tests/elf_shape.rs`,
  `link.ld`, `Cargo.toml`, `README.md`), root `Cargo.toml`/`Cargo.lock`,
  `.github/workflows/ci.yaml`.
- **Normative refs checked:** ARCHITECTURE.md §1 (crate layout, "~2 KiB" guest),
  §2.2 (VM construction, MMIO map, PIO map), §2.3 (boot protocol).

## Summary

This is a clean, well-reasoned, genuinely portable build pipeline for the
freestanding test guests. I built the crate on this x86_64 host, inspected the
emitted ELF with `readelf`, and ran the full test set — everything passes and the
artifact has exactly the shape the tests claim (ET_EXEC, EM_X86_64, e_entry =
0x100000, single PT_LOAD covering the entry, 5144 bytes file / 0x4060 memsz).

I independently chased every angle flagged for a second reviewer and could **not**
find a correctness bug that breaks the build or the tests today. The drift-test
parser — the most suspicious piece — is **correct** under the actual file's
whitespace (all spaces, no tabs; the trailing-space lookup convention defeats the
`BOOTINFO_OFF_CMDLINE` vs `…_CMDLINE_LEN` prefix collision; verified by walking
every line). The magic encoding ("DHBI" LE = 0x49424844) matches between asm and
Rust. The linker probe (`<ld> -m elf_x86_64 --version`) correctly fails-fast on the
wrong emulation (verified: GNU ld 2.42 here exits non-zero on a bogus `-m` even with
`--version`). SysV stack alignment at `prog_main` entry is correct (RSP%16==8 after
the `call`, because `stack_top` = 0x104060 is 16-aligned). nasm include resolution
works via both `-I` and source-relative fallback.

What I *did* find are two **latent fragilities** worth fixing before the loader bead
and future guests build on top, plus several documentation/coverage gaps:

1. The drift parser is correct **only** because every caller passes a trailing
   space; the abstraction silently rewards a wrong answer if a future caller forgets
   it. (Important — test integrity.)
2. The BootInfo struct layout (magic, version, `reserved`, field offsets) is
   asserted by `include/bootinfo.inc` + `src/lib.rs` and cited as "ARCH §2.3," but
   §2.3 does **not** define any of it — only "a versioned struct … carrying
   mem_size, MMIO base, cmdline bytes." The loader-bead author reading §2.3 will not
   find the contract this code mirrors. (Important — ABI source-of-truth.)
3. The PT_LOAD `memsz > filesz` (16409-byte `.bss` tail) **requires** the §2.3
   loader to zero-fill, and that contract is written nowhere a loader author will
   see it. (Important — loader contract.)
4. The single PT_LOAD is **RWE** (no W^X separation at the segment level). Fine for
   a test guest, but a W^X-enforcing loader would map `.text` writable. (Suggestion.)

## Verdict

**APPROVE WITH CHANGES.** No blocker for the build pipeline itself — it is correct,
portable, and tested. The Important items are about preventing a *future* silent
failure (drift parser footgun) and closing the ABI/loader contract gap that the next
bead depends on. None require reworking this iteration's structure.

## Stats

| Category   | Count |
|------------|-------|
| Critical   | 0     |
| Important  | 3     |
| Suggestions| 6     |
| Positive notes | 8 |

Files reviewed: 11 changed (421 insertions). Build + 4 tests + clippy `-D warnings`
all verified green on x86_64.
