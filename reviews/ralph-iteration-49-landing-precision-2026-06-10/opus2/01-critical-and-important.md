# Critical & Important findings

## Critical

None.

---

## A. (Verification, not a defect) Residue→RIP analysis CONFIRMS instruction-start landing

This was my primary review angle: the committed test asserts RIP
correctness *only* via cross-boot equality (`assert_eq!(first, second)`).
Two boots could in principle land at the same **wrong** RIP — e.g.
deterministically one instruction late — and the test would still pass.
So I checked it independently.

For the landing loop (8 retired instructions/iteration) and the REP loop
(6/iteration), if a landing is at an instruction *start*, then targets
sharing a residue (`target mod 8`, resp. `mod 6`) — modulo the fixed
prologue offset — must all land at the **same** RIP, and the total set of
distinct RIPs must be **≤ 8** (resp. ≤ 6). I ran a scratch test over the
*same fixed-seed targets* and grouped landed RIPs by residue class
(reverted afterward; tree clean).

**Landing loop (2,000 targets):**
```
8 distinct RIPs total (expected ≤ 8)
residue 0 -> 0x1000bf   residue 4 -> 0x1000b0
residue 1 -> 0x1000c3   residue 5 -> 0x1000b4
residue 2 -> 0x1000ca   residue 6 -> 0x1000b7
residue 3 -> 0x1000ce   residue 7 -> 0x1000bb
nonfunctional residue classes: 0
```
Every residue class maps to exactly one RIP. The mapping is a function.
(The loop body begins at 0x1000b0 = residue 4 because of the prologue
offset; the cyclic ordering is exactly an 8-instruction body.)

**REP loop (600 targets):**
```
6 distinct RIPs total (expected ≤ 6)
res 0 -> 0x100030  rcx {0}
res 1 -> 0x100035  rcx {64}   <-- the rep movsb itself
res 2 -> 0x100037  rcx {0}
res 3 -> 0x10003b  rcx {0}
res 4 -> 0x100020  rcx {0}
res 5 -> 0x100028  rcx {0}
residue classes that ever show rcx==64: [1]
nonfunctional residue classes: 0
```
Disassembly confirms `0x100035` is exactly `f3 a4  rep movsb`. So
`rcx==64` occurs at **exactly one** residue class, and that class is the
REP instruction's start — never any other value, never any other RIP.

**Conclusion:** landings are at true instruction boundaries on both
guests; the "same wrong RIP in both boots" failure mode is ruled out, not
merely assumed. This is strong corroboration of the test's central claim.
No action needed — recorded so the next reviewer/maintainer knows the
cross-boot-equality assertion has been independently backstopped.

---

## Important

### I1. New `rep_loop` guest is missing from the elf_shape static-shape gate

`tests/nanokernel/tests/elf_shape.rs::every_guest_is_a_static_x86_64_exec_at_the_load_addr`
explicitly lists each guest and asserts ELFCLASS64 / ET_EXEC /
EM_X86_64 / `e_entry == load addr` / a PT_LOAD covering entry / size
< 64 KiB. The new `rep_loop` guest was added to `build.rs` PROGRAMS and
to `lib.rs` (`rep_loop_elf()`) but **not** to this list.

Impact: a future edit that breaks rep_loop's link (wrong entry, PIE,
oversize) would not be caught by the cheap host-runnable shape gate;
it would only surface as a slow hardware-gated landing test failure (or
silently if that lane is skipped). Low effort, locks the new artifact
into the same gate as its siblings.

Fix: add one line to that test:
```rust
assert_guest_shape("rep_loop", rep_loop_elf());
```

### I2. `REP_LOOP_INSTRS_PER_ITER` is exported but unused — and breaks the established sibling pattern

`lib.rs` adds `pub const REP_LOOP_INSTRS_PER_ITER: u64 = 6;`. It has
zero references anywhere in the tree. Because it is `pub` it produces no
dead-code warning, so it can rot silently.

The sibling `LANDING_LOOP_INSTRS_PER_ITER` is **not** decorative — it is
consumed by `lib.rs` (total-icount derivation) *and* by an
`elf_shape.rs` test that disassembles the landing loop and asserts the
loop body is exactly that many instructions. The new const has no such
backing test, so the asm `rep_loop` could drift to 5 or 7 instructions
per iteration and nothing would notice (and the test's `target mod 6`
mental model in my §A would silently no longer correspond to the guest).

Two acceptable resolutions:
1. **Preferred:** add a `rep_loop_asm_matches_rust_constants` shape test
   mirroring the existing `landing_loop_asm_matches_rust_constants`, so
   the const is *used* to pin the 6-instruction body (and ideally the
   REP-MOVSB position). This is the highest-value way to keep the
   residue arithmetic honest over time.
2. Or, if no counting use is planned, drop the const (the test uses
   absolute targets, not iteration-derived ones — it never needs it) and
   keep only `REP_LOOP_RCX_AT_REP_START`, which *is* used.

Either way, an exported-but-unused numeric const that diverges from its
own asm is a maintainability hazard, not future-proofing.
