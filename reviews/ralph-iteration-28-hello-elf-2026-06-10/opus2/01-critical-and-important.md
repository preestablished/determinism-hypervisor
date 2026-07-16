# Critical and Important findings

**None.**

I checked each angle that could plausibly hide a Critical/Important bug in a
30-line serial stub and cleared all of them against the disassembly:

## 1. Missing real-mode→long-mode transition — NOT a bug (cleared)

Already covered in 00-overview. ARCH §2.3 makes long-mode setup the VMM's job
(`KVM_SET_SREGS`, `RIP = e_entry`). A freestanding ELF entered at `e_entry` is
already in long mode; `BITS 64` with no mode-switch code is exactly correct, and
the file header documents the divergence from the stale plan wording. No action.

## 2. DF / `lodsb` direction — cleared

`lodsb` post-increments `rsi` only when DF=0. crt0 executes `cld` at `0x100010`
immediately before `call prog_main` (`0x100011`), and nothing between that `cld`
and the `lodsb` touches DF — `prog_main` does only `lea`/`mov`/`mov`. So DF=0 is
guaranteed at the first `lodsb` and the six bytes are read forward. (Confirmed in
the disassembly: `_start` → `cld` → `call`; `prog_main` has no `std`/`pushf`/
flag-clobber.) No action.

## 3. Section math / `msg` address — cleared

`lea 0x100040,%rsi` is an absolute address baked at link time. `objdump -h`
confirms `.rodata` VMA is `0x100040` and `nm` confirms `msg = 0x100040` — they
match exactly. `.text` (ALIGN 4096, ends `0x100036`) and `.rodata` (ALIGN 16 →
`0x100040`) both live inside the first page `0x100000–0x100fff`, i.e. the single
RWE `PT_LOAD` the linker script and `elf_shape` enforce. The absolute `lea`
resolves to mapped, loaded memory. No action.

## 4. `HELLO_SERIAL_OUTPUT` literal correctness — cleared

The diff defines `pub const HELLO_SERIAL_OUTPUT: &[u8] = b"HELLO\n";` — a plain
Rust byte-string literal, **not** a python-generated escape (the prompt's framing
does not match what is checked in; the simpler literal is what shipped). It is 6
bytes, `48 45 4C 4C 4F 0A`, ending `0x0A`. This is byte-identical to the assembled
`.rodata` (`objdump -s -j .rodata`: `48454c4c 4f0a`) and consistent with
`MSG_LEN 6`. No action.

## 5. `out dx, al` byte-not-line-buffered serial — cleared

The program writes raw bytes one `out` at a time to PIO `0x3F8`; the harness greps
raw serial bytes, so unbuffered single-byte writes are the intended contract
(matches `pipeline_smoke.asm`'s single-byte `out`). No action.
