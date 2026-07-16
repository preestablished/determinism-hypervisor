# Review: `ralph/iteration-31-elf-boot-path` — the real ELF boot path

- **Reviewer:** Claude Opus
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-31-elf-boot-path` vs `main`
- **Bead:** determinism-hypervisor-s0p
- **Head commit:** `39f66d4` (iteration 31 checkpoint — dh-vmm ELF boot path)

## Verdict

**APPROVE.** This is a clean, well-factored promotion of the M0 dh-cli loader into a
reusable `dh-vmm::boot` module that adds exactly the two M1 obligations it claims: the
MMIO hole is mapped in the guest page tables (device accesses now surface as KVM MMIO
exits instead of triple-faulting), and the §2.2 default-deny MSR filter is applied at
boot. No correctness defects found. Page-table math, BootInfo ABI, the MSR resume
contract, and the determinism claim all check out under independent verification. The
findings below are all non-blocking suggestions and hardening notes.

## What was verified

| Check | Result |
|---|---|
| `cargo test -p dh-vmm -p dh-cli` | 48 + 4 pass, 0 fail |
| 3 host-side boot unit tests (page tables, BootInfo, ELF copy/zero-fill/reject) | pass |
| Live `device_exercise_reaches_a_real_mmio_exit` (dh-cli, real /dev/kvm) | pass — MMIO exit `0xd000_0008`, not triple fault |
| Live `hello_boots_and_prints`, `landing_loop_is_deterministic_across_runs` | pass |
| `cargo clippy -p dh-vmm -p dh-cli --all-targets -- -D warnings` | clean |
| `cargo fmt --check` | clean |
| Page-table slot math (hole PD3 slot 128; max-RAM last page PD3 slot 127; no collision) | verified independently |
| BootInfo offsets vs `tests/nanokernel/src/lib.rs` ABI | exact match |
| No consumer hardcodes the old `0x5000` BootInfo GPA (grep) | confirmed — guest reads it via RSI |
| MSR-denied dispatch writes deterministic reply before resume | confirmed in `classify_exit` |

## Stats

- Files changed: 4 (`crates/dh-vmm/src/boot.rs` new +345; `crates/dh-vmm/src/lib.rs` +1;
  `tools/dh-cli/src/boot.rs` −193/+23 net; `tools/dh-cli/tests/boot_hello.rs` +18)
- Findings: **0 critical, 0 important, 4 suggestions**

## Finding counts

- Critical: 0
- Important: 0
- Suggestions: 4
