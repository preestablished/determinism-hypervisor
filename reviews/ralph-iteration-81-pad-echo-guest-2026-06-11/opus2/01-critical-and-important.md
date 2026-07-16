# Critical and Important Findings

No Critical findings. Two Important.

---

## IMPORTANT 1 — The table is unbounded and runs off the end of guest RAM well before any plausible M5 horizon; no cap, mask, or documented capacity

`tests/nanokernel/asm/pad_echo.asm:40-46`

```asm
    mov     rcx, [r9]                ; r9 = TABLE_GPA = 0x30_0000
    lea     rdx, [r9 + 8 + rcx*8]
    mov     [rdx], r10d
    mov     [rdx + 4], eax
    add     rcx, 1
    mov     [r9], rcx
```

The frame loop is infinite and appends 8 bytes per frame with **no upper
bound** on `rcx`. The table grows from `0x30_0000` upward into guest RAM. The
guest doesn't read or bound its own count, so the only thing that stops the
growth is the write faulting when `rdx` leaves mapped RAM.

**The real overflow math** (the task asked for it):

Instructions per frame:
- Frame body (`.frame:` through `out dx, al`): `add, mov[FRAME], mov eax,
  mov rcx, lea, mov, mov, add, mov, mov dx, out` = **11**.
- Pace setup: `lea r12, xor ebx, mov r11d, mov rax` = **4**.
- Pace loop: `PACE_ITERS (64) × 6` = **384**.
- `jmp .frame` = **1**.
- **Total ≈ 400 instructions/frame.**

At the test-default guest size (`boot.rs` tests use `ram(64 << 20)` = 64 MiB,
the only size in the tree today) the table has `0x400_0000 - 0x30_0000 =
0x3D0_0000` ≈ 64.0 MB of room, i.e. ~8.0M entries. Reaching that takes
`8.0M × 400 ≈ 3.2e9` instructions ≈ **~3.2 s-vns at 1:1**.

Contrast with a 60 s-vns M5 run at 1:1: `60e9 / 400 = 150,000,000` frames,
needing `150M × 8 = 1.2 GB` of table — **~19× the entire 64 MiB guest.** The
guest exhausts RAM at roughly **5% of a 60 s run.**

**What happens at the boundary:** the loader maps `[0, mem_bytes)` plus one
2 MiB page over the MMIO hole at `0xD000_0000` (`boot.rs:145`,
`load_and_enter`). A store to a GPA at or above `mem_bytes` (64 MiB) but below
the MMIO hole hits **no memslot and no mapped page** — it surfaces as an EPT
violation / `KVM_EXIT_MMIO` to an address the bus reports `Unmapped` for. That
is an unexpected exit mid-table: the consuming M5 run loop will either fault the
slot or fall through its "unexpected exit" arm (`runctl.rs:716`
`BoundaryError::Exit`). Either way the acceptance run dies partway through the
table rather than completing.

**Why this is Important, not Critical:** nothing in *this diff* triggers it —
`pad_echo` is not yet wired into any acceptance/run test (grep confirms only
`lib.rs`, `build.rs`, `elf_shape.rs` reference it). It is a latent trap for the
next iteration that schedules the M5 run against this guest. The module comment
even advertises a "60 s-vns M5 run" framing in the broader work, which is
exactly the horizon this guest cannot survive.

**Fix options (pick one, document the choice):**
1. **Mask the index** so the table is a fixed ring:
   `and rcx, COUNT_MASK` before the `lea`, with `COUNT_MASK` a `%define` (e.g.
   `0x3FFFF` for 256 K entries = 2 MiB table) that is drift-pinned in `lib.rs`.
   This bounds RAM use forever and makes the table a deterministic fixed-size
   region — much friendlier to snapshot/replay than a region that grows without
   limit.
2. **Document a hard capacity** in the asm header and `lib.rs` (max frames vs a
   stated minimum `mem_bytes`), and require the M5 run to bound its
   icount/frame budget below that capacity. This is weaker — it pushes the
   invariant onto the run author with nothing enforcing it.

At minimum, the header comment's "table grows unbounded" reality must be stated
explicitly next to a concrete frame budget, so the run author cannot
accidentally schedule past it. Right now the comment says the loop is "the only
writer" and "count starts at 0" but says nothing about where it stops.

---

## IMPORTANT 2 — The drift pin is partial: REG_PAD0, REG_FRAME, SERIAL_PORT, and the 8-byte entry stride are not pinned against the device-side truth

`tests/nanokernel/tests/elf_shape.rs:248-250`, `tests/nanokernel/src/lib.rs:90-96`

The new drift test pins exactly three values:

```rust
assert_eq!(define("TABLE_GPA"), PAD_ECHO_TABLE_GPA);
assert_eq!(define("PACE_ITERS"), PAD_ECHO_PACE_ITERS);
assert_eq!(define("PAD_BASE"), 0xD000_1000); // dh_devices pad::PV_PAD_BASE
```

But the guest's correctness depends on **five** asm constants matching the
device, and the table layout depends on a stride that is not a `%define` at all:

- `REG_PAD0 0x08` must equal `dh_devices::pad::REG_PAD0` (= `0x08`).
- `REG_FRAME 0x1C` must equal `dh_devices::pad::REG_FRAME_COUNTER` (= `0x1C`).
- `SERIAL_PORT 0x3F8` must equal `dh_devices::serial::SERIAL_PIO_BASE`
  (= `0x3F8`).
- The **entry stride** is hardcoded twice in the asm — `lea rdx, [r9 + 8 +
  rcx*8]` (header offset `8` + `rcx*8` stride). `lib.rs` defines
  `PAD_ECHO_ENTRY_BYTES = 8` to mirror it, **but nothing pins the asm to that
  constant** — there is no `%define ENTRY_BYTES` to parse, and the test never
  asserts the asm's `8`/`*8` against `PAD_ECHO_ENTRY_BYTES`. If someone changes
  the host-side mirror to assume 12-byte entries, the drift test stays green and
  the host reads garbage.

These offsets/ports are the exact things a "drift pin" exists to protect:
silently re-mapping `REG_FRAME_COUNTER` device-side, or moving the serial base,
would make this guest write to the wrong register with **no test failure** —
the guest would still assemble, still pass `elf_shape`, and produce a wrong
table or no FRAME_MARK at run time, which is the worst kind of M5 regression
(deterministic-but-wrong).

**Fix:**
1. Add `%define REG_PAD0`, `%define REG_FRAME`, `%define SERIAL_PORT` parsing to
   the test and pin them against `dh_devices::pad::REG_PAD0`,
   `pad::REG_FRAME_COUNTER`, `serial::SERIAL_PIO_BASE`.
2. Introduce `%define ENTRY_BYTES 8` (and `%define HEADER_BYTES 8`) in the asm,
   use them in the `lea`, and pin them against `PAD_ECHO_ENTRY_BYTES`. A
   hardcoded literal `8` inside an addressing mode is invisible to the drift
   pin; the whole point of the iteration's `lib.rs` mirror is to make it
   visible.

This is feasible without a dependency cycle (see Suggestion 1 — `nanokernel`
can dev-dep `dh-devices`), but even the literal-constant pin (asserting against
`0x08`/`0x1C`/`0x3F8` with a sourced comment, as the existing `PAD_BASE` line
does) closes the gap.
