# vCPU full capture/restore + DHSNAP VCPU section codec — 2nd-reviewer report

- **Branch:** `ralph/iteration-70-full-vcpu-capture-restore` (commit `57057bf`) vs `main`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus (2nd reviewer) — independent, experiment-driven
- **Bead:** 55f — `vcpu_state.rs`: capture/restore + DHSNAP `VCPU` section codec
- **Scope:** the single new file `crates/dh-vmm/src/vcpu_state.rs` (+1 `pub mod` line in `lib.rs`); 479 lines, no other production code touched.

## How this review was run

This is an empirical review on a box with `/dev/kvm`. I wrote scratch tests
(now reverted — tree verified clean) that:

1. Captured a live `VcpuState`, encoded the section, and dumped **every**
   struct-padding / reserved byte range in the encoding from a live capture.
2. Compared the encodings of **two different freshly-created VMs** for byte
   identity (the cross-VM / cross-fork concern).
3. Ran the **real-mode harness guest** (`out 0xD3,al ; hlt`, the same code
   `kvm.rs` uses) for two exits, then tested the capture→restore→capture
   fixed point *after real execution* (the committed live test only perturbs
   regs, it never runs the guest).
4. Exercised the **EFER double-set** path (SREGS then MSR) live.
5. Ran `cargo test -p dh-vmm --lib` (101 pass) and `cargo clippy` (clean).

## Empirical results (kernel 6.8, lab box, release/debug)

| experiment | result |
|---|---|
| All padding/reserved ranges zero on live capture | **YES** — sregs segment/dtable pads, fpu pad1/pad2, xcrs padding[16]+unused entries, events.reserved[26], dbg.reserved[9] all zero; totals all 0 |
| Two different fresh VMs → byte-identical encoding | **YES** — `CROSS-VM encodings identical: true` |
| Fixed point after **real guest execution** (rip=0x3 post out+hlt) | **YES** — `FIXED POINT after real execution: true` |
| EFER double-set (SREGS then MSR) round trips | **YES** — restore succeeds, re-capture byte-identical |
| TSC offset attribute programmed by restore | **YES** (committed test, re-confirmed) |

The headline PADDING risk (the iteration-69 XSAVE class) is **empirically
clean for `encode_section` on this kernel**: KVM's `GET_*` ioctls zero the
reserved/padding fields, so unlike raw XRSTOR/XSAVE the byte-copy carries no
kernel garbage. That said, see 01 for the structural finding that
overshadows the padding question.

## Verdict

**APPROVE WITH IMPORTANT NOTES.** The code is correct, well-documented,
defensive (fail-closed on XSAVE2, strict decode validation), and every live
round trip is a fixed point — including after real guest execution, which the
committed tests do not cover. The merge-blocking concerns are NOT bugs in
`vcpu_state.rs` itself; they are a **divergence between this new
`encode_section` and the already-committed `hash.rs::canonical_vcpu_blob`**,
which `dh-snapshot/src/dhsnap.rs` documents as *the* VCPU-section encoder.
Two parallel, non-identical definitions of "the VCPU section" now exist; the
bead that wires qmp/9e4 must reconcile them or the stored section bytes, the
hash preimage, and the cross-fork byte-compare will be three different things.

## Stats

- Files reviewed: 1 production file (`vcpu_state.rs`), cross-referenced
  against `hash.rs`, `tsc.rs`, `msr.rs`, `kvm.rs`, `dhsnap.rs`,
  `docs/decisions/tsc-alignment.md`, `ARCHITECTURE.md` §4/§8, `API.md` §4.
- Findings: **0 Critical**, **2 Important**, **4 Suggestions**.
- Tests: 101 pass (`cargo test -p dh-vmm --lib`); clippy clean.
- Scratch reverted: **verified** (`git status` clean).
