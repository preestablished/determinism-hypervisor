# Review: iteration 29 — dh-cli M0 boot path (bead 1mz)

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-29-dh-cli-boot` vs `main`
- **Scope:** M0 dh-cli boot path — minimal ELF loader, identity 2 MiB paging (≤ 1 GiB),
  `KVM_SET_SREGS` long-mode entry, BootInfo at `0x5000`, run-until-HLT with serial sink.
- **Files:** `tools/dh-cli/src/{boot.rs,main.rs,lib.rs}`, `tools/dh-cli/tests/boot_hello.rs`,
  `tools/dh-cli/Cargo.toml`; cross-read `crates/dh-vmm/src/{kvm.rs,msr.rs}`,
  `tests/nanokernel/{src/lib.rs,asm/*.asm,include/bootinfo.inc}`.

## Verdict

**APPROVE.** The M0 boot path is correct, minimal, and live-verified end-to-end on this box.
I independently built and ran it: `hello.elf` boots and prints `HELLO\n` (7 exits),
`pipeline_smoke` reports `K` (BootInfo magic/version OK), `landing_loop` runs and prints `L`
deterministically across repeated runs and across cmdline values. `cargo test -p dh-cli` passes
both live legs; `cargo clippy -p dh-cli` is clean. The BootInfo byte layout matches
`bootinfo.inc` exactly, page-table and 1-GiB arithmetic are correct, and the ELF loader handles
the hostile `p_memsz < p_filesz` and bss-tail cases safely.

No Critical findings. The findings below are all **Important-as-documentation** or **Suggestion**
class — chiefly that one documented failure mode (device_exercise → "MMIO error") does **not**
actually occur, because the M0 page tables never map the MMIO hole, so the guest page-faults to a
triple-fault `Shutdown` before any MMIO exit is generated. The two MMIO arms in `run_until_hlt`
are therefore **dead code** for any RAM-only page-table layout. This is benign for M0 (still a
correct failure) but the comment and the dead arms are misleading and should be corrected or
annotated. I deliberately did not mirror the first reviewer; these are my own angles.

## Live verification performed

| Check | Result |
|---|---|
| `cargo build -p dh-cli` | clean |
| `cargo test -p dh-cli` | 2/2 live tests pass (hello, pipeline_smoke) |
| `cargo clippy -p dh-cli` | clean (workspace) |
| `dh-cli boot hello.elf` (text + `--json`) | `HELLO\n`, `exits:7` |
| `dh-cli boot pipeline_smoke.elf --json` | `{"serial":"K","exits":2}` |
| `landing_loop --cmdline 100` ×2 | identical `{"serial":"L","exits":2}` (deterministic) |
| `landing_loop --cmdline 0` ×2 | identical (deterministic) |
| `landing_loop --cmdline 1000000` ×3 | identical |
| `device_exercise` (16 MiB and 1 GiB) | `Shutdown`, **not** the documented MMIO error |
| `--mem-mib 2048` (>1 GiB) | clean rejection: "M0 loader maps at most 1 GiB" |
| `--mem-mib 0` | EINVAL (cryptic but no panic) |
| `--mem-mib 2^44` (`<<20` overflow) | wraps → EINVAL (cryptic) |
| arg-parse edges (`--bogus`, `abc`, missing path) | usage + exit 2 |
| BootInfo byte-walk vs `bootinfo.inc` | byte-perfect (reserved u32 present, cmdline @ 0x20) |
| `2 << 20` precedence, `1 << 30` cap, `div_ceil` page count | all correct |
| ELF/page-table overlap | none (guest @ 0x100000; tables @ 0x1000–0x5000) |

## Stats

- Critical: 0
- Important: 2
- Suggestions: 5
- Positive notes: 7
