# Positive Notes

## 1. `al` correctly survives the table append to reach `out dx, al` — verified instruction by instruction

The task flagged this as the load-bearing pedantic check. Trace from the PAD0
read to the serial echo:

```asm
    mov     eax, [r8 + REG_PAD0]     ; al = pad0 low byte
    mov     rcx, [r9]                ; rcx — not al
    lea     rdx, [r9 + 8 + rcx*8]    ; rdx — not al
    mov     [rdx], r10d              ; memory write, reads r10d
    mov     [rdx + 4], eax           ; memory write, READS eax (doesn't clobber)
    add     rcx, 1                   ; rcx
    mov     [r9], rcx                ; memory write
    mov     dx, SERIAL_PORT          ; 16-bit load into dx; AL untouched
    out     dx, al                   ; emits pad0's low byte ✓
```

Nothing between the read and the `out` writes `al`/`eax`. `mov dx, SERIAL_PORT`
is a 16-bit move into the low half of `rdx` and leaves `rax`/`al` intact. The
subsequent `mov rax, 0x9AD5` clobbers `eax` but only *after* the `out`. The
echoed byte is exactly the polled pad0 low byte. Correct.

## 2. Torn-read discipline is right: entry written before the count increment

```asm
    mov     [rdx], r10d              ; frame
    mov     [rdx + 4], eax           ; pad0   — both entry words first
    add     rcx, 1
    mov     [r9], rcx                ; THEN publish the new count
```

A host sampling the table mid-run sees count = N only after entries `[0, N)`
are fully written. The header is the publish barrier; no host can observe a
count that points at a half-written entry. This is the correct ordering for a
single-writer / concurrent-reader table.

## 3. Register-width hygiene is clean throughout

`xor r10d, r10d` zeroes all 64 bits of `r10` (x86-64 zero-extension on 32-bit
ops), so the frame counter starts truly at 0. `mov eax, [...]`, `add r10d, 1`,
`mov r11d, ...`, `add ebx, 1`, `sub r11d, 1` all use 32-bit ops that zero the
upper halves — no high-half garbage leaks into addressing or the table. The MMIO
stores/reads (`r10d` at `+0x1C`, `eax` from `+0x08`) are 4-byte, exactly what
the pv-pad device requires (`pad.rs:99,113` reject non-4-byte access).

## 4. MMIO offsets are 4-byte naturally aligned and land in the right device window

`PAD_BASE + 0x1C` (FRAME_COUNTER) and `PAD_BASE + 0x08` (PAD0) are both `% 4 ==
0`, within `WINDOW_LEN` (4096), so they pass `MmioBus::check_access`
(`bus.rs:75-83`) and dispatch to `PvPad`. Offset `0x1C` is `>= REG_DEVICE_BASE`
so the write isn't rejected as read-only. The device accepts any FRAME value and
logs the FRAME_MARK (`pad.rs:126-129`) — monotonicity is run-control's job, and
the guest writes strictly increasing values anyway. All consistent with the §6.4
register map.

## 5. The drift test parser is correct and fail-loud

The `find_map` + `.then(|| ...)` closure returns `Some(value)` only on an
exact-token `%define NAME` match (`split_whitespace`, not substring), and
`unwrap_or_else(|| panic!(...))` makes a missing define a hard test failure
rather than a silent skip. Hex (`strip_prefix("0x")` → radix 16) and decimal
branches both handle the actual values (`0x300000`, `64`, `0xD0001000`). It
mirrors the proven `bootinfo_inc_matches_rust_constants` style. (Its only flaw
is coverage breadth — see Important 2 — not correctness.)

## 6. Skipping hello.asm's LSR-THRE wait is sound here — it changes neither correctness nor determinism

`hello.asm` spins on `LSR (0x3FD) & THRE` before each byte because it follows
real 16550 driver discipline. `pad_echo` omits it and fires `out dx, al`
unconditionally. That is **safe** against this platform's `DebugSerial`: the
model is output-only, `LSR` always reads `0x60` (THRE+TEMT ready,
`serial.rs:32,58-65`), and `pio_write` to THR never blocks or back-pressures —
it just `extend_from_slice`s into a host-side observability buffer
(`serial.rs:73-78`). The run loops dispatch `IoOut` on `0x3F8..0x400` directly
to the serial sink (`kvm.rs:479-493`, `runctl.rs:710-714`) with no flow control.
So the LSR wait is pure driver-convention cargo against a never-busy device;
dropping it removes instructions from the per-frame icount but does not perturb
determinism (the output stream and the table are identical run-to-run). One
nuance worth noting (not a defect): until a `PAD_SET` lands, PAD0 reads the
default latch `0`, so the serial stream emits NUL bytes — harmless, captured
verbatim in the JSON log, never hashed (serial state is excluded from the state
hash by design, `serial.rs:8-11,122-123`).
