# Positive Notes

### P-1. The runtime GDT/IDT construction is byte-perfect (verified by disassembly)

I disassembled the built ELF and checked every gate write against the AMD64 64-bit
interrupt-gate layout:

```
100040: lea rax, isr_40 (0x100123)
100048: lea rbx, [rdi+0x400]        ; vector 0x40 * 16 = 0x400  ✓
10004f: mov [rbx],     ax           ; offset[0:15]              ✓ (+0)
100052: mov [rbx+0x2], 0x8          ; CS selector               ✓ (+2)
100058: mov [rbx+0x4], 0x8e00       ; IST=0, P=1 DPL=0 type=0xE ✓ (+4)
10005e: shr rax, 0x10
100062: mov [rbx+0x6], ax           ; offset[16:31]             ✓ (+6)
100066: shr rax, 0x10
10006a: mov [rbx+0x8], eax          ; offset[32:63]             ✓ (+8)
```

The reserved dword at +12 is never written and `idt` lives in `.bss` (sym at 0x106000), so it
is zero — correct. The offset arithmetic (`ax → +0`, `shr 16 → +6`, `shr 16 → eax → +8`)
matches the spec exactly, and the intermediate immediate writes never clobber `rax`.

### P-2. The descriptor tables themselves decode correctly

`objdump -s -j .data`: `gdtr` limit = `0x17` = 3·8−1 ✓; GDT entries decode (little-endian) to
`0x00209A0000000000` (64-bit code, L=1) and `0x0000920000000000` (data) ✓ — these match the
selectors the loader cached (0x08 / 0x10), so the CS reload on interrupt delivery reads a valid
descriptor instead of triple-faulting. `idtr` limit = `0x041F` = 0x42·16−1 ✓ (covers vectors
0…0x41). Both bases are patched at runtime to the `.bss`/`.data` symbol addresses via the `+2`
offset, confirmed in the disasm (`mov [0x100152], rax` / `mov [0x10017a], rax`).

### P-3. The "GDT first" comment captures a real and non-obvious failure mode

The header comment at `prog_main` explaining that the loader only fills segment *caches* and
that interrupt delivery *reloads CS from the in-memory GDT* (so a missing GDT triple-faults on
the descriptor fetch) is exactly the kind of hard-won x86 detail that saves the next person
hours. It is also *correct*.

### P-4. ISR atomicity across the two vectors is handled correctly — empirically confirmed

The interrupt gate (`0x8E00`, not a trap gate `0x8F00`) clears IF on delivery, so the first
ISR runs with IF=0 and KVM holds the second vector. The chaining only queues the second vector
*after* `step_one_entry` returns, which is after `iretq` has restored IF=1. I confirmed the
ordering empirically: vector 0x40 queued at icount 50000 (rip 0x1000e2), the trap after its
delivery lands at icount 50011 / rip 0x1000e6 (post-`iretq`, IF restored), then 0x41 queues
there. The delivery table reads `[0x40, 0x41]` in order with count==2 — no overwrite, both ran.

### P-5. The chaining fix is the *right* fix, and the comment explains *why* `+1` was wrong

Replacing `land_at(at.icount + 1, …)` with `step_one_entry` is exactly right: a `+1` landing
target would sit *inside* the delivery window (the 11-retirement ISR run), and `land_at` would
overshoot LOUDLY. The inline comment at `runctl.rs:271-274` states this precisely. This is the
correct mental model — "one entry, not one retirement" — and the boundary.rs doc-comment
generalizes it well (delivery suppresses the single-step; the returned boundary can be many
retirements ahead).

### P-6. The budget cap institutionalizes the 17-minute lesson, deterministically

`epoch_len` (50M) as the defer budget meant a masked guest would single-step 50M times — the
17-minute stall. `INJECT_DEFER_BUDGET = 1<<16` bounds it to ~tens of ms while staying
deterministic (it only caps a *loud* failure path; the success path is unaffected, so replay
identity is preserved). The const's doc-comment explains the reasoning. The masked-variant test
proves the bounded loud failure (`WindowNeverOpened`) fires and that `count==0` (no delivery
with IF=0).

### P-7. Determinism and drift discipline

- 5/5 fresh-process runs produced an identical `(icount, rip, hash)` tuple.
- The `timer_guest_table_gpa_matches` drift test parses the asm `%define` and asserts it equals
  the Rust `TIMER_GUEST_TABLE_GPA` — the ABI cannot silently drift.
- Each test boots a fresh rig, so the guest's persistent delivery `count` does not accumulate
  across tests (non-reentrancy across segments is moot — no shared rig).
- The `inject.rs` unit tests pass an explicit budget directly to `inject_at_boundary`, so they
  are correctly insulated from the production const change.
