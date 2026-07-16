# Suggestions (non-blocking)

### S1 — Replace hard-coded channel offsets/sizes with named `%define`s tied to the wire constants

**File:** `tests/nanokernel/asm/device_exercise.asm:160-177, 191-206`

The header build uses bare literals (`[rbx + 0x10]`, `0x8000`, `0x4000`, …,
`[rbx + 0x280]`, `[rdi + 0x8]`). These mirror `detguest_wire::header::OFF_*`,
`RING_*_SIZE`, and `record::*`. Promoting them to `%define`s named after the wire
constants (e.g. `OFF_RING_DESC 0x10`, `OFF_RING_W_PROD 0x280`,
`OFF_RING_W_DATA 0x20000`, `RING_W_SIZE 0x100000`, `RECORD_HEADER_LEN 16`)
documents intent and makes the C1 fix self-evidently correct. It also reduces the
chance the next editor mis-aligns a descriptor field by hand.

### S2 — Add a build-time consistency assertion between the asm channel constants and the Rust/wire constants

**Files:** `tests/nanokernel/tests/elf_shape.rs`, `tests/nanokernel/src/lib.rs`

`elf_shape.rs` already parses `bootinfo.inc` `%define`s and asserts they match the
Rust constants (`bootinfo_inc_matches_rust_constants`,
`landing_loop_asm_matches_rust_constants`). Extend the same pattern: assert the
asm's channel `%define`s (once S1 lands) equal `detguest_wire::header::CHANNEL_*`
/ `RING_W_SIZE` / `RECORD_HEADER_LEN` and that `DEVICE_EXERCISE_CHANNEL_GPA`
(0x40_0000) matches the asm `CHANNEL_GPA`, `DEVICE_EXERCISE_BEACON_ID` (0xB33F)
matches the asm beacon id. This makes a future spec-table change impossible to
mis-transcribe silently.

### S3 — Tolerate `vns_sample == 0` explicitly, or sample ICOUNT for `vnanos`

**File:** `device_exercise.asm:78, 195-202`

`vns_sample` is captured from `[CLOCK_BASE + VNS]` at the very start. On a
fresh-boot pv-clock with `vns_base = 0` at low icount, `vns()` can legitimately
return 0 (`clock.rs::vns` is `base + icount*num/den`). The Beacon then carries
`vnanos = 0`, which the drain accepts (it is opaque), so this is harmless today —
but the comment "vnanos (sampled)" implies a meaningful nonzero stamp. Either note
that 0 is acceptable, or sample `REG_ICOUNT` (always nonzero by the time the
record is written) if a visibly nonzero stamp is desired for log inspection.

### S4 — Document why `IN` immediately follows `OUT` on the same `dx` for INIT_GO and DOORBELL

**File:** `device_exercise.asm:183-184, 205-206`

`out dx, eax` then `in eax, dx` with `dx` unchanged is correct (INIT_GO and
DOORBELL answer status/0 on the same port — verified against the API.md §5 ABI
table and `detchannel.rs::pio_in`). A one-line comment that the detcall ports are
read/write on the same address (unlike a typical command/status split) would help
the next reader who expects a separate status port. Minor.
