# Suggestions

### S1 — The "19 retirements per iteration" comment is internally contradictory; coverage claim depends on it

**File:** `tests/nanokernel/src/lib.rs` (mmio_stepper_elf doc) +
`crates/dh-vmm/src/boundary.rs:484-486` (the 120-landing test doc).

The lib.rs comment says: *"19 retirements per iteration (the three MMIO
instructions are emulated and never retire)."* The asm loop body is:

```
mov dword [rbx+0x14], 1   ; MMIO  (emulated)
mov [rbx+0x08], rbx       ; MMIO  (emulated)
mov eax, [rbx+0x18]       ; MMIO  (emulated)
nop x16                   ; 16 retiring
sub ecx, 1                ; 1 retiring
jnz .l                    ; 1 retiring
```

If the three MMIO instructions "never retire," the body is **16 + 1 + 1 = 18**
retirements, not 19. The "19" and the parenthetical contradict each other. Two
possibilities:
- **18 is right** (the hardware INST_RETIRED PMC does not count
  emulator-executed instructions): then the doc number "19" is wrong.
- **19 is right**: then ONE of the MMIO instructions actually does retire on
  this host's counter, contradicting the parenthetical.

This is not a correctness bug — the landing tests assert exact `icount`
regardless of the per-iteration count, and they pass. But it undermines the
adjacent coverage claim (S2) and should be made accurate. The cleanest fix is
to state the empirically observed count and drop the unverifiable "never
retire" assertion, or pin it (S2).

### S2 — The "every instruction offset in the body" coverage claim is only true if body = 19

**File:** `crates/dh-vmm/src/boundary.rs:484-486` and the test at 487-505.

The 120-landing test marches `target += 1 + (k % 23)` from 100. The claim is
"every step-walk distance from 1 to a full cluster width gets exercised against
every instruction offset in the body." I checked the arithmetic:

- The 23-periodic stride sequence DOES cover every distance 1..23 (verified —
  `1 + (k%23)` hits all of {1..23}). The stride claim is fully correct.
- The OFFSET claim depends on the body length the targets are taken modulo:
  - body = **19**: the landed offsets hit **19/19** distinct positions —
    full coverage. ✓
  - body = **18**: the landed offsets hit only **12/18** — offsets
    {prologue-relative 1,4,7,10,13,16 in one accounting} are never landed on,
    because the alternating-parity interaction of stride-period 23 against
    body 18 leaves a gcd-style hole.

So whether the "every offset" claim holds is the SAME open question as S1. If
the body really is 18, the test does NOT cover 6 of the offsets — which is
fine for a regression (it still spans hundreds of MMIO clusters and would have
caught the original bug), but the comment overstates it. **Recommendation:**
either (a) confirm body=19 and fix S1's contradiction so the claim stands, or
(b) if body=18, weaken the comment to "a broad spread of offsets and every
stride 1..23," OR change the stride generator to one that is coprime with the
true body length (e.g. `1 + (k % body)` with an odd step) to guarantee exact
offset coverage. Exact-coverage suggestion: pick a stride sequence whose
partial sums are a complete residue system mod `body` — e.g. constant
stride `s` with `gcd(s, body) = 1` visits all `body` offsets in `body` steps.

### S3 — `mmio_stepper` carries no const drift pin, unlike entropy_draw/pad_echo — confirm intentional (it is)

**File:** `tests/nanokernel/tests/elf_shape.rs:68`, `tests/nanokernel/src/lib.rs`.

Other device guests pin their asm `%define`s against Rust constants
(TABLE_GPA, ENTROPY_DRAW_BATCH, RING_CAPACITY) because the host reads guest
memory at those addresses or asserts batch counts — drift would silently
desync host and guest. `mmio_stepper` exports NO constants (no
`MMIO_STEPPER_BASE`/`ITERS`) and the probes (`ack_mmio`, the two landing
tests) are address-agnostic: `ack_mmio` matches any `MmioWrite`/`MmioRead`
and zero-fills, and the landing targets are bare numbers. Nothing
cross-references `MMIO_BASE` or `ITERS`, so nothing CAN drift. **This is the
correct call — no pin needed.** Noting it only so the next reader doesn't
"fix" the apparent inconsistency. If ITERS is ever lowered such that a probe
target exceeds the loop's total retirements, the test would fail loudly
(Overshoot at the `hlt`), so the dependency is self-checking. Worth a one-line
comment near `mmio_stepper_elf` saying "no const pin: probes are
address-agnostic and never read these %defines."

### S4 — Memorialize the lessons via a code comment + bd remember, not just the commit body

**Process.** The commit body records two hard-won lessons: (1) the
`immediate_exit` completion belt does NOT work on 6.8 because the EINTR check
pre-empts `complete_userspace_io`; (2) raw-code real-mode probes are vacuous
(real mode misdecodes 64-bit encodings and can't reach the MMIO hole). Both
are exactly the kind of thing a future agent will re-derive painfully. The
revised root cause (TF survives MMIO *completion*; the EMULATOR's Debug
delivery is what disarms) also corrects the bead's original hypothesis and the
iteration-82 commit's framing ("MMIO-write exit can lose the trap"). Suggest:
- A short `bd remember` (or note on 4a3) capturing the corrected mechanism so
  the stale "MMIO-write eats the trap" framing in two prior commits and the
  bead description doesn't mislead.
- A one-line breadcrumb in `boundary.rs` near the step-walk docs (module
  header or line 162) pointing at "raw-code probes must be long-mode guests
  (real mode can't reach the MMIO hole) — see mmio_stepper" so the next probe
  author starts from a working pattern.
