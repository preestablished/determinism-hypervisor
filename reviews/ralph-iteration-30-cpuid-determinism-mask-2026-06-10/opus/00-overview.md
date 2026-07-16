# CPUID Determinism Mask — Review Overview

- **Reviewer:** Claude Opus
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-30-cpuid-determinism-mask` vs `main`
- **Bead:** determinism-hypervisor-8jx
- **Normative reference:** ARCHITECTURE.md §2.2, §7.2 (and cross-refs §3.1, §4, §5)

## Verdict

**APPROVE.** This is a clean, correct, and well-documented iteration. Every cleared
CPUID bit was verified against the Intel SDM bit assignments and against live
`KVM_GET_SUPPORTED_CPUID` output on this lab box; all are correct. The mask covers
everything ARCHITECTURE §7.2 *requires* (the only §7.2 item not implemented —
AVX512 LCD masking — is explicitly marked optional and "a determinism-class concern,
not a correctness one"). The PV-leaf removal, the vPMU-off cap, the order-independent
hash, and the `cpuid-diff` tool all behave as designed. All 45 dh-vmm/dh-cli tests
pass live, clippy is clean, and the live `cpuid-diff` output is sane.

There are **no Critical findings** and **no blocking Important findings**. The one
substantive Important item is a *consistency/wiring* observation (the new
`cpuid_table_hash` and the existing `config.rs` CPUID-table preimage are two separate
hashes of the same logical data) — this is correctly deferred to the config bead, but
the divergence in *which fields are hashed* (flags) should be reconciled when wired.
The suggestions are polish.

## Live verification performed (on this box, /dev/kvm rw)

- `cargo build -p dh-vmm -p dh-cli` — clean.
- `cargo test -p dh-vmm -p dh-cli` — **45 passed, 0 failed**, including the 3 new
  live CPUID tests (mask assertions, hash order-independence/sensitivity, vCPU
  carries masked table).
- `cargo clippy -p dh-vmm -p dh-cli` — no warnings.
- `cargo run -p dh-cli -- cpuid-diff` — produced the expected diff (see below).

### Live cpuid-diff (Coffee Lake lab box)

```
supported entries: 42   masked entries: 40
leaf 0x00000001.0 ecx: 0x76fab223 -> 0x36da3223 (cleared 0x40208000)   # PDCM|x2APIC|RDRAND
leaf 0x00000006.0 eax: 0x00000004 -> 0x00000000 (cleared 0x00000004)   # ARAT + leaf6 zeroed
leaf 0x00000007.0 ebx: 0x009c67ab -> 0x009867ab (cleared 0x00040000)   # RDSEED (bit18)
leaf 0x0000000a.0 eax: 0x07300802 -> 0x00000000                        # PMU zeroed
leaf 0x0000000a.0 edx: 0x00008603 -> 0x00000000
leaf 0x40000000.0: REMOVED (KVMKVMKVM signature)
leaf 0x40000001.0: REMOVED (PV features)
leaf 0x80000001.0 edx: 0x2c100800 -> 0x24100800 (cleared 0x08000000)   # RDTSCP (bit27)
leaf 0x80000007.0 edx: 0x00000100 -> 0x00000000 (cleared 0x00000100)   # INVTSC (bit8)
masked table hash: 4dac1b7a...46bdc03
```

`MONITOR` (bit3), `TSC_DEADLINE` (bit24), leaf1.EDX `TM`/`ACPI` are not in the diff
only because KVM's *supported* table does not advertise them on this host — the mask
still clears them unconditionally (`&= !`), which is correct and future-proof.

## SDM bit verification (all correct)

| Bit constant | Value | SDM leaf:reg.name | Verdict |
|---|---|---|---|
| `L1_ECX_MONITOR` | 1<<3 | CPUID.1:ECX[3] MONITOR | ✓ |
| `L1_ECX_PDCM` | 1<<15 | CPUID.1:ECX[15] PDCM | ✓ |
| `L1_ECX_X2APIC` | 1<<21 | CPUID.1:ECX[21] x2APIC | ✓ |
| `L1_ECX_TSC_DEADLINE` | 1<<24 | CPUID.1:ECX[24] TSC-Deadline | ✓ |
| `L1_ECX_RDRAND` | 1<<30 | CPUID.1:ECX[30] RDRAND | ✓ |
| `L1_EDX_ACPI` | 1<<22 | CPUID.1:EDX[22] ACPI | ✓ |
| `L1_EDX_TM` | 1<<29 | CPUID.1:EDX[29] TM | ✓ |
| `L7_EBX_RDSEED` | 1<<18 | CPUID.7.0:EBX[18] RDSEED | ✓ |
| `L8_1_EDX_RDTSCP` | 1<<27 | CPUID.80000001:EDX[27] RDTSCP | ✓ |
| `L8_7_EDX_INVTSC` | 1<<8 | CPUID.80000007:EDX[8] InvariantTSC | ✓ |

## Stats

- Files changed: 7 (2 new source files, 5 edited).
- New source: `crates/dh-vmm/src/cpuid.rs` (196 lines), `tools/dh-cli/src/cpuid.rs` (62 lines).
- New dep: `blake3` on `dh-vmm` (workspace dep; ARCH §1 external-crates list ✓).
- Tests added: 3 live (all passing).
- Findings: **0 Critical, 1 Important (deferred/consistency), 5 Suggestions, 9 Positive notes.**
