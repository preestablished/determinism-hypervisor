# Critical & Important Findings

## Critical

None. The artifact is correct: the loop body is exactly 8 instructions
(disassembly-verified), the ring buffer does not collide with the stack, the
parse logic is sound, and the determinism property the bead requires holds for a
fixed cmdline.

## Important

### I-1. The two new harness constants have no asm↔Rust drift test

`LANDING_LOOP_INSTRS_PER_ITER = 8` and `LANDING_LOOP_DEFAULT_ITERS = 12_500_000`
(src/lib.rs:49,53) restate values that are *authoritatively* defined in the asm:
the loop body's instruction count is implicit in `landing_loop.asm`, and
`DEFAULT_ITERS 12500000` is a `%define` at landing_loop.asm:25. This is exactly
the situation `bootinfo.inc` faced, which the team solved with
`bootinfo_inc_matches_rust_constants` (elf_shape.rs:64) — a test that parses the
`.inc` file's `%define`s and asserts they equal the Rust constants "so the two
cannot drift."

The new constants get no such guard:

- `LANDING_LOOP_DEFAULT_ITERS` can silently diverge from the asm `%define
  DEFAULT_ITERS`. If someone edits one and not the other, every harness that
  computes `8 * LANDING_LOOP_DEFAULT_ITERS + offset` as the expected icount for a
  no-cmdline run silently computes the wrong number, and `mov rcx, DEFAULT_ITERS`
  in the asm produces a different actual count. `default_iters_hit_the_100m_budget`
  only checks `8 * 12_500_000 == 100_000_000` — it pins the Rust side to itself,
  not to the asm.
- `LANDING_LOOP_INSTRS_PER_ITER = 8` is the most fragile value in the whole
  change: it is what makes the icount predictable, yet nothing mechanically ties
  it to the actual assembled loop. A future edit that adds or removes a loop-body
  instruction breaks the icount contract with zero test failures.

**Why it matters:** the entire point of this guest is a *predictable* icount for
M2/M3. A drift in either constant defeats that purpose silently — the most
dangerous failure mode for a determinism test fixture.

**Suggested fix (two parts):**
1. Promote `DEFAULT_ITERS` (and ideally `LANDING_LOOP_INSTRS_PER_ITER` /
   `BUF_QWORD_MASK`) into `include/`-style `%define`s in a small landing_loop
   include or reuse the existing parse pattern, then add an
   `elf_shape`-style test asserting the asm `%define` equals
   `LANDING_LOOP_DEFAULT_ITERS`.
2. Pin the instruction count to reality: disassembling in a test is heavy, but a
   cheap proxy is to scan `landing_loop.asm` for the `.loop:` … `jnz .loop`
   region and assert it contains exactly `LANDING_LOOP_INSTRS_PER_ITER`
   instruction-bearing lines (skipping comments/blank/`align`/label). Even a
   coarse line-count guard would have caught a stray extra `mov`.

### I-2. The `lib.rs` doc under-warns that the calibration offset varies with cmdline length

src/lib.rs:45–48 tells harnesses to compute `8 * iters + prologue` and says
"harnesses calibrate the exact offset once, it is deterministic." That is true
for a *fixed cmdline*, but the prologue/parse instruction count is **not
constant across cmdlines** — the `.parse` loop runs once per cmdline digit
(`movzx`/`sub`/`cmp`/`ja`/`imul`/`add`/`mov`/`inc`/`dec`/`jmp`, ~10 instructions
per digit), so a 1-digit cmdline and an 8-digit cmdline have measurably different
total icounts even before the main loop. "Calibrate the offset once" invites a
harness author to calibrate with `cmdline="100"` and then reuse that offset for
`cmdline="12500000"`, producing a wrong expected icount.

**Why it matters:** an M3 regression harness comparing actual vs expected icount
will see a spurious mismatch (off by ~50–70 instructions) if it caches the
offset across cmdlines of different digit-lengths — and will likely blame the
hypervisor's determinism rather than its own calibration.

**Suggested fix:** amend the doc to state the offset is deterministic *per
cmdline* and varies with the number of parsed digits; recommend calibrating with
the exact cmdline that will be used, or using the no-cmdline / empty-cmdline path
(constant offset) as the canonical calibration point.

### I-3. `prog_main` clobbers `r12` (callee-saved by SysV) with no note

`r12` is callee-saved in the System V AMD64 ABI; `prog_main` loads `ring_buf`
into it and never restores it (landing_loop.asm:68). In practice this is
**harmless** — crt0 calls `prog_main` and then unconditionally `hlt`-parks
(crt0.asm:22–25), so no caller ever observes the clobbered `r12` — and the
crt0 contract is explicitly "SysV-ish: no args." But the guest's own header
calls it "SysV-ish" while quietly violating the callee-saved half of that
convention, and `pipeline_smoke.asm` (the sibling guest) happens to touch only
caller-saved registers, so this guest is the first to break the pattern.

**Why it matters:** low, but if any future crt0 change does work after
`call prog_main` (e.g. reads a result, computes a hash of register state), the
silent `r12`/`r10`/`r11` clobbers become real bugs. A one-line comment now is
cheap insurance.

**Suggested fix:** add a comment at the top of `prog_main` noting it clobbers
all GPRs including callee-saved `r12` and relies on crt0 never doing work after
the call (HLT park). Optionally tighten the crt0 header to say "prog_main may
clobber every register."
