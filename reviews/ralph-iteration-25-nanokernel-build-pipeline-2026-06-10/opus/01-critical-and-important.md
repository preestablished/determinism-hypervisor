# Critical and Important Findings

## Critical

None. The build is correct and reproducible, the produced ELF matches the boot
protocol (ARCH §2.3), and the headline portability concern does not materialize
(see I-1 for the analysis that downgrades it).

---

## Important

### I-1. Probe reasoning is correct, but lean on it explicitly — and prefer a real-link probe long-term

**File:** `tests/nanokernel/build.rs`, `probe()` (lines 116–123) and `find_linker()` (lines 67–111)

The review brief asks whether `probe()` can false-positive on an aarch64 single-target
`ld` that lacks `elf_x86_64`. I tested this directly on binutils:

```
ld -m elf_BOGUS  --version  -> exit 1   (unrecognised emulation mode)
ld -m elf_x86_64 --version  -> exit 0
ld               --version  -> exit 0
```

GNU `ld` parses and validates `-m` *before* honoring `--version`, so an `ld` without
the `elf_x86_64` emulation compiled in returns a non-zero exit to the probe. The
`status.success()` check is therefore **sound**: such an `ld` correctly falls through
to `ld.lld` → `lld` → `rust-lld`. No correction needed for correctness.

**Why this is still Important, not just a positive note:** the *only* thing standing
between this and a confusing aarch64 failure is an undocumented binutils behavior
(`-m` validated ahead of `--version`). `lld`/`rust-lld` are multi-target and will
*always* pass the `--version` probe regardless of `-m`, so the probe contributes no
signal for them — it is load-bearing only for the GNU `ld` branch. Two hardening asks:

1. Add a one-line comment in `probe()` recording the verified fact ("GNU ld validates
   `-m` before `--version`; a single-target ld without elf_x86_64 exits non-zero here"),
   so a future reader does not "simplify" the probe away.
2. Consider a stronger probe that actually links an empty object to `/dev/null` with the
   script, rather than `--version`. `--version` does not exercise the `-T link.ld` /
   `-static` path; a probe that mirrors the real link command catches more skew (e.g. an
   `lld` too old for a flag) at probe time instead of at first link.

### I-2. `which()` matches any file on PATH, not an executable file

**File:** `tests/nanokernel/build.rs`, `which()` (lines 149–154)

```rust
.find(|p| p.is_file())
```

`is_file()` is true for a non-executable regular file named `nasm`/`ld`/`lld` anywhere
on `PATH`. On a host with, say, a stray `~/bin/lld` text file, the probe binary resolves
to a non-executable, `Command::spawn` fails, and the result is a spawn panic rather than
a clean fall-through. Filter on the executable bit:

```rust
use std::os::unix::fs::PermissionsExt;
.find(|p| p.metadata().map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false))
```

This also avoids picking a directory entry that happens to share the tool name. (The
crate has no deps by policy, so a hand-rolled `which` is the right call — just make it
match what `Command` will actually execute.)

### I-3. CI nasm step: `sudo`/idempotency/durability assumptions are unstated and brittle

**File:** `.github/workflows/ci.yaml`, line 58

```yaml
- run: which nasm || { sudo apt-get update && sudo apt-get install -y nasm; }
```

Three durability concerns:

1. **`sudo` availability on the arm runner.** The shell grouping (`{ ...; }`) and the
   `||` short-circuit are correct, but the whole branch assumes passwordless `sudo` and
   an apt-based image. If the arm runner image lacks `sudo` (or is non-Debian), the
   `which nasm` miss leads straight to a `sudo: command not found` failure. GitHub-hosted
   `ubuntu-*` runners have passwordless sudo, so this is fine *if* both lanes are
   GitHub-hosted Ubuntu — but that invariant is implicit. Add a comment asserting it, or
   gate on `command -v apt-get`.
2. **`apt-get update` runs unconditionally inside the miss branch.** Harmless but slow;
   if a mirror hiccups, a transient `apt-get update` failure now fails the lane even
   though nasm might already have been installable. Acceptable for now; worth a retry or
   `|| true` on the update only.
3. **kvm-intel lane (line 80, `self-hosted`) has no nasm step.** The brief notes "box has
   nasm." That is true today but undocumented and undurable — a reprovisioned box silently
   breaks the nanokernel build on the gated lane only. Either add the same `which nasm ||`
   guard to the kvm-intel lane, or add a comment on that lane pointing at the box's
   provisioning manifest so the dependency is discoverable.

### I-4. `link.ld` has no orphan-section landing zone; relies on linker default placement

**File:** `tests/nanokernel/link.ld` (lines 8–22)

The script places `.text`/`.rodata`/`.data`/`.bss` and discards `.note`/`.comment`/`.eh_frame`,
but defines no catch for sections a future program might emit: `.data.rel.ro`, `.got`,
`.got.plt`, `.tbss`, `.init_array`, etc. For the current freestanding asm programs none of
these are generated (verified: the produced ELF has only `.text`/`.bss` in its single
PT_LOAD), so this is not a present bug. But:

- **GNU ld and lld place orphan sections differently.** lld tends to append orphans after
  the last matching output section; GNU ld inserts by heuristic. A program that emits, say,
  `.got` could land it in a way that pushes content past the PT_LOAD the `e_entry`-coverage
  test asserts, or splits it into a second segment with surprising flags. Because the two
  linkers in the probe chain disagree here, the pipeline's "same ELF everywhere" promise is
  only true as long as zero orphans appear.
- The single PT_LOAD is currently **RWE** (`readelf` shows `RWE` flags) because `.text` and
  `.bss` share one segment with no `PHDRS`/segment split. For a throwaway test guest entered
  in long mode this is acceptable (no NX policy in the guest), but it is worth a one-line
  comment so it is a deliberate choice, not an accident — and so the real device-exercise
  guests don't inherit a W^X-violating layout silently.

Recommended (cheap) hardening: add an explicit orphan sink and/or an assertion, e.g.
`.got : { *(.got*) }` near `.data`, and an `ASSERT(SIZEOF(.got) == 0, ...)` or a comment
that orphans are intentionally unhandled until a program needs them. At minimum, document
that the e_entry-coverage test is the backstop that will fail loudly if a future program's
layout drifts.
