# Tests

Passed:

- `cargo test -p dh-vmm linux_cpu_compat -- --nocapture`
  - 7 passed, 0 failed.
- `cargo test -p dh-vmm -- --nocapture`
  - 162 library tests passed, 3 `blk_fixture` integration tests passed, doctests passed.
- `cargo test -p determinism-tests trace_tests -- --nocapture`
  - 2 `linux_boot_trace` serializer/contract tests passed.

Not runnable locally:

- `cargo test -p determinism-tests linux_entry_smoke -- --ignored --nocapture`
  - Failed at artifact setup because `DH_M9_BZIMAGE`, `DH_M9_INITRAMFS`, `DH_M9_BASE_IMAGE`, `DH_M9_GAME_IMAGE`, and `DH_M9_IMAGE_CACHE` were not set.
  - This means I did not locally validate the full ignored Linux boot trace against real M9 Linux artifacts.

Test adequacy assessment:

The branch has good focused coverage for the new CPU compatibility policy: CPUID masking, CPUID hash-preimage sensitivity, denied MSR classification, `IA32_BIOS_SIGN_ID` WRMSR no-op behavior through live KVM, no in-kernel irqchip construction, interrupt-window deferral determinism, and timer vns-to-icount conversion. The remaining gap is the artifact-backed Linux boot smoke/trace, which should be run in an environment with the required M9 artifacts before treating the early-boot compatibility work as fully characterized.
