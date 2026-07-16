# Suggestions (non-blocking)

## S1 (minor accuracy) — the fault-path `LEN` poison is decorative, not the mechanism that trips the count assert

`entropy_draw.asm`:

```asm
.fault:
    mov     dword [r8 + REG_LEN], 0xDEAD ; poison: harness count check trips
.fault_spin:
    hlt
    jmp     .fault_spin
```

The comment "poison: harness count check trips" overstates the poison's role:

- `0xDEAD` = 57005, and `MAX_FILL` = `1 << 20` = 1,048,576, so `0xDEAD < MAX_FILL`.
  The poison does **not** push a later draw into `STATUS_FAULT`. (Confirmed
  against `entropy.rs:123` — `if self.len > MAX_FILL`.)
- More to the point: once `.fault` is reached the guest enters `.fault_spin`
  (`hlt; jmp`) and **never draws again**. So the host's count register stops
  advancing, and that is what trips `assert_eq!(count_pause, BATCHES_BEFORE * BATCH)`
  and the final count pins. The count stops because of the early HLT, **not**
  because of the `LEN` write. The `LEN` poison would still leave the count
  asserts failing even if the write were deleted.

Writing `LEN` mid-fault is harmless (it touches only the guest's own device
register on a path that never draws again), but the comment is misleading.
Suggest either:

- Reword to: `; marker for a host inspecting device regs; the count stops
  advancing because of the fault HLT below, which is what trips the harness`, or
- Drop the `LEN` write entirely and keep just the `.fault_spin` HLT loop.

This is purely a comment/clarity fix; no behavioral change.

## S2 — assert a STATUS-fault path actually halts, or note it is unexercised

The guest's `.fault` branch (`jne .fault` after `cmp eax, STATUS_OK`) is never
taken in the happy path the test runs, so the "halts loudly on STATUS!=OK"
contract is asserted only by construction. The device's fault behavior is
unit-tested in `entropy.rs` (`bad_gpa_faults_without_serving`,
`oversized_len_faults`), so this is covered at the device layer. Optional: a
one-line comment in the test noting that the fault path is device-unit-tested
elsewhere and intentionally not driven here would close the loop for a future
reader wondering why `.fault` is "dead" in this acceptance.

## S3 — the golden-nonzero pin could be slightly stronger

`golden.iter().any(|b| *b != 0)` rules out the all-zero "doorbell never ran"
failure. Because ChaCha20 output is dense, a single nonzero byte is plenty in
practice. If you want belt-and-suspenders against a partial-fill bug (e.g. only
the first slot written), you could additionally assert the **last** golden draw's
16 bytes are nonzero, or that `golden` has no all-zero 16-byte slot. Low value —
the byte-equality assertion against leg B already catches structural fill bugs;
mention only for completeness.

## S4 — consider pinning the device window base via the device's own constant

`elf_shape.rs` pins `define("ENT_BASE") == 0xD000_3000` with a literal, matching
`common/mod.rs`'s `bus.register(0xD000_3000, …)`. This is the **test bus**
convention, deliberately distinct from `entropy.rs`'s `PV_ENTROPY_BASE`
(`0xD000_2000`, which `common/mod.rs` uses for `CLOCK_BASE`). The literal is
correct, but the duplicated magic number lives in two test files
(`common/mod.rs` and `elf_shape.rs`) with no shared constant. Optional: hoist
`0xD000_3000` into a `nanokernel`/`common` const so a future bus-layout change
can't silently desync the guest base from the registered base. Not load-bearing
(both the bus registration and the elf_shape pin would have to drift together to
escape, and the live test would then fail), so this is just future-proofing.
