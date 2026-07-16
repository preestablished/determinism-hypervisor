# Iteration 31 — dh-vmm ELF boot path — Review Overview

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-31-elf-boot-path` vs `main`
- **Bead:** s0p (dh-vmm's real ELF boot path, ARCH §2.3 type 1)
- **Scope:** `crates/dh-vmm/src/boot.rs` (new), `crates/dh-vmm/src/lib.rs`,
  `tools/dh-cli/src/boot.rs` (delegation), `tools/dh-cli/tests/boot_hello.rs`.

## Verdict

**APPROVE.** The new loader is correct, deterministic, and well-tested.
Every adversarial angle I probed (page-table arithmetic, the 0x5000→0x7000
BootInfo move, layout overlap, MMIO-hole PTE collision, the hand-built test
ELF offsets, MSR-emulation completion in `classify_exit`, cross-subsystem
interference at boot) verified clean against direct measurement on this box.
No Critical or Important findings. Two genuine-but-minor gaps worth a
follow-up bead (OSFXSR/SSE for future non-asm guests; no explicit upper
bound on `p_vaddr` in `load_elf`), plus one doc nit in the commit message
("every lane" is false for the arm lane, which `--exclude`s dh-vmm).

## What I verified live (this box has `/dev/kvm` rw)

| Check | Result |
|---|---|
| `cargo build --workspace` | clean |
| `cargo clippy --workspace --all-targets -D warnings` | clean |
| `cargo fmt --check` (dh-vmm, dh-cli) | clean |
| `cargo test --workspace` | all pass (dh-vmm 48, dh-cli boot_hello 4, dh-devices 59, etc.) |
| dh-vmm boot + msr + cpuid + kvm live tests together | 48/48 pass — no interference |
| landing_loop `--cmdline 7777` ×2 via CLI | identical: `{"serial":"L","exits":2}` |
| hello via CLI | `{"serial":"HELLO\n","exits":7}` (few exits ✓) |
| device_exercise via CLI | surfaces `MMIO at 0xd0000008` (pv-clock read) — NOT a triple fault ✓ |

## Arithmetic verified by computation

| Claim | Verified |
|---|---|
| `PAGE_2M = 2<<20 = 0x200000` (= `1<<21`, no typo) | ✓ |
| PDs occupy `0x3000..0x7000` exclusive (4 pages) | ✓ |
| BootInfo `0x7000..0x8000`, no overlap with PDs | ✓ |
| MMIO-hole PTE at slot `0x6400` (PD#3, idx 128); last RAM PTE at `0x63f8` — adjacent, no collision; RAM top `0xcfe00000 < 0xd0000000` | ✓ |
| `MAX_CMDLINE = 4064`; `0x20 + 4064 == 4096` (one page) | ✓ |
| Hand-built test ELF phdr: type@0, offset@8, vaddr@16, filesz@32, memsz@40; phentsize 56; e_phentsize@54, e_phnum@56 | ✓ (matches ELF64 spec) |
| BootInfo offsets in `write_bootinfo` match nanokernel `lib.rs` canonical ABI (magic 0, ver 4, mem 8, mmio 0x10, cmdlen 0x18, cmdline 0x20) | ✓ |

## Stats

- Files changed: 4 (1 new module +345, dh-cli boot.rs net −145, lib.rs +1, 1 test +18)
- Findings: **0 Critical, 0 Important, 4 Suggestions, 1 doc nit**
- Build/lint/test gates: all green on this KVM-capable host
