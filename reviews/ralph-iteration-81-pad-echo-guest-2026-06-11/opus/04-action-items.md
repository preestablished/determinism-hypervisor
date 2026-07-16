# Action items

### Critical

_None._

### Important

_None._

### Suggestions

- [ ] **S1 — Bound or document the table capacity.** The frame loop appends 8 bytes/frame
  unbounded; capacity before colliding with `0x40_0000` is 131 071 frames (`0x10_0000 - 8`
  ÷ 8). The M5 60s-vns/~3 600-frame run is far under this, so it is not a blocker — but
  add a `PAD_ECHO_TABLE_MAX_FRAMES` const in `tests/nanokernel/src/lib.rs` documenting the
  cap, or cap the stored count in `tests/nanokernel/asm/pad_echo.asm` (compare `rcx`
  against a `MAX_FRAMES` `%define`, stop appending but keep ticking FRAME_COUNTER/serial).

- [ ] **S2 — Remove or annotate the dead `and ebx,511`** in the `.pace` loop of
  `tests/nanokernel/asm/pad_echo.asm`. `ebx` only reaches 63 per frame so the mask never
  fires and `work_buf` (512 qwords) is never over-indexed. Drop it (and optionally shrink
  `work_buf`), or comment it as a vestigial mirror of the timer_guest spin idiom.

- [ ] **S3 — Pin the per-frame / `.pace` instruction shape in the drift test.** Frame
  boundaries land at a fixed icount only if `PACE_ITERS` *and* the instruction counts in
  `.frame`/`.pace` stay fixed. Extend `pad_echo_asm_matches_rust_constants` in
  `tests/nanokernel/tests/elf_shape.rs` to assert `.pace` is exactly 6 instructions
  (mirroring the `rep_loop`/`landing_loop` drift tests), so an accidental body edit that
  would invalidate the M5 icount baseline is caught by CI instead of by a failed re-record.

- [ ] **S4 — Pin the remaining register/port offsets.** Add
  `assert_eq!(define("REG_PAD0"), 0x08)`, `REG_FRAME → 0x1C`, and `SERIAL_PORT → 0x3F8`
  to `pad_echo_asm_matches_rust_constants` in `tests/nanokernel/tests/elf_shape.rs` (the
  `define` closure already parses them) so the asm offsets cannot drift from
  `dh-devices` `pad::REG_PAD0` / `pad::REG_FRAME_COUNTER` / `serial::SERIAL_PIO_BASE`.

### Scope note for bead a5e (not an action on this PR)

The bead 29a text says the guest "polls pv-pad latch(es)" (plural) but pad_echo polls
only `PAD0`, and it is polling-only (never enables `IRQ_VECTOR`). Both are acceptable
for `a5e` as written: the M5 accept (IMPLEMENTATION-PLAN §M5) specifies a "scripted pad
sequence" and ARCH §6.4 states the demo harness *polls per frame* with the edge
interrupt *disabled by default*, and a single-port scripted sequence on port 0 satisfies
"guest-visible state the hash chain covers." If `a5e` later wants multi-port scripts or
to exercise the edge-interrupt path (`IRQ_VECTOR` + an IDT), that is a follow-up guest
variant (cmdline-selected, like timer_guest's `mask`/`defer`/`arm` modes), not a defect
in this prep guest. Capture it on `a5e` rather than blocking 29a.
