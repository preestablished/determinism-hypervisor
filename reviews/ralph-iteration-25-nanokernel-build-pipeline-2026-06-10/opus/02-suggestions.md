# Suggestions

### S-1. `build.rs` does not declare `rerun-if-env-changed` for `RUSTC`/`HOST`

**File:** `tests/nanokernel/build.rs`, lines 24–26, 89–96

The linker selection branches on `RUSTC` and `HOST` (for the `rust-lld` fallback), but
the script only emits `rerun-if-changed` for `asm/`, `include/`, and `link.ld`. Cargo sets
both env vars and they rarely change mid-session, so this is low-risk. Still, when a host
*does* switch toolchains/targets (e.g. a multi-arch CI cache), the build script won't
rerun and could keep using a stale linker path. Add:

```rust
println!("cargo:rerun-if-env-changed=RUSTC");
println!("cargo:rerun-if-env-changed=HOST");
```

Note on the brief's question: cargo **does** set `RUSTC` and `HOST` for build scripts
(confirmed — the `rust-lld` fallback relies on it and built cleanly), and `build.rs`
itself is implicitly tracked for changes by cargo, so no explicit `rerun-if-changed` for
the script is needed. Only the two env vars are missing.

### S-2. `bootinfo.inc` `%define` lookup matches on a trailing space — fragile

**File:** `tests/nanokernel/tests/elf_shape.rs`, lines 63–73 and call sites (e.g. `lookup("BOOTINFO_MAGIC ")`)

The drift test finds the `%define` line via `l.contains(name)` where every `name` carries a
trailing space (`"BOOTINFO_MAGIC "`) specifically to avoid `BOOTINFO_MAGIC` matching
`BOOTINFO_OFF_MAGIC`. This works, but it is a subtle prefix-collision guard that a future
edit (reformatting the `.inc` to tabs, or aligning columns) could silently break by
removing the single space after the name. Prefer matching the token exactly:

```rust
let line = inc.lines().find(|l| {
    let mut t = l.split_whitespace();
    t.next() == Some("%define") && t.next() == Some(name)  // name without trailing space
});
```

This is robust to whitespace and removes the need for the trailing-space convention at all
seven call sites.

### S-3. Rust mirror omits `reserved` and (arguably) `cmdline_len` offset constant naming

**File:** `tests/nanokernel/src/lib.rs`, lines 12–17 vs `include/bootinfo.inc`, lines 9–11

The `.inc` documents `0x1C u32 reserved`. The Rust side has `BOOTINFO_OFF_CMDLINE_LEN`
(0x18) and `BOOTINFO_OFF_CMDLINE` (0x20) but no constant for the reserved word at 0x1C.
That is fine (reserved is reserved), but the drift test then can't catch a future
relocation of `reserved`. Optional: add `BOOTINFO_OFF_RESERVED = 0x1C` on both sides so
the whole header layout is pinned, not just the consumed fields. Also consider pinning the
total fixed-header size (`BOOTINFO_HEADER_LEN = 0x20`) so the cmdline offset and any future
field can't overlap.

### S-4. No fixed BootInfo GPA pinned in this crate — acceptable split, but cross-reference it

**File:** `tests/nanokernel/include/bootinfo.inc` (header comment), `src/lib.rs`

The struct layout lives here; the fixed GPA where dh-vmm places the struct lives in the
loader bead. Splitting layout (guest-owned) from placement (loader-owned) is the right
factoring. Suggestion: add a one-line cross-reference in the `.inc` header comment naming
the loader bead/file that owns the GPA, so the two halves of the contract are navigable
from either side. Right now a reader of `bootinfo.inc` has no pointer to where the address
is decided.

### S-5. Stack-alignment contract for `prog_main` is correct but undocumented

**File:** `tests/nanokernel/asm/crt0.asm`, lines 18–22

Verified: `stack_top` resolves to `0x104060`, which is 16-byte aligned, so RSP is `% 16 == 0`
at the `call prog_main` site and therefore `% 16 == 8` on entry to `prog_main` — exactly the
SysV state a compiled `prog_main` expects from a `CALL`. The current `pipeline_smoke` is pure
asm and doesn't care, but the real guest beads may write `prog_main` in a higher-level form
that assumes SysV entry alignment. Add a one-line comment in crt0 stating the guarantee
("RSP is 16-aligned at the call site; prog_main sees the standard post-CALL `RSP%16==8`"),
so the contract survives if `stack_bottom`'s size or the `.bss` layout ever changes the
final alignment of `stack_top`. (The `align 16` before `stack_bottom` is what currently
makes this hold — worth noting it is load-bearing.)

### S-6. ELF-shape test could assert the bss zero-fill contract it documents

**File:** `tests/nanokernel/tests/elf_shape.rs`, lines 28–46

The test verifies a PT_LOAD covers the entry, but the brief specifically asks whether the
test should assert `p_memsz > p_filesz` (i.e. that there is bss to be zero-filled, which the
loader must honor per ARCH §2.3). The produced ELF has `FileSiz=0x47`, `MemSiz=0x4060`, so
the condition holds. Adding `assert!(memsz > filesz)` for the entry-covering segment turns
the loader's zero-fill obligation into an executable invariant on the guest side — cheap
insurance that a future link-script change doesn't accidentally materialize `.bss` in the
file image (which would silently mask a loader zero-fill bug). Read `p_filesz` at `at + 32`
alongside the existing `p_memsz` at `at + 40`.
