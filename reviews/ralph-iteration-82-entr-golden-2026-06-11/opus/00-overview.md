# Review: M4 ENTR-golden ACCEPT (bead dy8)

- **Branch:** `ralph/iteration-82-entr-golden`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Commit under review:** `83c4e1d` ("iteration 82 checkpoint - M4 ACCEPT ENTR golden (1024 draws through real MMIO)")
- **Verdict:** **APPROVE**

## Summary

This change discharges dy8, the M4 ENTR-golden acceptance: a new NASM guest
(`entropy_draw.asm`) drives 16-byte entropy fills through the **real** pv-entropy
MMIO doorbell, ringing the raw bytes at `0x50_0000`; a new host test
(`entr_golden.rs`) boots the guest, draws 512 fills, snapshots mid-stream,
keeps the un-snapshotted leg running for 1024 "golden" fills, then restores into
a fresh slot and asserts the restored machine's next 1024 fills are
**byte-identical** to the golden continuation, with the `{seed, stream, word_pos}`
tuple and the device registers round-tripping exactly.

I verified the acceptance is **honestly and fully discharged**:

- **Real MMIO path.** Both legs service genuine `MmioRead`/`MmioWrite` exits
  through `MmioBus` → `PvEntropy::doorbell`, which draws from the slot's
  `DetEntropy` and writes guest RAM synchronously. No shortcut.
- **Both halves of ENTR v2 are load-bearing.** Leg B's PRNG comes from
  `outcome.entropy` (the ENTR PRNG half) and its device registers
  (`buf_gpa`/`len`/`status`) come from the ENTR device half via
  `device.restore(&regs, 1)`. Critically, after restore the guest resumes at
  the `.batch` label (instruction after `HLT`), **not** at `prog_main`, so it
  never re-programs `LEN`. Byte equality therefore depends on the restored `LEN`
  register being correct. If either half were broken, leg B's bytes would
  diverge from golden and the test would fail. There is **no false-pass path**
  where the device re-derives state some other way.
- **"Mid-stream" is honest.** The snapshot lands at a batch boundary (HLT after
  512 draws), not mid-instruction — but the PRNG is genuinely mid-stream
  (word_pos ≈ 2048 words into an ongoing ChaCha20 stream, neither at draw 0 nor
  at stream end). dy8 asks for "snapshot mid-stream ... draw 1024 more"; this
  satisfies it. The module doc is transparent that the boundary is a HLT.

The **highest-value verification — resume-after-HLT semantics — checks out** and
is sound rather than accidental (see 01). The bead 4a3 overshoot diagnosis is
plausible and the repro is good quality.

One minor doc-accuracy nit (the fault-path `LEN` poison is decorative, not the
mechanism that trips the count assert) and a few optional hardening suggestions;
none block approval.

## Stats

- Files changed: 5 (`entr_golden.rs` +333, `entropy_draw.asm` +92,
  `build.rs` +1, `lib.rs` +18, `elf_shape.rs` +38) — 482 insertions.
- New host test: 1 (hardware-gated, self-skips without `/dev/kvm`).
- New guest program: 1; wired into build.rs, lib.rs consts, elf_shape shape +
  drift pins (device-side register truth from `dh_devices::entropy`).
- Findings: 0 Critical, 0 Important, 1 minor accuracy note, 4 suggestions.
