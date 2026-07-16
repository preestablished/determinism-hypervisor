# Iteration 34 Review — Deterministic Interrupt Injection (§3.4)

- **Reviewer:** Claude Opus
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-34-interrupt-injection` vs `main`
- **Bead:** determinism-hypervisor-mny
- **Scope:** `crates/dh-vmm/src/inject.rs` (new, 290 LOC) + `lib.rs` module export
- **Normative reference:** `.agents/docs/determinism-hypervisor/ARCHITECTURE.md` §3.4 (lines 318–336)

## Verdict

**APPROVE.** The §3.4 interrupt-injection rule is implemented faithfully, the
ioctl number is verified against the kernel uapi, the determinism argument is
sound, and the two live tests empirically prove both the closed-window deferral
path (bit-identical deferral, no window-request leak) and the open-window
queue-and-deliver path (KVM_INTERRUPT queues, vector delivers on next entry,
proven by deterministic empty-IDT triple fault). No Critical or Important
findings. A small set of low-risk suggestions and follow-ups are recorded.

## What was checked

- **Tests RUN (not just read):** `cargo test -p dh-vmm` — **60 passed, 0 failed**,
  including `inject::tests::closed_window_defers_deterministically_live` and
  `inject::tests::open_window_injects_and_delivers_live`. Default parallel test
  harness (no `--test-threads 1` forced); the suite is green under parallelism.
- **`cargo clippy -p dh-vmm --all-targets`:** clean, zero warnings.
- **ioctl number verified against kernel uapi:** `/usr/include/linux/kvm.h:1532`
  → `KVM_INTERRUPT _IOW(KVMIO, 0x86, struct kvm_interrupt)`. The code's
  `ioctl_iow_nr!(KVM_INTERRUPT, 0xAE, 0x86, kvm_interrupt)` uses 0xAE = KVMIO,
  0x86, `kvm_interrupt` — an exact match. The msr.rs precedent
  (`KVM_X86_SET_MSR_FILTER, 0xAE, 0xc6` at line 58) matches the same header's
  line 1727. `kvm_run` fields `ready_for_interrupt_injection`, `if_flag`,
  `request_interrupt_window` all confirmed present (kvm.h:230/236/237).
- **No-irqchip regime confirmed:** `kvm.rs` never calls KVM_CREATE_IRQCHIP /
  KVM_CREATE_PIT2 and smoke-asserts a fresh vCPU has no in-kernel irqchip — this
  is exactly the userspace-irqchip regime in which KVM_INTERRUPT is the valid
  vector-queue mechanism.
- **`get_kvm_run` semantics:** kvm-ioctls 0.24 returns `&mut kvm_run` backed by
  the mmap'd shared page, so `run.request_interrupt_window = 1` is kernel-visible
  on the next KVM_RUN — the deferral write-back is correct.
- **land_at contract** (`boundary.rs`): confirmed single-exit error precedence,
  single-step always dropped (incl. error paths), counting unaffected by exits.

## Stats

| Metric | Value |
|---|---|
| Files changed | 2 (`inject.rs` new, `lib.rs` +1 line) |
| New LOC | 290 |
| Tests added | 2 live (both passing) |
| Full suite | 60 passed / 0 failed |
| Clippy | clean |
| Critical findings | 0 |
| Important findings | 0 |
| Suggestions | 4 |
| Follow-ups (non-blocking) | 2 |
