# Suggestions

## S1 — `align 16` injects 13 *retiring* NOPs into the prologue; call it out

The `align 16` directive immediately before `.loop` pads `0x100a3..0x100af` with **13
single-byte NOPs**:

```
1000a1: xor edx,edx
1000a3: nop          ; ×13, all RETIRE on INST_RETIRED.ANY
...
1000af: nop
1000b0: <.loop>
```

These NOPs are part of the fixed prologue retirement count. This is *not* a bug — they are
deterministic — but a harness author reading the asm might mentally count "setup = 5
instructions" and be off by 13. Two ways to make this honest:

- **Cheapest:** add a one-line comment at the `align 16` site, e.g.
  `align 16   ; NB: pads with ~13 NOPs that RETIRE — part of the fixed prologue offset`.
- **Alternative:** drop `align 16` entirely. The loop is tiny (32 bytes) and dominated by a
  dependent `imul`/`rol` chain; 16-byte loop-entry alignment buys little here and removing it
  makes the prologue count match the source line-for-line. (Keep it only if a future
  microbenchmark actually shows a front-end penalty.)

Either is fine. The point is that the retiring-NOP count should be *visible*, not a surprise
during calibration.

## S2 — `lib.rs` doc should state the offset is *per-cmdline*, not universal

`LANDING_LOOP_INSTRS_PER_ITER`'s doc says harnesses compute `8 * iters + prologue` and
"calibrate the exact offset once, it is deterministic." Precise, but it omits that the
prologue retired-count **changes with the number of cmdline decimal digits**: each digit runs
the parse loop body once more (`movzx, sub, cmp, ja(NT), imul, add, mov, inc, dec, jmp` ≈ 9–10
retired instructions per digit), plus the magic/`test rsi` branches differ between the
"valid BootInfo + digits" path and the "empty cmdline → default" path.

So "calibrate once" is only true *for a fixed cmdline*. The M2 landing test using the default
(empty cmdline) is one calibration point; a test passing `"500"` is a different one. Suggest
tightening the doc to: *"deterministic for a given cmdline; recalibrate if the cmdline digit
count changes."* This prevents a harness from reusing the empty-cmdline offset against a
digit-bearing run.

## S3 — Add a host-side guard pinning the loop body to 8 instructions

Nothing currently fails if someone edits `.loop` to 7 or 9 instructions while leaving
`LANDING_LOOP_INSTRS_PER_ITER = 8` — the most dangerous silent drift for an icount harness.
A practical, dependency-free host check: locate the `prog_main.loop` symbol in the ELF's
symbol table, take the bytes from that address up to and including the backward `jne`, and
count instructions (or assert the exact 32-byte `[0xb0, 0xd0)` body length / a known byte
signature). Even a coarse assertion — "the byte span from `.loop` to its terminating
`0x75`/`0x74` rel8 back-branch decodes to exactly `LANDING_LOOP_INSTRS_PER_ITER` instructions"
— catches edits. A length check (`loop_end - loop_start == 32`) is the cheapest version and
already meaningfully guards the contract. Pair it with a comment that the magic number is the
sum of the 8 instruction encodings.

## S4 — `imul rax, rax, 10` in the parser can overflow on a long cmdline

A cmdline like `"99999999999999999999999"` overflows the 64-bit accumulator silently
(`imul rax,rax,10` wraps). The result is still *deterministic* and `mov rcx, rax` accepts any
64-bit value, so this is not a correctness bug for the harness — but the resulting iteration
count is whatever the wrapped value is, which could be enormous and hang the landing test, or
could wrap to a small/zero value (and a wrap-to-0 would then keep DEFAULT via the existing
`test rax,rax` guard — actually a benign outcome). Worth a one-line comment that the parser
is intentionally permissive and overflow is wrap-defined; optionally cap the digit count
(e.g. stop accepting digits past 10–12) so a fat-fingered cmdline can't request 1e18
iterations. Low priority since the cmdline is operator-controlled.

## S5 — Note the `r12` callee-saved clobber for future crt0 evolution

`prog_main` clobbers `r12` (SysV callee-saved) and never restores it. crt0 doesn't care today
— it only does `call prog_main` then falls into the HLT park, reading no saved registers. But
if crt0 ever grows post-`prog_main` logic that assumes the SysV callee-saved set survived, this
would bite. A one-line comment in `landing_loop.asm` ("clobbers r12/r13…; crt0 parks and reads
no callee-saved regs") documents the assumption that makes it safe.
