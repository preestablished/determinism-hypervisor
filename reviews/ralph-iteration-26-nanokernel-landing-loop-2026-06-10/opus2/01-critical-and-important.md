# Critical & Important

**None.** This section documents the issues I hunted for and *cleared*, so the next
reviewer/harness author doesn't have to re-derive them.

## Cleared: loop body retires exactly 8 instructions per iteration

Disassembly of `<prog_main.loop>` (built artifact):

```
1000b0: imul rax,r10          ; 1
1000b4: add  rax,r11          ; 2
1000b7: rol  rax,0xd          ; 3
1000bb: mov  [r12+rdx*8],rax  ; 4
1000bf: add  rdx,0x1          ; 5
1000c3: and  rdx,0x1fff       ; 6
1000ca: sub  rcx,0x1          ; 7
1000ce: jne  1000b0           ; 8
```

Every source line is exactly one instruction. PMU `INST_RETIRED.ANY` counts the branch
whether taken or not: iterations `1..N-1` retire the taken `jnz` (8 each), iteration `N`
retires the not-taken `jnz` (still 8). **Loop retirements = 8·N exactly.** No off-by-one.

## Cleared: `lea r12, [ring_buf]` resolves to the full absolute address

nasm `BITS 64` without `default rel` emits **absolute disp32 via SIB**, not RIP-relative:

```
10008f: 4c 8d 24 25 40 41 10 00   lea r12,ds:0x104140
```

`ModRM.rm=100b` (SIB) + `SIB base=101b, index=100b` → `[disp32]` absolute. `0x104140` is the
final link-time VMA of `ring_buf`. This is **correct for this ET_EXEC non-PIE guest loaded at
its link address** — the guest only runs at `0x100000`, never relocated. (If this guest were
ever made PIE, this absolute disp32 would silently address the wrong page — noted as a
future-proofing suggestion, not a current bug. crt0's `lea rsp,[stack_top]` uses the same
absolute form and is equally fine.)

## Cleared: ring store stays in bounds

`ring_buf` VMA `0x104140`, `resq 8192` = `0x10000` bytes → `[0x104140, 0x114140)`. `rdx`
starts 0, masked by `and rdx, 0x1fff` (8191) every iteration, so the index is always
`0..8191`. The store writes 8 bytes at `r12 + rdx*8`; worst case index 8191 →
`0x104140 + 8191*8 = 0x114138`, end `0x114140` = exactly the buffer end. **In bounds.** The
mask is applied *after* the increment, so even the wrap step never produces an out-of-range
index for the following store.

## Cleared: `and rdx, 8191` imm32 sign-extension is harmless

`48 81 e2 ff 1f 00 00` = `and rdx, 0x00001fff`. The imm32 is positive, sign-extends to
`0x0000000000001fff`. Correct mask. (Had the mask had bit 31 set, sign extension would matter
— it does not here.)

## Cleared: cmdline parser rejects non-digits correctly

`sub edx,'0'` then `cmp edx,9 / ja`:
- `':'` (0x3A) → 10 → `ja` taken → exits. Good.
- `' '` (0x20) → `0x20-0x30 = 0xFFFFFFF0` unsigned-huge → `ja` taken → exits. Good.
- Loop is bounded by `cmdline_len` (`dec r9d / jz`), no NUL semantics needed (cmdline is
  not NUL-terminated per the ABI). `cmdline_len == 0` → parse skipped → default. Good.
- `"0"` → parsed value 0 → `test rax,rax / jz .have_count` keeps DEFAULT, so the guest never
  instant-exits on a zero cmdline. Good defensive choice.
- Partial-flag hazards (`inc r8`/`dec r9d` writing partial RFLAGS before `jnz`): none — the
  branch consumes `dec r9d`'s ZF directly (`test r9d / jz` at loop top), and `inc/dec` here
  feed nothing that reads CF. Fine.

## Cleared: BSS layout — no overlap, deterministic

Linker concatenates crt0 `.bss` then landing `.bss`:

```
BOOT_INFO_PTR  0x100100  (resq 1, align 8)
stack_bottom   0x100110  (resb 16384)
stack_top      0x104110
ring_buf       0x104140  (align 64 → 0x104140, resq 8192) … end 0x114140
```

`stack_top (0x104110)` < `ring_buf (0x104140)` — **no overlap**; the `align 64` gap (48 bytes)
sits between them. PT_LOAD memsz `0x14140` (~80.3 KiB) from `0x100000`, filesz `0x0d8`
(text only). Loader must zero-fill `[filesz, memsz)` — already the documented LOADER CONTRACT
in `lib.rs`. Total memsz comfortably small.

## Cleared: LCG stream is meaningful

Simulated 16 iterations of `rax = rol((rax*M + INC), 13)` with M=6364136223846793005 (MMIX),
INC=1442695040888963407, seed `0x4448424900000001`: all 16 values non-zero and distinct
(`0x49b0287fa00f90cd`, `0xb14d5ec9550b046f`, …). State hashes over the ring will be
non-degenerate at any pause boundary. `rol rax, 13` encodes as a single `48 c1 c0 0d`
(`rol r64, imm8`) — one instruction.

## Cleared: DEFAULT_ITERS encoding

`mov rcx, 12500000` is emitted as `mov ecx, 0xbebc20` (32-bit form, zero-extends to RCX).
`0xbebc20 == 12500000`, fits in 32 bits, upper bits cleared. `8 * 12_500_000 = 100_000_000`,
matching `default_iters_hit_the_100m_budget`.

## Why nothing escalated to Important

The two real findings — the 13 retiring `align 16` NOPs and the per-cmdline-variable prologue
length — affect only the *constant offset* a harness adds to `8·N`. The change's own
`lib.rs` doc already tells harnesses to "calibrate the exact offset once, it is
deterministic," and that is true. So these are documentation-precision and
regression-guard items (Suggestions), not determinism or correctness breaks.
