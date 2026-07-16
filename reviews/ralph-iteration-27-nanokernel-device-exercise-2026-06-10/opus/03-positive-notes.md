# Positive Notes

### P1 — Every pv-device register offset, width, and status code is exactly right

Verified against the live models and confirmed in the disassembly:

- **pv-clock** (`clock.rs`): base `0xD000_0000`, `VNS 0x08` and `ICOUNT 0x10`
  both read as 8-byte MMIO (`mov rax, [rbx+0x8]`, `mov r8, [rbx+0x10]`) — matches
  `REG_VNS`/`REG_ICOUNT` served as u64.
- **pv-entropy** (`entropy.rs`): base `0xD000_2000`, `BUF_GPA 0x08` (8B write),
  `LEN 0x10` (4B), `DOORBELL 0x14` (4B write triggers synchronous fill),
  `STATUS 0x18` (4B), `STATUS_OK == 1`. All widths match the model's
  `mmio_write`/`mmio_read` arms exactly.
- **pv-pad** (`pad.rs`): base `0xD000_1000`, `PAD0 0x08` read as 4B — matches
  `REG_PAD0` and the `(off-REG_PAD0)/4` latch index.
- **pv-blk** (`blk.rs`): base `0xD000_4000`, `SECTOR 0x08` / `BUF_GPA 0x10` as 8B
  writes, `COUNT 0x18` / `CMD 0x1C` as 4B writes, `STATUS 0x20` 4B read,
  `CMD_WRITE 2` / `CMD_READ 1`, `STATUS_OK == 0`. All correct.

### P2 — Synchronous-completion contract is respected

Both the entropy doorbell and the blk CMD register complete their work *inside*
the MMIO-write VM exit (`entropy.rs::doorbell`, blk's "STATUS valid when the CMD
write's VM exit completes"). The guest reads STATUS on the very next instruction,
which is exactly the right discipline — no polling, no spurious wait.

### P3 — The icount non-decreasing check is correctly reasoned

Two consecutive 8-byte reads of `REG_ICOUNT`, each a separate VM exit, with
`cmp r9, r8; jb .fail_c`. `jb` (unsigned below) is the right failure predicate,
and equality is tolerated — important, because the model serves `ctx.icount`
stamped per exit and the VMM advances icount between exits, so equal-or-greater is
the only sound expectation. The comment ("icount regressed across exits") is
accurate.

### P4 — BootInfo access and the mem_size guard are correct

`mov rsi, [BOOT_INFO_PTR]` loads the pointer crt0 stashed from RSI, the null guard
(`test rsi, rsi; jz .fail_d`) is sound, and `[rsi + BOOTINFO_OFF_MEM_SIZE]`
(0x08) reads the right field per `bootinfo.inc`. The threshold
`CHANNEL_GPA + 0x200000` correctly demands room for the full 2 MiB channel page
above the donated GPA.

### P5 — Channel header magic, version, ring order, and index discipline match the wire crate

`magic = 0x5453455547544544` decodes to "DETGUEST" LE (confirmed by
`detguest_wire::header::CHANNEL_MAGIC` and its own
`magic_bytes_spell_detguest` test), `proto_version = 1`, `flags = 0`, ring_desc
order C/I/A/W at `0x10` with `{offset,size}` pairs, and the `ringW_prod` store at
`+0x280` all match `header.rs` (`OFF_MAGIC`, `OFF_PROTO_VERSION`,
`OFF_RING_DESC`, `OFF_RING_W_PROD`). The index cells are left zeroed, which is
correct since guest RAM boots zeroed and the producer index publish is the only
W-side write. (Only the W *size* value is wrong — see C1.)

### P6 — Beacon record framing matches API.md §3.0/§3.2 precisely

`len = 24` (16 header + 8 payload, multiple of 8, within 16≤len≤4096),
`kind = 5` (Beacon), `flags = 0`, `seq = 0`, `vnanos` at +8, payload
`beacon_id u32` at +16 and `_pad u32` at +20. Matches `record.rs`
(`RECORD_HEADER_LEN = 16`, `MIN_RECORD_LEN = 16`) and the Beacon payload spec
(8 bytes: `beacon_id` + `_pad`). The record sits at ring W offset 0 with the
producer index published afterward — correct producer release ordering for a
single vCPU.

### P7 — Clean asm hygiene and faithful clean-room sourcing

- `putc` clobbers only DX (`out dx, al` does not touch AL); AL is preserved across
  the documented call sites.
- `loop`/`rcx` fill and compare loops are correct (rcx=64 qwords = 512 bytes),
  with `lea`-based absolute addressing of `.bss` symbols (valid for the non-PIE
  static exec confirmed by `elf_shape.rs`).
- 4-byte vs 8-byte operand widths are right everywhere in the disassembly
  (`DWORD` stores for 4B regs/fields, `QWORD`/`movabs` for 8B).
- Failure letters map to the correct stages (`c/e/b/d`); 'p' is intentionally
  absent because the pad stage is a pure read that cannot fail. No fall-through
  hazards between stages — each failure path `jmp`s to a shared `putc`+`ret`.
- The module header correctly distinguishes repo-owned device maps (mirrored from
  `crates/dh-devices`) from the clean-room channel layout (from
  `.agents/docs/guest-sdk/` only), and does not leak guest-sdk internals beyond
  the documented wire layout. (The one issue is that it transcribed the *wrong*
  W-size from a self-contradictory doc table — see C1.)
- `lib.rs` constants are internally consistent: `DEVICE_EXERCISE_CHANNEL_GPA =
  0x40_0000` equals the asm `CHANNEL_GPA 0x400000`, `DEVICE_EXERCISE_BEACON_ID =
  0xB33F` equals the asm `0xB33F`, and `DEVICE_EXERCISE_OK_SEQUENCE = b"CEPBDX"`
  matches the documented uppercase stage letters.
