# Suggestions (non-blocking)

### S1 — `at = phoff + i * phentsize` is unchecked `usize` arithmetic

`crates/dh-vmm/src/boot.rs:106` computes the per-phdr offset with a plain add/multiply:

```rust
let at = phoff + i * phentsize;
```

`phoff` (u64→usize), `phentsize` (u16), and `phnum` (u16) are all attacker-controlled
header fields. On a 64-bit host this cannot realistically overflow `usize` for any input
that also satisfies the later bounds checks, but it is not *defensively* impossible: in a
release build the add/mul wraps silently, after which `elf.get(at..at+4)` would
bounds-check a wrapped (small) offset and read a phdr from the wrong place — a
wrong-but-memory-safe parse, returning either `Err` or a bogus segment that the
subsequent `write_slice`/`p_vaddr` checks would still catch. In a debug build it panics
on overflow (fail-closed, acceptable). This is loader-of-untrusted-input code, so prefer
explicit `checked_mul`/`checked_add` returning `bad("phdr table overflow")`, matching the
care already taken for `p_offset + p_filesz` two lines down.

### S2 — the MMIO-hole PTE over-maps beyond `MMIO_HOLE_LEN`

`write_page_tables` installs one 2 MiB PTE covering `0xD000_0000..0xD020_0000`, but the
documented hole (`MMIO_HOLE_LEN = 0x7000`) is only `0xD000_0000..0xD000_7000`. The extra
`0xD000_7000..0xD020_0000` is "present" in the page tables but has no memslot, so it also
produces MMIO exits — harmless today (no RAM overlaps it; RAM caps at `0xD000_0000`), and
unavoidable at 2 MiB page granularity without splitting to 4 KiB. Worth a one-line comment
noting the hole PTE deliberately maps a full 2 MiB superset of the `0x7000` device window,
so a future reader does not assume the mapped extent equals `MMIO_HOLE_LEN`. (The spec
§2.2 describes a *second small memslot* for the hole; this impl instead relies on "no
memslot ⇒ MMIO exit" with the hole mapped only in the page tables. That divergence is
intentional and well-commented, but a pointer from the code to §2.2 would help.)

### S3 — `e_entry` is not validated to lie within a PT_LOAD

`load_elf` returns `entry = e_entry` without checking it falls inside any loaded segment
(same as the iter-29 loader it replaces — explicitly called out as acceptable in the
brief). A guest with a stale/garbage `e_entry` would fault at first fetch rather than
being rejected at load time. Low value for the trusted nanokernel images, but a cheap
"entry not in any PT_LOAD" check would turn an opaque triple fault into a clear loader
error. Optional.

### S4 — unit tests mirror the implementation's offset arithmetic

`page_tables_map_ram_and_the_mmio_hole` recomputes the hole slot with the *same*
expression the implementation uses
(`PD_BASE_GPA + (MMIO_HOLE_BASE / GIB) * 0x1000` and
`((MMIO_HOLE_BASE % GIB) / PAGE_2M) * 8`). If that formula were wrong, both impl and test
would be wrong together and the test would still pass. Consider asserting against an
independently derived literal — e.g. the hole PTE GPA is `0x6400` and slot index `128` for
`MMIO_HOLE_BASE = 0xD000_0000` — so the test pins the *answer*, not the *formula*. Same
applies to the `last` RAM-page slot assertion. (These constants were confirmed by hand
during this review.)
