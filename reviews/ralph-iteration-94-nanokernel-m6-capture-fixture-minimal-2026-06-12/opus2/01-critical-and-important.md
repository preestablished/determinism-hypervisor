# Critical & Important Issues

**None.**

I actively probed the failure modes the prompt flagged and several of my own, and found no
Critical or Important defects. Recording the negative results, since "I checked X and it's
fine" is itself useful signal:

## Verified safe (would have been bugs)

1. **`rep movsb` direction flag** — `asm/capture_fixture.asm:160-164`. `rep movsb` copies the
   11-byte region name forward, which is only correct with DF=0. `crt0.asm:21` issues `cld`
   before `call prog_main`, and nothing in the fixture sets DF, so forward copy is
   guaranteed. Safe.

2. **63 zeroed manifest entries counting as "live"** — A zeroed `RegionEntry` has `flags=0`,
   so `is_live()` returns `true` (`detguest-wire/manifest.rs:179-181`, DEAD = bit 31). I
   traced every host path that iterates all 64 slots:
   - `read_manifest` (`detguest-host/manifest.rs:97-99`) runs `validate_extents` on **every
     live entry**. A zeroed entry has `extent_off=0, extent_n=0`, so `0.checked_add(0)=0 <=
     extent_count(1)` → `Ok`. No reject.
   - `resolve` (`detguest-host/manifest.rs:47-51`) matches on `name_bytes() == name`. Zeroed
     entries have empty `name_bytes()`, so they never match `"framebuffer"` and never shadow
     the real slot-0 entry.
   - Confirmed empirically: `read_region` / `read_manifest` succeed in the interop test.
   No host path chokes. Safe.

3. **`CHANNEL_INIT` page count + alignment** — `dh-devices/detchannel.rs:399-410` requires
   `size_pages == CHANNEL_SIZE_PAGES` (512) **and** 2 MiB GPA alignment. The fixture donates
   `CHANNEL_PAGES = 512` (`capture_fixture.asm` `%define`) at `CHANNEL_GPA = 0x400000` (2 MiB
   aligned). Matches `device_exercise.asm` exactly. Safe.

4. **Channel page vs framebuffer overlap** — Channel is the 2 MiB page `0x400000..0x600000`;
   the framebuffer extent is `0x600000..0x610000`. They are exactly adjacent, never
   overlapping, so a capture read of `"framebuffer"` cannot see ring traffic. The interop
   test pins this with `const _FB_CLEAR_OF_CHANNEL` (`elf_shape.rs`), and the
   `mem_size >= FB_GPA + FB_BYTES` guard at `capture_fixture.asm:62-66` is the runtime
   counterpart. Safe.

5. **Generation stays even (0)** — The guest writes the whole manifest **before**
   `CHANNEL_INIT`, leaving `generation = 0` (zeroed RAM, even = quiescent). The host's first
   reader runs at attach, after all stores are in place, so no seqlock dance is needed. The
   interop test asserts `generation == 0`. The host reader (`manifest.rs:75-83`) accepts an
   even, unchanged generation. Safe — and the in-source comment justifying skipping the
   seqlock is correct.

6. **Mirror-vs-codec drift** — `capture_fixture_asm_matches_rust_constants`
   (`elf_shape.rs:330+`) compares the asm's restated manifest constants against the **actual**
   `detguest_wire` symbols (`MANIFEST_MAGIC`, `OFF_MANIFEST`, `RegionEntry::offset(0)`,
   `Extent::offset(0)`, `REGION_FLAG_FRAMEBUFFER`), not hand-typed numbers. This is the right
   pattern and means the asm's `0x20`/`0x1820` cannot silently diverge from the codec. Safe.

All of the above are confirmed by a clean `cargo test --test elf_shape --test
capture_manifest_interop` (13 tests green) and `cargo clippy --tests` (no warnings).
