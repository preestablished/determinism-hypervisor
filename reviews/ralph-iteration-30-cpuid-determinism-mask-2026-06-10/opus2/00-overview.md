# CPUID determinism mask — review overview

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-30-cpuid-determinism-mask` vs `main`
- **Bead:** determinism-hypervisor-8jx (P0, M1)
- **Scope:** `crates/dh-vmm/src/cpuid.rs` (new), `crates/dh-vmm/src/kvm.rs` (PMU cap + SET_CPUID2 in `create_slot_vm`), `tools/dh-cli/src/cpuid.rs` (new `cpuid-diff`), wiring (`lib.rs`, `main.rs`, `Cargo.toml`).

## Verdict

**APPROVE WITH FOLLOW-UPS.** The implementation is correct, well-commented, and live-verified on this box. Every bead-named bit maps to the right leaf/register/bit (independently checked against `/proc/cpuinfo` and a raw `KVM_GET_SUPPORTED_CPUID` dump). No bit is translated to the wrong leaf. The hash is order-independent as claimed. Nothing here is a merge blocker.

The findings are about **determinism leaks the mask leaves open** that §7.2's wording arguably covers (WAITPKG/UMWAIT, leaf 0x15/0x16 frequency, hybrid leaf 0x1A), the **best-effort vs hard-fail philosophy** of the PMU cap, and **zero hosted-lane test coverage** (all three tests skip without `/dev/kvm`). These are correctness-of-completeness and robustness issues, not bugs in the code that exists.

## What I verified live (this box: Intel i5-8400, Coffee Lake, kernel 6.8.0-124)

- `cargo build -p dh-cli -p dh-vmm` — clean.
- `cargo test -p dh-vmm cpuid` — **3 passed, 0 failed** (mask assertions, hash order-independence + mask-sensitivity, slot-vCPU carries masked table).
- `./target/debug/dh-cli cpuid-diff` — ran; output below.
- Independent raw `KVM_GET_SUPPORTED_CPUID` dump (standalone probe) to cross-check leaf presence, subleaf flags, and duplicate `(function,index)` pairs.

### cpuid-diff output (this host)

```
supported entries: 42   masked entries: 40
leaf 0x00000001.0 ecx: 0x76fab223 -> 0x36da3223 (cleared 0x40208000)   # RDRAND|x2APIC|PDCM
leaf 0x00000006.0 eax: 0x00000004 -> 0x00000000 (cleared 0x00000004)   # ARAT
leaf 0x00000007.0 ebx: 0x009c67ab -> 0x009867ab (cleared 0x00040000)   # RDSEED
leaf 0x0000000a.0 eax: 0x07300802 -> 0x00000000 (cleared 0x07300802)   # arch PMU
leaf 0x0000000a.0 edx: 0x00008603 -> 0x00000000
leaf 0x40000000.0: REMOVED   # KVM signature
leaf 0x40000001.0: REMOVED   # KVM features
leaf 0x80000001.0 edx: 0x2c100800 -> 0x24100800 (cleared 0x08000000)   # RDTSCP
leaf 0x80000007.0 edx: 0x00000100 -> 0x00000000 (cleared 0x00000100)   # INVTSC
masked table hash: 65be80759e6ff65db310c595041da2c9c8a15522d802a5ec909f18c841712d38
```

### Bit-mapping audit (bead list → implementation)

| Bead item | Leaf/reg/bit in code | Correct? |
|---|---|---|
| RDRAND | L1 ECX[30] `0x40000000` | yes |
| RDSEED | L7.0 EBX[18] `0x00040000` | yes |
| TSC_DEADLINE | L1 ECX[24] | yes (absent in KVM-supported here; mask is a no-op but correct) |
| ARAT | L6 EAX[2] (whole leaf zeroed) | yes |
| MWAIT/MONITOR | L1 ECX[3] | yes |
| x2APIC | L1 ECX[21] | yes |
| PDCM/PMU | L1 ECX[15] PDCM + L0xA zeroed + `KVM_PMU_CAP_DISABLE` | yes |
| RDTSCP | 0x80000001 EDX[27] | yes |
| invariant-TSC advert | 0x80000007 EDX[8] | yes |
| TM/turbo/thermal zeroed | L1 EDX TM[29]/ACPI[22] + L6 zeroed | yes |
| KVM paravirt 0x4000_00xx removed | `retain` drops `0x4000_0000..0x4000_0100` | yes (42→40 entries confirmed) |

RDTSC itself (L1 EDX[4]) is intentionally **retained** — confirmed against ARCH §4.1 (pv-clock + `tsc=unstable` + CR4.TSD + verification mode are the backstops; §7.3 lists stray RDTSC as an accepted residual). Not a finding.

## Stats

- Files changed: 7 (2 new source, 1 new tool source, 4 wiring/lock).
- New tests: 3 (all live-gated, all green here).
- Findings: **0 Critical, 3 Important, 5 Suggestions, 6 Positive notes.**
