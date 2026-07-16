# Suggestions (non-blocking)

## S1 — Unbounded table growth: document the capacity or cap the count

The frame loop appends 8 bytes/frame forever with no bound. The table window before it
collides with the next named GPA region is:

- table base `0x30_0000`, next region `0x40_0000` (`DEVICE_EXERCISE_CHANNEL_GPA`)
- usable span = `0x10_0000 - 8` (header) = `1 048 568` bytes ÷ 8 = **131 071 frames**

For the actual M5 scenario this is comfortable: a 60s-vns run at ~60 fps is ~3 600
frames (~28 KiB of table), two orders of magnitude under capacity. So **this is not a
hazard for `a5e` as specified** — but two things make it worth a guard or a doc line:

1. The collision target (`0x40_0000`) belongs to a *different* guest (device_exercise),
   so in practice running off the end just walks into higher RAM that is valid up to
   `mem_bytes` (16 MiB in tests). Still, an unbounded guest writer silently corrupting
   GPA above its table on a longer-than-expected run is a latent foot-gun.
2. There is no `lib.rs` constant or asm comment stating the table's frame capacity.

**Pick one:**
- (cheap, preferred) add a `PAD_ECHO_TABLE_MAX_FRAMES` const to `lib.rs` documenting the
  131 071-frame capacity at `PAD_ECHO_ENTRY_BYTES`, and a one-line asm comment that the
  loop assumes the M5 frame budget stays well under it; **or**
- (defensive) cap the stored count in asm (e.g. compare `rcx` against a `MAX_FRAMES`
  `%define` and stop appending — keep incrementing FRAME_COUNTER/serial so frame pacing
  and the FRAME_MARK stream are unaffected). The drift test could then pin MAX_FRAMES too.

## S2 — `and ebx,511` in the pace loop is dead code

`ebx` resets to 0 each frame and only counts to `PACE_ITERS-1 = 63`, so the mask never
changes `ebx`; `work_buf` (512 qwords) is never indexed out of range. The instruction is
inherited from the timer_guest spin idiom (where the wrap *is* load-bearing because that
loop is unbounded). Here it is harmless but misleading. Either drop it (and shrink
`work_buf` toward `PACE_ITERS`), or add a comment that it is a vestigial guard kept only
to mirror the timer_guest body. Note: if you keep the asm↔Rust instruction-count style
of the landing_loop/rep_loop drift tests in mind, removing it would change the
per-iteration instruction count — but the pad_echo drift test pins `PACE_ITERS`/GPA, not
the body instruction count, so removal is safe today.

## S3 — Consider pinning the per-frame instruction shape, like landing_loop/rep_loop

`landing_loop` and `rep_loop` drift-tests count the loop-body instructions so the
documented per-iteration icount cannot silently drift. pad_echo's determinism for `a5e`
depends on the frame boundary landing at a fixed icount, which is a function of BOTH
`PACE_ITERS` and the count of instructions in `.pace` (6) and in the per-frame prologue.
The current drift test pins `PACE_ITERS` and the GPAs but not the instruction counts. If
a future edit adds/removes an instruction in `.frame` or `.pace`, frame-boundary icounts
shift and the M5 baseline would need re-recording with no test to flag it. Consider
extending `pad_echo_asm_matches_rust_constants` to also assert the `.pace` body is
exactly 6 instructions (mirroring the rep_loop/landing_loop approach) and/or pin the
per-frame instruction count behind a `PAD_ECHO_*_INSTRS` const.

## S4 — Drift test could also pin REG_PAD0 / REG_FRAME / SERIAL_PORT

The new test asserts `PAD_BASE == 0xD000_1000` against the dh-devices window, which is
great. The register offsets `REG_PAD0 (0x08)` and `REG_FRAME (0x1C)` and `SERIAL_PORT
(0x3F8)` are equally load-bearing ABI to dh-devices (`pad::REG_PAD0`,
`pad::REG_FRAME_COUNTER`, `serial::SERIAL_PIO_BASE`) and are currently unpinned. Adding
three more `assert_eq!(define("REG_PAD0"), 0x08)`-style lines (the `define` closure
already parses them) would close the remaining asm↔device drift surface cheaply.
