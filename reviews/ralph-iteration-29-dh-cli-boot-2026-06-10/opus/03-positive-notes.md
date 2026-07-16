# Positive notes

### P-1 — IN-FILL contract honored exactly, at the right layer

The single most determinism-critical detail in this diff is handled correctly. The
comment in `crates/dh-vmm/src/kvm.rs` (~line 225) is explicit that for `DetcallIn` /
`SerialIn`, `classify_exit` does **not** fill the kvm_run IO buffer — the caller must
write the reply *before* re-entering `KVM_RUN`, and `classify_exit` consumes the
`VcpuExit` by value (so post-classify filling is impossible by construction). `boot.rs`
intercepts `VcpuExit::IoIn(_, data)` and `data.fill(0)` **on the raw exit, before
`classify_exit` is ever called**. This is the only correct place to do it, and the
inline comment explains why. Excellent fidelity to a subtle contract.

### P-2 — Hostile-ELF parsing is genuinely robust

Every multi-byte field read goes through `u16le` / `u64le` helpers built on
`slice::get(..).try_into()`, returning `Option` on any out-of-bounds — no panics, no
unchecked indexing. `p_offset + p_filesz` cannot index past the file because the
`elf.get(p_offset..p_offset + p_filesz)` slice returns `None` for a hostile huge
`p_offset` (the range start already exceeds `len`). `p_vaddr` is unvalidated against
guest RAM by design, but `write_slice` errors cleanly on an out-of-range target. The
`phnum * phentsize` loop is bounded by `phnum <= 65535`, each iteration cheap and
bounds-checked. A malformed image fails as a typed `BootError::Elf`, never UB.

### P-3 — Page-table arithmetic is correct and the 1 GiB cap is load-bearing

`2 << 20 == 0x20_0000 == 2 MiB` ✓. `mem_bytes.div_ceil(2 << 20)` is the right page count.
The `mem_bytes > 1 << 30` guard ensures at most `512` PD entries (`1 GiB / 2 MiB = 512`),
which exactly fills the `0x3000..0x4000` page without spilling — the cap and the
single-PD layout are consistent. Exactly 1 GiB is allowed and still fits (512 × 8 = 4096
bytes). The MMIO hole at `0xD000_0000` is deliberately left unmapped, matching the M0
"hello, not device-exercise" scope and the comment.

### P-4 — Low-RAM layout has no overlaps and matches the ABI

Page tables (`0x1000`–`0x4000`), BootInfo (`0x5000`), guest image (`0x10_0000`) do not
collide — all control structures sit below the 1 MiB load address. BootInfo at a fixed
GPA passed in RSI matches ARCH §2.3 ("`RSI = &BootInfo`") and the nanokernel crt0
contract, and the page is written with the exact canonical field order/offsets from
`tests/nanokernel/src/lib.rs` (DHBI / version 1 / mem_size / mmio_base / cmdline_len /
reserved / cmdline). `mmio_base = 0xD000_0000` matches the ARCH §2.2 PIO/MMIO map.

### P-5 — Explicit bss zero-fill despite already-zeroed RAM

`load_elf` zero-fills `[filesz, memsz)` explicitly even though fresh memfd-backed guest
RAM is already zero. The comment correctly frames this as defending the nanokernel
loader contract (crt0's stack + `BOOT_INFO_PTR` live in `.bss`) against a future memslot
reuse that could regress the zeroing assumption. Good defensive instinct in a
determinism-sensitive codebase.

### P-6 — Clean lib/bin split enabling in-process live tests

Splitting the boot path into `lib.rs` (`pub mod boot`) lets `tests/boot_hello.rs` drive
`dh_cli::boot::boot` directly in-process rather than shelling out to the binary —
faster, and it asserts on structured `BootOutcome` rather than parsing stdout. The
kvm-usable gate (`/dev/kvm` open probe, skip on NotFound/PermissionDenied, panic on
unexpected errno) is the right pattern for hardware-dependent tests and matches the
repo's lab-box/CI-lane gating convention.

### P-7 — `forbid(unsafe_code)` holds across the new surface

`#![forbid(unsafe_code)]` is present at crate level in both `lib.rs` and `main.rs`, so
`boot.rs` (a module of the lib crate) inherits it — no `unsafe` anywhere in the new
loader, which is appropriate since all the genuinely unsafe KVM ioctls are encapsulated
behind `dh-vmm`'s wrapped API.

### P-8 — Exit handling fails loud, never silent-corrupts

`run_until_hlt` collects serial OUTs, ignores only the genuinely-ignorable
`DetcallOut` / `PioIgnored` arms (deterministic no-ops per the PIO map), and turns MMIO
and every other unmodeled exit into a typed `UnexpectedExit` error with the offending
GPA. The exit budget prevents an infinite-loop guest from hanging the harness. This is
the correct posture for a determinism platform: anything the M0 loop doesn't model is an
error, not a silent absorb.
