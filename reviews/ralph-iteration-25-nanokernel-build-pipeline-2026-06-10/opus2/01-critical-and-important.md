# Critical & Important Findings

## Critical

None. The build pipeline assembles, links, embeds, and shape-tests correctly; I
reproduced the full chain on x86_64 and every test passes.

---

## Important

### I-1. Drift-test parser is correct today but is a silent-pass footgun

**File:** `tests/nanokernel/tests/elf_shape.rs:60-92`

The `lookup` closure does:

```rust
let line = inc.lines()
    .find(|l| l.starts_with("%define") && l.contains(name))
    .unwrap_or_else(...);
let val = line.split_whitespace().nth(2).unwrap();
```

`.find` returns the **first** line containing the substring. Every call site passes
a name with a **trailing space** (`lookup("BOOTINFO_OFF_CMDLINE ")`), and that
trailing space is the *only* thing preventing a prefix collision:

- `BOOTINFO_OFF_CMDLINE_LEN` (offset 0x18) is defined **before** `BOOTINFO_OFF_CMDLINE`
  (offset 0x20) in the `.inc` file.
- `lookup("BOOTINFO_OFF_CMDLINE ")` (trailing space) does **not** match the `_LEN`
  line, because the character after `BOOTINFO_OFF_CMDLINE` there is `_`, not a space.
  I verified this against the actual file bytes (all spaces, no tabs — confirmed with
  `cat -A`): the first matching line is the 0x20 line. **Correct today.**

The fragility: this is a hidden invariant of the *caller*, not the parser. If a
future maintainer adds `lookup("BOOTINFO_OFF_CMDLINE")` (no trailing space) — the
natural thing to write — `.find` returns the **`_LEN` line first** and the test
silently asserts `0x18 == 0x20`'s Rust const, OR worse, passes against a wrong const
without anyone noticing the parser matched the wrong symbol. A test whose
correctness depends on every caller remembering an undocumented trailing space is a
test that will eventually lie.

**Fix (pick one):**
- Parse the symbol name as a whole token, not a substring:
  ```rust
  .find(|l| l.starts_with("%define")
            && l.split_whitespace().nth(1) == Some(name))
  ```
  Then call `lookup("BOOTINFO_OFF_CMDLINE")` with no trailing space — exact-match,
  collision-proof, self-documenting.
- Or, at minimum, add a code comment at the closure explaining the trailing-space
  requirement is load-bearing and why.

**Severity rationale:** not a bug *now*, but it's a test-integrity defect: the whole
point of this test is to catch ABI drift, and the current form can be made to
silently pass on the wrong symbol by an innocent edit.

---

### I-2. BootInfo struct layout is cited as "ARCH §2.3" but §2.3 does not define it

**Files:** `tests/nanokernel/include/bootinfo.inc:1-2`, `src/lib.rs:1-9`

The `.inc` header comment says: *"BootInfo ABI (ARCH §2.3): the versioned struct
dh-vmm places at a fixed GPA …"* and `src/lib.rs` says *"the BootInfo ABI (ARCH
§2.3)."* I read §2.3. It says only:

> `RSI = &BootInfo` (a versioned struct at a fixed GPA carrying mem_size, MMIO base,
> cmdline bytes).

§2.3 does **not** specify: the magic value, the existence of a magic field at all,
the version field's offset, the `reserved` word, `mmio_base` (it says "MMIO base"
but not the 0x10 offset or the 0xD000_0000 value belonging here), or any of the
`BOOTINFO_OFF_*` offsets. In other words, **this iteration is the de-facto normative
source** for the BootInfo binary layout, but it points the reader at a §2.3 that
won't corroborate it.

Concretely, the loader bead (the dh-vmm side that *writes* this struct at boot) will
read §2.3 to learn the layout, find no magic/version/offsets, and either invent a
second incompatible layout or be forced to reverse-engineer it from these test
files. That is exactly the drift this whole module is trying to prevent — except at
the producer/consumer boundary rather than the asm/Rust boundary.

**Fix:** Make the source-of-truth explicit and bidirectional. Either:
- Add the full BootInfo layout table (magic/version/offsets/reserved) to
  ARCHITECTURE.md §2.3 as normative, and have `.inc`/`lib.rs` say "mirrors ARCH §2.3
  Table N," **or**
- Change the comments to state plainly that `include/bootinfo.inc` is the normative
  layout for now and §2.3 is the higher-level boot-protocol reference — and file a
  bead to fold the layout into §2.3 before the loader is written.

The values themselves are internally consistent and match §2.2 where they overlap
(`mmio_base = 0xD000_0000` matches the §2.2 MMIO map; serial 0x3F8 matches §6.9), so
this is purely a "where does the contract live" gap — but it's the gap most likely
to bite the very next bead.

---

### I-3. PT_LOAD memsz > filesz: the bss zero-fill loader contract is undocumented

**Files:** `tests/nanokernel/asm/crt0.asm:27-33`, `link.ld:19`, ARCH §2.3

`BOOT_INFO_PTR` (8 B) and the 16 KiB stack live in `.bss`. The emitted ELF has a
single PT_LOAD with **`p_filesz = 0x47` but `p_memsz = 0x4060`** (verified via
`readelf -l`): 16409 bytes of `.bss` tail exist only as `memsz`, with no file
bytes. The §2.3 loader ("`dh-vmm` loads the ELF PT_LOAD segments into guest RAM")
**must zero-fill `[filesz, memsz)`** or the guest's stack region contains whatever
was in guest RAM previously — nondeterministic, and a correctness hazard.

For *this* smoke program the result is benign (it writes `BOOT_INFO_PTR` before
reading it, and uses the stack only for the `call`/`ret` return address, which it
writes before reading), so the smoke test would still print 'K' even without
zero-fill. But the contract is real for every future guest that relies on the
crt0 comment's promise of "a zeroed `.bss`" (`crt0.asm:8`), and it is written
**nowhere a loader author will see it** — §2.3 just says "loads the PT_LOAD
segments," which a naive implementer reads as "memcpy filesz bytes."

**Fix:** Add one sentence to §2.3 (or wherever the loader bead's contract lands):
"PT_LOAD segments with `p_memsz > p_filesz` must have `[p_filesz, p_memsz)`
zero-filled in guest RAM (`.bss`)." Optionally add an `elf_shape.rs` assertion that
the smoke ELF actually exercises this (`memsz > filesz`) so the loader bead's
zero-fill code has a fixture that would catch a regression.

**Severity rationale:** Important, not Critical — it doesn't break anything in this
iteration, but it is a load-bearing producer/consumer contract that the next bead
depends on and that the current docs do not state.
