# Suggestions (non-blocking polish)

### S1. Constant name overstates generality — `COUNTING_DELTA_AT_OUT_EXITS`
`tests/nanokernel/src/lib.rs:130`. The empirics are explicitly lab-box /
kvm-intel-class (the doc comment says so), but the constant name reads as a
universal truth. Bead gfb (single-step attribution) will rely on this; a name
like `COUNTING_DELTA_KVM_INTEL` or a one-line "class-specific; AMD/other KVM
may differ" caveat at the constant (not just buried in the prose) would keep a
future cross-vendor run from silently trusting 997. The honesty is already in
the comment — this is just hoisting it to the name/first line.

### S2. The asm "3 exiting instructions" count is correct but implicit
`tests/nanokernel/asm/counting.asm:11-16` and `lib.rs:111-113`. The region has
exactly 3 exiting instructions, which I verified by disassembly (CPUID @
0x100040, MMIO read @ 0x100047, MMIO write @ 0x10004b — nothing else exits).
Consider an assembly-time guard analogous to the `%if ICOUNT != 1000` check:
e.g. `%assign EXITCOUNT 0` bumped by a dedicated `XI` macro wrapping the three
exiting instructions, with `%if EXITCOUNT != 3 %error`. That would make the
"3" in `COUNTING_EXIT_INSTRS_IN_REGION` self-enforcing at build time the same
way the 1000 is, instead of a hand-maintained pair.

### S3. The MARK-setup accounting is subtle — document the off-by-window
The 'E' marker's `mov dx` / `mov al` (counting.asm:91-92) are INSIDE the
region (counted) while the OUT itself is outside. This is correct, but it is
the kind of boundary detail that bites the next editor. A one-line comment at
line 91 noting "these two retire INSIDE the window; only the OUT is excluded"
would help (the asm already hints at it, but the precise count consequence is
worth stating).

### S4. Smoke could assert the absolute S/E counter values, not just the delta
`counting_smoke.rs:154`. The test asserts `e - s == 997`. The absolute values
are also stable (`s == 6`, `e == 1003` on this box from the prologue +
region). Asserting `s` as well would catch a regression where crt0/prologue
instruction count drifts (which would shift both endpoints together and leave
the delta — but not the absolute landing — wrong). Optional; the delta is the
load-bearing invariant.

### S5. Leftover reviewer scratch in the working tree (housekeeping)
At review start the tree contained an UNTRACKED
`tests/determinism/tests/zz_scratch_counting_probe.rs` from a prior reviewer
session (not part of this iteration's commit). It is harmless (untracked, and
`reviews/` debris is gitignored) but worth deleting so it doesn't accrete. I
removed it as part of my own scratch cleanup; flagging so the author knows it
existed and isn't surprised.
