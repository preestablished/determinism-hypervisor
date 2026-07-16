# Positive notes

### P-1 — BootInfo layout is byte-for-byte correct against the canonical contract
`write_bootinfo` builds `DHBI` + version(u32) + mem_size(u64) + `0xD000_0000`(u64) +
cmdline_len(u32) + **reserved(u32)** + cmdline. I walked the offsets: magic@0x00, version@0x04,
mem_size@0x08, mmio_base@0x10, cmdline_len@0x18, reserved@0x1C, cmdline@0x20 — an exact match to
`include/bootinfo.inc` and `nanokernel/src/lib.rs`. The reserved `0u32` is present (a common place
to drift by 4 bytes), and `mmio_base` is written as `0xD000_0000`, consistent with
`MMIO_HOLE_BASE`. `pipeline_smoke` reading `K` off serial confirms the guest accepted the page live.

### P-2 — The ELF loader handles the hostile / degenerate segment cases safely
- Pure-bss segment (`p_filesz == 0`): `elf.get(p_offset..p_offset)` is a valid empty slice — no
  panic, no spurious "beyond file end".
- `p_memsz < p_filesz` (hostile): `p_memsz.saturating_sub(p_filesz)` yields `0`, so no underflow and
  no bogus huge zero-fill.
- Bss tail zero-fill `[filesz, memsz)` is done explicitly even though fresh memfd RAM is already
  zero — a deliberate belt-and-suspenders against future memslot reuse, exactly as the comment says.
- Header validation rejects non-ELF64-LE and non-`x86_64 ET_EXEC` before touching offsets; all
  multi-byte reads go through bounds-checked `get(..)` helpers (`u16le`/`u64le`).

### P-3 — Page-table construction arithmetic is correct
`2 << 20` is exactly 2 MiB and binds correctly in `i * (2 << 20)`; `mem_bytes.div_ceil(2 << 20)`
gives the right page count (8 for 16 MiB); PD entries carry `present|writable|PS` (`0x83`); PML4/PDPT
carry `present|writable` (`0x3`). The `1 << 30` cap in `boot()` uses correct precedence (shift
before compare). CR3 points at PML4 (`0x1000`), CR4 has PAE, EFER has LME|LMA — a coherent direct
long-mode entry.

### P-4 — Live M0 acceptance is real, not mocked
I reproduced the full path on this box: `cargo test -p dh-cli` passes both live legs;
`dh-cli boot hello.elf` prints `HELLO\n` (7 exits) and the `--json` form escapes correctly
(`{"serial":"HELLO\n","exits":7}`); `pipeline_smoke` reports `K` (2 exits). The tests correctly
**skip** rather than fail when `/dev/kvm` is unusable (`kvm_usable()` guard), so CI lanes without
KVM stay green while the kvm-intel lane and lab box actually exercise it.

### P-5 — Determinism holds where it matters
`landing_loop` produced byte-identical `{serial:"L", exits:2}` across repeated runs at
`--cmdline 0`, `100`, and `1000000`, and reached `L` for every cmdline I tried. Same input →
identical observable output, which is the property the whole hypervisor is built to guarantee.

### P-6 — The `IoIn` IN-FILL handling respects the kvm_run buffer ordering
`data.fill(0)` on `VcpuExit::IoIn(_, data)` writes through the `&mut [u8]` that aliases the kvm_run
IO data area, so the deterministic reply is in place *before* re-entry — the ordering the
`classify_exit` IN-FILL contract demands. (See I-2 for the caveat that it does this for *all*
ports.)

### P-7 — Forbidden-list posture inherited cleanly
The boot path builds on `create_slot_vm`, which never creates an in-kernel irqchip/PIT/kvmclock,
so `HLT` exits to userspace and is mapped to `ExitEvent::Hlt` → clean run termination (verified:
hello/pipeline_smoke/landing_loop all end on HLT). `Shutdown` (triple fault) is surfaced as a hard
`UnexpectedExit` error rather than silently looping — the right default for a determinism harness.
Clippy is clean and `lib.rs` carries `#![forbid(unsafe_code)]` (all unsafe stays in dh-vmm).
