# Positive Notes

### P-1. Linker probe reasoning is genuinely correct — and verified

The portability chain (GNU `ld -m elf_x86_64` → `ld.lld` → `lld` → sysroot `rust-lld
-flavor gnu`) is well-conceived, and the load-bearing assumption holds: GNU `ld` validates
`-m` before honoring `--version`, so a single-target `ld` returns a non-zero exit to the
probe and is correctly skipped. I confirmed this against binutils on this host. The
`rust-lld` fallback (always present in the Rust sysroot) is exactly the right way to make
the pipeline work on an aarch64 dev host with stock binutils. This is the hardest part of
the change and it is right.

### P-2. ELF output verified end-to-end against the boot protocol

A clean build produced exactly what ARCH §2.3 describes: `ET_EXEC`, `EM_X86_64`,
statically linked, `e_entry == 0x100000` with `_start` as the first symbol there, and a
single `PT_LOAD` with `MemSiz (0x4060) > FileSiz (0x47)` — i.e. the file image carries no
`.bss`, leaving zero-fill to the loader as intended. The `*(.text.start)` ordering in
`link.ld` does guarantee crt0 lands first.

### P-3. Single-source ABI with an executable drift guard

Defining the BootInfo ABI once in `include/bootinfo.inc` and mirroring it in `src/lib.rs`,
then having an integration test *parse the `.inc`* and assert equality, is a strong pattern.
It makes the asm↔Rust contract impossible to drift silently — exactly the right defense for
a struct that crosses the loader/guest boundary. The magic byte order is consistent:
`u32::from_le_bytes(*b"DHBI") == 0x49424844`, matching the `.inc` constant (verified).

### P-4. include_bytes! propagation works correctly

I verified the full chain: editing `pipeline_smoke.asm` reruns the build script, rebuilds
the ELF, and the new bytes flow through `include_bytes!(concat!(env!("OUT_DIR"), ...))`
into the lib (the embedded ELF's hash changed after a one-byte asm edit). Directory-level
`rerun-if-changed` on `asm/` and `include/` also correctly triggers rebuilds on content
changes. The build wiring is sound.

### P-5. No-deps policy honored cleanly

The crate carries zero dependencies — the hand-rolled `which`, the manual ELF64 header
parsing in the shape test, and the `%define` parser in the drift test all avoid pulling in
`which`/`object`/`goblin`. For a test-guest build crate inside a hypervisor workspace,
keeping the dependency surface at zero is the right call and is executed consistently.

### P-6. Excellent inline documentation and ARCH cross-references

Every file opens with a comment tying it to the relevant ARCH section (§1 tree, §2.3 boot
protocol, §6.9 serial, §2.2 MMIO). The build.rs header explains the portability strategy,
crt0 documents the HLT-park terminal-stop contract, and the shape test explains why each
offset is read. This makes the change reviewable and the next bead (real guests) much
easier to land.

### P-7. CI shell grouping is correct

The `which nasm || { sudo apt-get update && sudo apt-get install -y nasm; }` construct is
syntactically correct: the `{ ...; }` group with the trailing semicolon and the `||`
short-circuit behave as intended (install only on miss). The durability caveats (P-3 in
the Important file) are about environment assumptions, not shell correctness.
