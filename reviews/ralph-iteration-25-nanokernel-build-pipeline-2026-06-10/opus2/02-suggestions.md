# Suggestions (non-blocking)

### S-1. `kvm-intel` lane has no nasm guard

**File:** `.github/workflows/ci.yaml:80-104`

The host lane guards nasm (`which nasm || { sudo apt-get … install -y nasm; }`,
line 58). The `kvm-intel` self-hosted lane runs `cargo build --workspace` /
`cargo test --workspace` (lines 101-104), which now pulls in `tests/nanokernel`
whose `build.rs` `panic!`s if nasm is absent. The lane relies on the box having
nasm pre-installed. If that box is ever re-imaged or nasm is removed, the **whole
workspace build** fails on this lane with a build-script panic, not a clear "install
nasm" message at the top.

**Suggestion:** Add the same guard line to the `kvm-intel` steps (before
`cargo build`), or add a one-line `command -v nasm || { echo "::error::nasm missing
on kvm runner"; exit 1; }` precheck so the failure is self-explanatory. Low effort,
removes a future mystery red.

---

### S-2. PT_LOAD is RWE — no W^X separation at the segment level

**File:** `tests/nanokernel/link.ld`, observed in `readelf -l`

Section flags are clean (`.text` = `AX`, `.bss` = `WA`), but the linker merges them
into **one PT_LOAD with `RWE`** flags (because `.text` and `.bss` share a segment
under `-static -nostdlib`). A loader that maps guest pages per program-header flags
and enforces W^X would map `.text` writable. For a freestanding determinism test
guest with no NX intent this is harmless, but it's worth a note so the loader bead
doesn't assume per-segment W^X is meaningful for nanokernel images.

**Suggestion:** Either document "nanokernel images use a single RWE load segment;
W^X is not modeled," or split `.text` (RX) from `.data`/`.bss` (RW) into separate
load segments via a `PHDRS`/`SEGMENT_START` arrangement if W^X ever matters here.

---

### S-3. 64 KiB size budget is loose vs the real ~5 KiB / 71-byte image

**File:** `tests/nanokernel/tests/elf_shape.rs:50-54`

The actual ELF is 5144 bytes on disk, with **`p_filesz = 0x47` (71 bytes)** of real
program content (the 16 KiB is `.bss` memsz, not file bytes). The 64 KiB ceiling is
~12× the current file and ~900× the real code. ARCH §1 says "~2 KiB." The budget
catches only catastrophic bloat.

**Suggestion:** Tighten to something like `< 16 * 1024` (or assert on `p_filesz`
rather than total file size, since the section-header/strtab overhead dominates the
5 KiB). A tighter bound on `filesz` would actually catch "someone accidentally
linked in a fat object" while leaving headroom for a few real guests.

---

### S-4. `reserved` field (0x1C) and the cmdline are documented but not mirror-tested

**Files:** `include/bootinfo.inc:10`, `src/lib.rs`

The `.inc` comment documents `0x1C u32 reserved = 0` and `0x20 cmdline`, but there is
no `BOOTINFO_OFF_RESERVED` `%define`/const and no drift assertion on the reserved
offset. `BOOTINFO_OFF_CMDLINE` is mirrored (0x20), so the cmdline start is covered,
but the reserved word — the one most likely to be silently repurposed later — is
not. Minor, but adding `BOOTINFO_OFF_RESERVED = 0x1C` on both sides closes the last
hole in the offset table and documents intent.

---

### S-5. `elf_shape.rs` indexes the ELF without bounds-checking

**File:** `tests/nanokernel/tests/elf_shape.rs:7-46`

`u16le`/`u64le` and the direct slices (`elf[0..4]`, header reads at fixed offsets,
`phoff + i*phentsize`) will **panic with index-out-of-bounds** on a truncated or
malformed image rather than a clean assertion. Because this is a test, a panic still
means "fail," so it's cosmetic — but a one-line `assert!(elf.len() >= 64)` before the
header reads (and a guard that `phoff + phnum*phentsize <= elf.len()`) would turn an
opaque panic into a readable failure if the build ever emits something degenerate.

---

### S-6. README.md is stale — says the pipeline is "implemented by later beads"

**File:** `tests/nanokernel/README.md`

> "The build pipeline and concrete guests are implemented by later beads."

This iteration **is** the build pipeline. The README now under-describes its own
directory. Update it to: the pipeline exists (`build.rs` + nasm + portable link
chain), `pipeline_smoke` is the first built+shape-tested guest, and the substantive
guests (hello stub, landing loop, device exercise) are the later beads. Keeps the
directory self-documenting for the next agent.
