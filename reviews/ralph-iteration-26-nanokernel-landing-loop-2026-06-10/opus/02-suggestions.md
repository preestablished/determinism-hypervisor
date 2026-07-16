# Suggestions (non-blocking)

### S-1. The `"0" → default` semantic is defensible but worth a one-line rationale in-code

cmdline `"0"` keeps the default 12.5M iterations (landing_loop.asm:60–61). The
asm comment says "keeps the default, never instant-exit," which is the right call
for a landing-test fixture: a 0-iteration run produces a useless near-empty
icount and would *hide* harness bugs (a harness that accidentally passes `"0"`
would silently get an instant exit and might mark the test green). Treating `"0"`
as "no meaningful request" is the safer default.

The one wrinkle: the bead says iterations "scale by BootInfo cmdline," and a
strict reading of "scale" would honor `0` as `0`. The chosen behavior is a
reasonable interpretation (you can never get fewer than `DEFAULT_ITERS` from a
digit-only cmdline, and that floor is a feature), but it is a *policy decision*
that the lib.rs doc mentions only in passing ("no digits or `0` → default"). Add
one sentence to the asm explaining *why* `0` is floored — so a future reader
doesn't "fix" it into honoring `0` and accidentally enable instant-exit.

### S-2. Parse loop silently accepts overflow on absurdly long digit strings

`imul rax, rax, 10` wraps mod 2^64 on cmdlines with ~20+ digits
(landing_loop.asm:51). This is deterministic (same cmdline → same wrapped value)
and therefore not a correctness bug for the determinism property, but a
pathological cmdline like `"99999999999999999999999"` yields a meaningless
iteration count with no diagnostic. Given these are test fixtures driven by the
harness (not untrusted input), this is acceptable as-is. If you want belt-and-
suspenders, cap the parse (e.g. stop accumulating past 10 digits, or saturate),
but it is genuinely optional.

### S-3. Consider asserting the loop-body region size in the asm via a `%assign`/`times` guard

NASM can self-check the loop length at assemble time. For example, bracket the
loop with labels and `%if (.loop_end - .loop) > N` or use a `times` sanity
construct. This catches an accidental extra instruction at *assemble* time
(build break) rather than at icount-mismatch time in a hardware-gated harness
run. Pairs well with I-1's Rust-side guard.

### S-4. `elf_shape` generalization: nudge toward data-driven over hand-listed

`every_guest_is_a_static_x86_64_exec_at_the_load_addr` now hand-lists the two
guests (elf_shape.rs:56–59). This is fine and explicit, but every new guest must
be added here by hand (the same maintenance tax that `build.rs::PROGRAMS`
already carries). Not worth a macro today with two guests, but if a `guests()`
slice of `(name, &[u8])` were exposed from lib.rs, both the shape test and any
future per-guest test could iterate it, and forgetting to register a guest would
fail in one obvious place. Purely a future-proofing note.
