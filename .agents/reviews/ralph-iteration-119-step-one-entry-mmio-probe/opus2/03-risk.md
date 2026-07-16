# Risk

Primary risk: false confidence. The current probe passes on this host and exercises a real MMIO-plus-interrupt path, but the assertions are loose enough that a future change could add unintended retirements after the emulated-MMIO completion and still pass.

Dynamic discovery is useful for avoiding brittle absolute icounts, but it needs to carry enough identity with it. Without RIP and exact MMIO GPAs, the test cannot distinguish "the intended MMIO-adjacent boundary" from "some deterministic entry that happened to include MMIO work."

Low risk areas:
- ISR clobbering looks handled for the current handler body.
- The new guest is included in both the build list and the host-runnable ELF shape test.
- The table GPA is drift-tested against the assembly define.
