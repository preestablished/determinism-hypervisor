# Positive Notes

Things this iteration got right that I specifically tried to break and could not:

### P-1. Linker probe correctly fails-fast on the wrong emulation

`probe()` runs `<ld> -m elf_x86_64 --version` and checks exit status. I verified on
GNU ld 2.42 (this host): `ld -m elf_x86_64 --version` exits 0, while
`ld -m elf_FOOBAR --version` exits **1** with "unrecognised emulation mode." So a
single-target / wrong-arch binutils `ld` (e.g. an aarch64-only ld without
`elf_x86_64` compiled in) is rejected at probe time, not at link time. The probe
reasoning in the comment is sound and the fail-fast is real.

### P-2. Portability fallback chain is genuinely robust

GNU ld → `ld.lld` → `lld` → sysroot `rust-lld -flavor gnu`. The rust-lld leg is
always present in any Rust install and is multi-target, so the pipeline links on an
aarch64 host with stock binutils. nasm cross-assembles x86 everywhere. The comments
correctly note that *execution* is hardware-gated elsewhere; only the *build* is
host-runnable. This is the right separation.

### P-3. nasm include resolution is correct and has a real fallback

`-I include/` (with the trailing slash, noted for old nasm) is the primary path,
necessary because includes live in `include/` while sources live in `asm/`. I
confirmed nasm **also** resolves `%include` relative to the source file, so the
build is robust even if the `-I` were wrong. Good belt-and-suspenders.

### P-4. SysV stack alignment at `prog_main` entry is exactly correct

`stack_top` links to **0x104060**, which is 16-aligned. crt0 does `lea rsp,[stack_top]`
(RSP%16==0) then `call prog_main`, which pushes the 8-byte return address → at
`prog_main` entry **RSP%16==8**, exactly the SysV contract. A future C-compiled
`prog_main` would be correctly aligned with no extra work. The `align 16` before
`stack_bottom` and the 16384 (=16 KiB, %16==0) size are what make this hold.

### P-5. BootInfo magic encoding is consistent across asm and Rust

`"DHBI"` little-endian = 0x49424844 ('D'=0x44 low byte … 'I'=0x49 high byte). The
`.inc` hardcodes `0x49424844`; `src/lib.rs` computes `u32::from_le_bytes(*b"DHBI")`;
both equal 0x4942_4844 (verified). The unit test `magic_spells_dhbi` pins both the
numeric value and the byte order. No endianness trap.

### P-6. Drift test catches the asm↔Rust offset/magic/version divergence

Despite the parser fragility (I-1), the test *as written* correctly maps all six
offsets plus magic and version from the `.inc` to the Rust consts, and the
trailing-space convention does defeat the `CMDLINE` / `CMDLINE_LEN` prefix collision
for the current file. It runs and passes; ABI drift in any mirrored field would fail
it.

### P-7. Linker invocation is correct for a freestanding static EXEC

`-nostdlib --no-dynamic-linker -static -T link.ld`: no libc, no PT_INTERP, no PT_DYNAMIC,
`ET_EXEC` (not PIE). `readelf -h` confirms `Type: EXEC`, `Entry: 0x100000`, one
program header. The `link.ld` `/DISCARD/` of `.note* .comment .eh_frame*` keeps the
image free of toolchain-injected sections (no stray `.note.gnu.property` survived in
the GNU-ld output here). e_entry lands on `_start` because `.text.start` is placed
first and `*(.text*)` follows.

### P-8. CI shell hygiene and lane scoping are thoughtful

The `cargo fmt --check` step correctly avoids `--all` (which would format the sibling
path-dep checkouts) and fails-closed if the member list is empty (`test -n
"$members"`). The arm lane excludes the x86-only crates with a documented reason. The
`kvm-intel` lane gates on rw `/dev/kvm` so a read-only grant can't leave live tests
silently skipped while the lane stays green. These are the kind of details that
prevent false-green CI.
