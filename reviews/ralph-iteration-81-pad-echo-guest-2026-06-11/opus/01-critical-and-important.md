# Critical and Important findings

**None.**

No correctness defects, determinism hazards, or ABI/drift problems were found. The
assembly is register-correct across MMIO exits and pace iterations, the serial echo
carries the right byte through a deterministic+logged path, the FRAME_COUNTER write
satisfies the §6.4 monotonicity contract from a fresh device, and the new drift test
plus elf_shape pin pass (`cargo test -p nanokernel --test elf_shape` → 7/7).

The detailed correctness walk-through that backs this "none" verdict is in
`00-overview.md` ("Correctness findings"). The forward-looking robustness and scope
notes — none of which block landing this prep guest — are in `02-suggestions.md`.

## Why the close calls resolve clean (recorded for the next reviewer)

These are the points the task flagged as "be pedantic"; each was checked and is fine.

- **`al` == pad0 low byte at `out dx, al`.** Order is: `mov eax,[r8+REG_PAD0]` →
  table append (reads `eax` into `[rdx+4]`, writes only `rcx`/`rdx`/`r10d`) →
  `mov dx, SERIAL_PORT` (writes only `dx`, the low 16 bits of `rdx`) → `out dx, al`.
  Nothing overwrites `eax`/`al` between the latch read and the OUT. Correct.

- **`r8`/`r9`/`r10` survive the pace loop across iterations.** The pace loop clobbers
  `rax`/`rbx`/`r11`/`r12` only; PAD_BASE (`r8`), TABLE_GPA (`r9`), and F (`r10d`) are
  untouched, so the next frame's MMIO and table-append see correct values. Confirmed.

- **Table addressing math.** `lea rdx,[r9 + 8 + rcx*8]` with `rcx = count` writes the
  frame u32 at `+0` and pad0 u32 at `+4`, then bumps and stores `count` — exactly the
  documented `0x00 u64 count` then `frame|pad0` 8-byte entries. Correct.

- **`and ebx,511` is dead but harmless.** `ebx` restarts at 0 each frame and only
  reaches `PACE_ITERS-1 = 63`, so the mask never fires; `work_buf` is 512 qwords so
  even unmasked indexing stays in bounds. Dead-but-safe (carried over from the
  timer_guest spin idiom). Noted as a tidy-up in suggestions, not a defect.

- **FRAME_COUNTER from a fresh device.** `frame_counter` starts at 0; the guest writes
  1,2,3,… strictly increasing, each logging a FRAME_MARK — satisfies the monotone
  contract. `xor r10d,r10d` resetting F to 0 each boot is correct for a from-snapshot
  M5 run because absolute carry-over lives in the *device's* `frame_counter` (snapshotted
  in PADD), not in the guest register.
