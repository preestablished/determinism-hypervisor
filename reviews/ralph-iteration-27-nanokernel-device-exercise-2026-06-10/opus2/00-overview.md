# Review Overview — iteration 27: nanokernel device-exercise guest

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-27-nanokernel-device-exercise` vs `main`
- **Bead:** 7ys — the M1 acceptance guest
- **Scope:** `tests/nanokernel/asm/device_exercise.asm` (+ build wiring in
  `build.rs`, `src/lib.rs`, `tests/elf_shape.rs`)
- **Angle:** x86-64 instruction pedantry + host-side interop skepticism.
  I built the ELF (`cargo test -p nanokernel`), disassembled it
  (`objdump -d`), and **executed the real guest-sdk attach/drain code**
  against the guest's exact channel-page bytes in a scratch
  `detguest-host` integration test (since removed).

## Summary

The guest is a clean, well-commented single-vCPU acceptance program: touch
pv-clock / pv-entropy / pv-pad / pv-blk, then drive the detchannel
(CHANNEL_INIT → one ring-W Beacon → doorbell), emitting one serial byte per
stage. The device register maps, port numbers, record framing, and detcall
ABI widths are all correct against this repo's `crates/dh-devices` constants
and the guest-sdk wire crate. The instruction-level encoding is clean: IN/OUT
widths, `dx` preservation across the OUT→IN sequences, `loop`/`rcx`, GPA
store widths, and the `mem_size` constant fold all check out.

**But the program cannot pass its own acceptance.** The ring-W descriptor it
writes into the channel header uses size **`0x1E0000`**, which is **not a
power of two**. The host it must interop with (`Channel::attach` in
`guest-sdk/.../channel.rs`, backed by `RingDesc::validate` in
`detguest-wire/.../header.rs`) rejects any non-power-of-two ring size. Attach
returns `Err(BadRingSize { ring: W })` → `init_status() = BadMagicVersion`
(status **2**). The guest reads nonzero from `IN 0xD37C`, takes the `.fail_d`
path, emits lowercase **`d`**, and parks. The serial log is **`CEPBd`**, never
the required **`CEPBDX`**. The Beacon and doorbell code after attach is dead.

I proved both halves by running the real SDK code:
- `attach(W=0x1E0000)` → `Err(BadRingSize { ring: W })`.
- `attach(W=0x10_0000)` (the SDK-canonical 1 MiB size) + drain → exactly one
  `Beacon { beacon_id: 0xB33F }`, ring W, seq 0. So every *other* byte the
  guest writes (len 24, kind 5, seq 0, arbitrary vnanos, 8-byte payload) is
  correct — the W ring size is the sole defect.

### Root cause

The guest was clean-roomed from `.agents/docs/guest-sdk/ARCHITECTURE.md`,
whose "Channel memory layout" table is **internally contradictory**: it states
"Indices are free-running `u32`, masked by `size - 1` (sizes are powers of
two)" two lines above giving `ring W data (1,966,080 bytes = 0x1E0000)` — a
non-power-of-two. The guest author copied the literal table value and missed
the invariant. The guest-sdk already resolved this contradiction in code
(`header.rs` RING_W_SIZE doc comment, lines 92–103) in favour of the
power-of-two rule, sizing W at **`0x10_0000` (1 MiB)** with `0x12_0000..`
reserved. The fix is a one-line change in the guest; the doc bug should be
filed separately so the next clean-room reader doesn't repeat it.

## Verdict

**REQUEST CHANGES.** One critical interop defect blocks the program's entire
reason to exist (the `D`/`X` stages). The fix is trivial and surgical. No
other blocking issues; the rest of the program is solid.

## Stats

| Severity   | Count |
|------------|-------|
| Critical   | 1     |
| Important  | 2     |
| Suggestions| 5     |
| Positive notes | 7 |

- Critical: ring-W size `0x1E0000` is rejected at attach (guest never reaches
  `D`/`X`).
- Important: (1) no automated assertion that the channel header the guest
  writes actually passes `Channel::attach` — the bug shipped green; (2) the
  ARCHITECTURE.md layout-table contradiction that misled the author is
  un-tracked here.
