# Suggestions

## SUGGESTION 1 — `nanokernel` can dev-dep `dh-devices`, so the `PAD_BASE` pin can be the real `PV_PAD_BASE` import instead of a hardcoded literal + comment

`tests/nanokernel/tests/elf_shape.rs:250`, `tests/nanokernel/Cargo.toml`

The task asked whether the nanokernel crate *could* depend on `dh-devices` for
a stronger pin. **It can, as a dev-dependency, with no cycle:**

- `dh-devices`'s deps are `detguest-host`, `detguest-wire`, `dh-inputlog`,
  `rand_*` — none of which is `nanokernel` or `dh-vmm`.
- `nanokernel` already dev-deps `detguest-host` + `detguest-wire`, the same
  crates `dh-devices` pulls. Adding `dh-devices` to `[dev-dependencies]` keeps
  the dependency DAG acyclic (nothing `dh-devices` touches depends back on
  `nanokernel`).

So the pin could become:

```rust
assert_eq!(define("PAD_BASE"), dh_devices::pad::PV_PAD_BASE);
assert_eq!(define("REG_PAD0"), dh_devices::pad::REG_PAD0);
assert_eq!(define("REG_FRAME"), dh_devices::pad::REG_FRAME_COUNTER);
```

That is a strictly stronger pin than `0xD000_1000 // dh_devices pad::PV_PAD_BASE`
— it fails the build if the device constant moves, whereas the comment is just a
promise. **Trade-off to weigh:** it adds a (dev-only) cross-crate build edge
from a `tests/` crate into a `crates/` crate, which some teams keep one-way.
This is a judgment call, not a defect — the current literal+comment form is
acceptable. But since the question was explicitly posed: yes, the import is
available and is the stronger pin.

---

## SUGGESTION 2 — Drop the unused `extern BOOT_INFO_PTR`; it is dead here and inconsistent with every other guest

`tests/nanokernel/asm/pad_echo.asm:28`

```asm
extern BOOT_INFO_PTR
```

I checked all guests that declare this extern:

| guest            | `BOOT_INFO_PTR` occurrences |
|------------------|-----------------------------|
| pipeline_smoke   | 2 (decl + use)              |
| landing_loop     | 2 (decl + use)              |
| device_exercise  | 2 (decl + use)              |
| timer_guest      | 2 (decl + use)              |
| **pad_echo**     | **1 (decl only — unused)**  |

`pad_echo` never references `BOOT_INFO_PTR` — it doesn't read BootInfo (it
hardcodes `PAD_BASE`/`TABLE_GPA`). `hello.asm`, which also ignores BootInfo,
correctly omits the extern entirely. The dangling `extern` is harmless (nasm +
ld resolve it from crt0 and emit nothing), but it is misleading — it implies the
guest consults BootInfo — and it breaks the codebase's "declare iff used"
convention. Delete the line.

---

## SUGGESTION 3 — The header comment attributes the zeroed table to "boot" but it actually relies on the fresh anonymous mmap, not the loader

`tests/nanokernel/asm/pad_echo.asm:14-15`

```asm
; The frame loop is the only writer; RAM is zeroed at boot so count
; starts at 0.
```

I traced the zeroing. The loader's explicit `[filesz, memsz)` zero-fill
(`boot.rs:132-136`) covers **PT_LOAD bss only** — i.e. `work_buf` in `.bss`,
which is correctly zeroed. The **table at `0x30_0000` is not in any PT_LOAD**,
so the loader never touches it; its zero comes from the guest RAM being a fresh
anonymous mmap (`GuestMemoryMmap::from_ranges`, zero-filled by the kernel). On a
**restored** snapshot the table comes from snapshot RAM, also fine.

The behavior is correct on every path, but the comment's "zeroed at boot"
phrasing credits the wrong mechanism. Suggest: "the table's count header reads 0
at a fresh boot because guest RAM is zero-initialized (anonymous mmap); the
loader's bss zero-fill covers `work_buf` but not the table region." This matters
because a future reader could move the table into a PT_LOAD-adjacent address
assuming the loader zeroes it, and be wrong.

---

## SUGGESTION 4 — `work_buf` is a 4 KiB scratch buffer whose only purpose is to give the pace loop a store target; a one-line note on *why it must exist* would help

`tests/nanokernel/asm/pad_echo.asm:53,60,67-69`

The pace loop writes `[r12 + rbx*8]` purely so the busy loop has a memory store
(making each iteration 6 retired instructions, the unit the icount budget is
built on). `rbx` is masked `and ebx, 511` so it stays inside the 512-qword
buffer — good, no overrun. But the *intent* (a fixed-cost store that keeps the
iteration at exactly 6 instructions and prevents the optimizer-free hand asm
from being a pure-register spin the icount model already covers) is not stated.
A one-liner — "store target exists only to fix the per-iteration instruction
count; value is discarded" — would stop a future editor from "optimizing it
away" and silently changing the per-frame icount, which would desync every
drift-pinned frame boundary. (This is convention-only; the `and ebx, 511` mask
correctly bounds it to the 512-qword `work_buf` regardless.)
