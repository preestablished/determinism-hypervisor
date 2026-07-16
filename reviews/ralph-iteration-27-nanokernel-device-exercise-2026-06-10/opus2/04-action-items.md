# Action Items

Self-contained, ordered by severity. Each is actionable without re-reading the
other review files.

### Critical

- [ ] **Change the ring-W descriptor size from `0x1E0000` to `0x100000`
      (1 MiB).**
      File: `tests/nanokernel/asm/device_exercise.asm`, the line
      `mov dword [rbx + 0x2C], 0x1E0000 ; W size`.
      Reason: `0x1E0000` is not a power of two. The host's
      `Channel::attach` (`guest-sdk/.../detguest-host/src/channel.rs`) and
      `RingDesc::validate` (`guest-sdk/.../detguest-wire/src/header.rs`)
      reject non-power-of-two ring sizes, so attach returns
      `BadRingSize { ring: W }` → `IN 0xD37C` status 2 → the guest emits
      lowercase `d` and parks. With `0x100000` (the SDK's canonical
      `RING_W_SIZE`), attach succeeds and the Beacon at W offset 0 drains
      cleanly (verified by running the real SDK code: yields
      `Beacon { beacon_id: 0xB33F }`). No other line changes; the Beacon still
      sits at offset 0 and fits within 1 MiB.
      Also update the module-header comment to state the W size is `0x100000`
      and note the ARCHITECTURE.md table's `0x1E0000` is a non-power-of-two
      doc bug.

### Important

- [ ] **Add a deterministic test that the guest's channel header attaches.**
      `crates/dh-devices` already links `detguest-host`/`detguest-wire`. Add an
      integration test (in this repo) that writes the device-exercise guest's
      canonical 0x40-byte header into a `MockGuestMem` and asserts
      `Channel::attach(...).is_ok()`, then writes the Beacon (`len 24`,
      `kind 5`, `seq 0`, `beacon_id 0xB33F`, `prod = 24`) and asserts
      `drain_events` returns exactly one
      `OwnedPayload::Beacon { beacon_id: DEVICE_EXERCISE_BEACON_ID }` on ring W.
      Strongest form: assert the guest's header bytes equal
      `ChannelHeader::canonical().write_to(&mut [0u8; 0x40])` so offset/size/
      magic/version drift fails CI. This would have caught the Critical above,
      which shipped with all `cargo test -p nanokernel` green.

- [ ] **File a doc-bug bead against the ARCHITECTURE.md channel-layout table.**
      `.agents/docs/guest-sdk/ARCHITECTURE.md`'s layout table lists
      `ring W data (1,966,080 bytes = 0x1E0000)` while the same table states
      sizes are powers of two — a self-contradiction that directly caused the
      Critical. The guest-sdk resolved it in code (`header.rs` RING_W_SIZE doc,
      "Tracked as a spec documentation issue") but this repo's doc copy still
      misleads. Correct the row to `0x100000 (1 MiB; 0x120000..0x200000
      reserved)` or annotate it with the resolution, so the next clean-room
      reader does not reproduce the bug.

### Suggestions

- [ ] Validate `BootInfo` magic/version before trusting `mem_size` in the `D`
      stage (`bootinfo.inc` defines `BOOTINFO_MAGIC`/`BOOTINFO_VERSION` for
      exactly this). Belt-and-suspenders; deterministic either way.
- [ ] Add a call-site comment on the `'P'` pad stage noting it has no failure
      path by design (an MMIO fault would trap, not emit lowercase `p`), so
      nobody adds a bogus `.fail_p`.
- [ ] Optionally zero the upper RAX bits before `mov al, <char>` for debug-dump
      readability (functionally unnecessary — `putc` reads only AL).
- [ ] Clarify the Beacon `vnanos` comment to say the value is sampled at the
      `'C'` stage (deliberately stale; the host treats `vnanos` as opaque).
- [ ] Machine-check the `0x5453455547544544` magic literal against
      `b"DETGUEST"` (fold into the attach test above) so a hand byte-swap can't
      silently break it.
