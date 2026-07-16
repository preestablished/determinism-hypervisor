# Critical and Important Findings

**None.**

Every item the brief asked me to scrutinize was checked against the SDM and/or
live behavior and found correct. The notable adjudications:

## 1. x86 guest correctness — all encodings verified (no finding)

- **GDT built before IDT, lgdt before lidt.** Correct and necessary: interrupt
  delivery reloads CS from the GDT (the loader fills only the hidden segment
  caches; a descriptor fetch with no in-memory GDT reads zeros and triple-faults,
  matching the asm comment / the live finding). The two non-null descriptors
  match the cached selectors (0x08 code64, 0x10 data).
- **code64 `0x00209A0000000000`** decodes to type=0xA (execute/read), S=1,
  DPL=0, P=1, **L=1, D=0** — the only legal combination for a 64-bit code
  segment (D must be 0 when L=1). Correct.
- **data `0x0000920000000000`** decodes to type=0x2 (read/write), S=1, P=1.
  Correct.
- **Gate attr word `0x8E00` stored at `[rbx+4]`** places byte4=0x00
  (IST=0/reserved) and byte5=0x8E. 0x8E = P=1, DPL=0, S=0 (system descriptor),
  type=0xE (64-bit interrupt gate). Because it is an *interrupt* gate (not a
  trap gate), entry clears IF — so the ISR runs with interrupts masked and the
  non-reentrant `count read → byte store → inc` sequence cannot nest. Correct.
- **IDTR limit `0x42*16-1` = 0x41F** covers vectors 0..0x41 inclusive — exactly
  the two installed gates plus the architectural slots below them. Correct.
- **ISR clobbers.** `push rax/rbx` … `pop`, `iretq`. RFLAGS (incl. IF) is saved
  on the stack by the gate and restored by `iretq`. No other GPR is touched.
  The table write order is read-count → store-byte → inc-count; with IF=0
  there is no reentrancy, so a torn count is impossible. Correct.
- **TABLE_GPA 0x200000.** Verified the built ELF: PT_LOAD #1 (.text/.data) ends
  ~0x100182, PT_LOAD #2 (.bss) is `[0x101000, 0x107440)`. 0x200000 (2 MiB) is
  well above the image, is *not* inside any PT_LOAD, and is covered by the
  identity 2 MiB page tables as present+writable and zeroed at boot. The
  host-side read of `count` (u64 LE) then `count` vector bytes is consistent
  with the asm writer. Correct.

## 2. `step_one_entry` contract (no finding for the actual caller)

The function's "one entry" semantics are correct **for its sole caller**. In
`run_segment`, the `exits!()` macro converts `VcpuExit::Hlt` into an `Err`
(`halted=true`), so the `Ok(exit) => on_exit(...)` arm can never re-enter the
guest on a halt — the loop breaks immediately. An MMIO/PIO exit serviced as
`Ok(())` correctly resumes the *same* instruction (it had not retired), so the
"one entry" invariant holds. Single-step is dropped on every path (incl. error),
guard drops in scope order, and the counter is read *after* single-step is
turned off. No R10 (guest-visible TF) leak. See suggestion 02-#1 for a doc
nicety about the generic contract wording.

## 3. Delivery-suppresses-step (no finding — empiric is the contract)

The doc claims an entry that delivers an interrupt runs the *whole* handler
before the single-step trap fires. This is observed live (50011 vs 50001) and is
KVM-emulated-singlestep-over-event-injection behavior on this kernel. It is the
**correct thing to depend on for determinism** precisely because record and
replay run the same kernel — identical to every other empiric this engine relies
on (skid=18, PERIOD-takes-effect-immediately). The doc already scopes the hazard
(§3.2 NEAR landings must not target inside a delivery window; M6 owns that), and
the `idt_guest_tests` prove the chained outcome is bit-identical across boots,
which is the property that actually matters. No finding.

## 4. `INJECT_DEFER_BUDGET = 1<<16` (no finding)

Replacing `seg.config.epoch_len` (50M) with a fixed 65536 makes the masked test
terminate in ~1–2s instead of single-stepping for minutes, while keeping
`WindowNeverOpened{stepped:65536}` fully deterministic. Confirmed there is
exactly **one** call site of `inject_at_boundary` in `runctl` (it now uses the
constant), and the `inject.rs` unit tests pass explicit literal budgets (e.g.
250), so they are unaffected. The masked live test asserts both the
`WindowNeverOpened` error class and `count==0` (no delivery with IF=0) — the
right two assertions.

## 5. runctl chaining (no finding)

`halted` handling is preserved across both the chaining step and the inject call.
After `step_one_entry`, the boundary is wherever the post-ISR trap fired; the
second vector is queued there. This is deterministic and matches the
"consecutive entries" model in the agenda docs. The two-vector test proves
delivery order == schedule order (0x40 then 0x41).
