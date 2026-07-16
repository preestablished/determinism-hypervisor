# Review Overview — Phase-1 StateHashChain

- **Reviewer:** Claude Opus
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-32-state-hash-chain` vs `main`
- **Bead:** determinism-hypervisor-35z
- **Scope:** `crates/dh-vmm/src/hash.rs` (new, 476 lines) + `crates/dh-vmm/src/lib.rs` (one `pub mod hash;` line)
- **Normative refs:** ARCHITECTURE.md §8.5 (lines 715–731), §8.1 MSR capture list (lines 643–647)

## Verdict

**Approve with required follow-ups (one Important, before the M4 codec lands).**

The implementation is clean, well-documented, correct against its own producer/consumer
symmetry, and fully tested (53/53 lib tests pass, including 5 live KVM tests on this host).
The Phase-1 scoping is honest and the in-code rationale is unusually good. Nothing here
breaks the M3 run-twice-compare use case it targets, because both the record and replay
sides run this exact code, so every preimage choice is self-consistent.

The findings that matter are all **forward-compatibility / preimage-discipline** issues that
become real bugs only when the M4 DHSNAP codec serializes the same logical state in
**§8.1 document order** and the two preimages must agree. Because the hash is versioned
(`"dh-statehash-v1"`) and we are pre-release, the cheap fix is to align the preimage to the
§8.1 document order **now**, while a version-string bump is still free.

## Key conclusions on the bead's specific questions

- **SREGS vs SREGS2:** kvm-ioctls 0.24 (pinned) **does not expose `get_sregs2` at all** —
  there is no such method in `src/ioctls/vcpu.rs`, and no `sregs2` symbol anywhere in the
  crate. KVM_CAP_SREGS2 enable is therefore moot: the binding can't issue the ioctl. The
  deviation is **acceptable-with-bead-note, not a must-fix** — it is the *only* option with
  the current dependency. The bead text ("KVM_GET_SREGS2") describes the §8.1 target, not a
  shippable instruction. The real risk is the **M4 preimage break** (see 01, Important #1).
- **§8.1 list-order deviation (IA32_TSC placement):** Confirmed real. §8.1 lists
  `… PAT, TSC_AUX, IA32_TSC … SPEC_CTRL`; the impl appends the normalized TSC slot **after**
  SPEC_CTRL. Self-consistent today; a preimage mismatch waiting for the M4 codec. Fix now.
- **SPEC_CTRL partial-return:** Confirmed real robustness concern. `get_msrs` returns the
  count KVM actually read and stops at the first unreadable index; the strict `n != len`
  check turns an unsupported SPEC_CTRL on an older host into a hard `push_final_link` failure.

## Stats

| Metric | Value |
|---|---|
| Files changed | 2 (1 new, 1 one-line) |
| Lines added | ~477 |
| Tests | 53 passed / 0 failed (incl. 5 live KVM) |
| Critical findings | 0 |
| Important findings | 3 |
| Suggestions | 5 |
| Positive notes | 7 |

## Test verification

Ran `cargo test -p dh-vmm --lib` on this host (/dev/kvm rw available). All 53 tests pass,
including the four live tests (`vcpu_blob_is_stable_across_reads_live`,
`final_link_sees_guest_ram_live`, and the cpuid/kvm live tests). The byte-flip offset
`0x1F_F123` is within the 2 MiB slot used by the live test — confirmed in-range.
