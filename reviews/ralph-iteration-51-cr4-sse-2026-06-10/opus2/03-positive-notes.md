# Positive notes

- **The OSXSAVE-off decision is the right call and is well-reasoned.** Enabling SSE
  (OSFXSR) without opening the XSAVE/AVX surface keeps the guest FP state to exactly the
  x87+SSE subset that `KVM_GET_FPU` already hashes (`hash.rs:247-260` hashes fpr[],
  fcw/fsw/ftwx, last_opcode/ip/dp, **xmm[] and mxcsr**). The CR4 comment and the ARCH
  §2.3 edit both spell out the invariant clearly: "the FP state that exists is exactly
  what KVM_GET_FPU captures — nothing outside the hash."

- **CPUID mask is kept consistent with CR4.** Advertising XSAVE/AVX while OSXSAVE is off
  would let a compiled guest feature-detect its way into a `#UD`. Masking FMA/XSAVE/
  OSXSAVE/AVX/F16C (leaf 1), AVX2/AVX-512 (leaf 7), and zeroing leaf 0xD closes that
  exactly. I verified the cleared-bit math live: leaf 1 ECX cleared `0x74209000`
  (FMA|PDCM|x2APIC|XSAVE|AVX|F16C|RDRAND), leaf 7 EBX cleared `0x00040020` (RDSEED|AVX2),
  leaf 0xD all subleaves → 0. No unexpected clears.

- **Precision in what is NOT masked.** BMI1/BMI2/ADX (leaf 7 EBX 3/8/19) are GPR-only
  and correctly kept. GFNI/VAES/VPCLMULQDQ (SSE-form-capable) are correctly NOT in the
  AVX mask — the change does not over-reach into SSE-usable features. On this host none
  are present anyway, but the mask is written to not touch them.

- **The probe's failure mode is genuinely loud and distinguishable.** Confirmed
  `crt0.asm` installs no IDT (just `call prog_main` then `hlt`), and only `timer_guest`
  sets up an IDT. So a missing-OSFXSR `#UD` in sse_probe triple-faults to
  KVM_EXIT_SHUTDOWN — never the serial `V`. The `assert_eq!(out.serial, b"V")` cannot
  pass by accident: a fault yields no byte, a wrong vector yields `v`. The asm vector
  math is correct (verified: lane0 `0x333…3a`, lane1 `0xccc…d5`).

- **No icount/trace drift for pre-existing guests.** `counting_semantics` still pins
  997 with OSFXSR on. The CR4 write is host-side at `KVM_SET_SREGS` before first entry
  and adds zero guest instructions; KVM injects nothing extra on first entry from
  OSFXSR. Verified by execution, not just reasoning.

- **The masked-table hash is deterministic.** `f19610e1…` is stable across 5 live runs
  and matches the committed artifact — the value that feeds `MachineConfig` is solid,
  notwithstanding the supported-side display-line wobble (I2).

- **Clean engineering hygiene.** New constants carry per-bit comments naming the
  nondeterminism/decision they encode; the AVX-512 group is a single named const rather
  than scattered magic; sse_probe is wired through `build.rs`, `lib.rs`, and the
  `elf_shape` shape gate so it can't rot. Full workspace tests + both-arch clippy clean,
  tree clean.
