# Critical & Important findings

## Critical

None. Determinism is unaffected (verified: `timer_determinism` 100-run zero-divergence and
`counting_smoke` both pass live), the layout table matches authoritative constants, and nothing
in the diff changes shipped behavior.

---

## Important

### I1 — §3.1 overclaims its evidence scope: "MEASURED" now covers HLT and PIO-IN, which were never isolated

**File:** `.agents/docs/determinism-hypervisor/ARCHITECTURE.md` lines 233–238

The new sentence reads:

> VM-exiting instructions (`CPUID`, `HLT`, PIO, MMIO) retire **zero** guest instructions —
> MEASURED on the kvm-intel class (counting guest, bit-stable across cold
> boots/cores/processes/load; see `nanokernel::COUNTING_DELTA_AT_OUT_EXITS`).

What the counting guest actually brackets (verified in `tests/nanokernel/asm/counting.asm`
lines 57–122 and `COUNTING_EXIT_INSTRS_IN_REGION = 3`):

| Instruction class | In measured S→E region? | Status |
|---|---|---|
| OUT (the `S`/`E` markers) | brackets the region (contributes 0) | measured = 0 |
| CPUID (leaf 0) | yes (`XI cpuid`, line 71) | measured = 0 |
| MMIO read (pv-clock VNS) | yes (`XI`, line 76) | measured = 0 |
| MMIO write (serial THR mirror) | yes (`XI`, line 77) | measured = 0 |
| REP MOVSB | yes (line 67) | measured = 1 |
| **HLT** | **NO** — sits in crt0 *after* the region (line 122: "crt0 parks in HLT") | **NOT measured** |
| **PIO IN** | **NO** — the region contains no `IN` at all | **NOT measured** |

This is corroborated by the project's own issue tracker, not just my reading:

- **Bead gfb** (counting_semantics) notes, verbatim: *"NOTE: HLT retirement is NOT yet measured
  (the smoke ends at HLT without bracketing it) — measure it here before relying on it."*
- **Bead 0sc** (the reconciliation bead this change implements) scopes the measurement to exactly
  *"VM-exiting instructions (PIO OUT, CPUID, MMIO access)"* — note **OUT**, not PIO generally, and
  no HLT.

So the sentence asserts as MEASURED FACT two things that are reasoned-about-but-unisolated:

1. **HLT = zero.** Plausible (HLT exits, KVM resumes by skipping RIP, same mechanism as the
   others), but explicitly flagged as not-yet-bracketed by gfb. Asserting it "MEASURED" is an
   overclaim that pre-empts the very measurement gfb says must happen first.

2. **PIO generally = zero.** Only OUT was isolated. IN exits *are* exercised heavily (hello.asm
   line 31 `in al, dx` LSR-poll, run through the 5 `boot_hello` tests, plus run_segment
   determinism), and those icounts are bit-identical across runs — but that **constrains** IN
   retirement to be *deterministic*, it does not **isolate** it to *zero*. A consistent +1 per IN
   would be equally bit-stable. The empirics bound the variance, not the value.

**Why it matters:** in this repo docs are normative. A future implementer reading "HLT retires
zero, MEASURED" will skip the gfb measurement and may build a boundary-attribution table that is
silently wrong for HLT. The honest, still-strong claim distinguishes measured from inferred.

**Recommended rewording** (keeps the strong measured core, demotes the rest to a typed inference):

> VM-exiting instructions retire **zero** guest instructions: they exit before retirement and KVM
> completes them host-side by skipping `RIP`, which an `exclude_host=1` counter never sees.
> **Measured** on the kvm-intel class for `CPUID`, `OUT`, and MMIO read/write (the counting guest's
> S→E window; bit-stable across cold boots/cores/processes/load; see
> `nanokernel::COUNTING_DELTA_AT_OUT_EXITS`). The same RIP-skip mechanism is **expected** to give
> zero for `HLT` and PIO `IN`, but those are not yet isolated by the counting guest (HLT parks in
> crt0 past the region; the region contains no `IN`) — re-confirm via bead gfb before an
> attribution table relies on them. Like the interrupt rule, this is a per-determinism-class
> measurement — re-validate per class, never assume across classes.

(If a tighter spec voice is preferred, at minimum change "(`CPUID`, `HLT`, PIO, MMIO) … MEASURED"
to scope MEASURED to `CPUID/OUT/MMIO` and mark HLT and IN as "expected, not yet isolated.")

---

### I2 — Stale-comment update is incomplete: counting.asm still says MMIO read/write "retire once," contradicting the same file and the new §3.1 rule

**File:** `tests/nanokernel/asm/counting.asm` lines 21–24

The iteration updated the CPUID line of the region-composition block:

```asm
;   CPUID (leaf 0)      — in-kernel emulated; retires ZERO (measured)   ; line 20 — UPDATED
```

…but left the two adjacent MMIO lines describing the *old, refuted* rule:

```asm
;   MMIO read           — pv-clock VNS (0xD000_0000+0x08), exits, retires
;                         once on the completing resume                  ; lines 21–22 — STALE
;   MMIO write          — debug-serial THR mirror (0xD000_6000+0x08),
;                         byte 'M', exits, retires once                  ; lines 23–24 — STALE
```

These two instructions are emitted with the **`XI` exiting-instruction macro** (lines 76–77), i.e.
they are precisely the class the new ARCH §3.1 rule and this same file's own header (line 13,
"instructions retire ZERO") and line 20 ("retires ZERO (measured)") say retire **zero**. The
comment now contradicts itself three lines apart and contradicts the spec the iteration just
landed. This is the literal stale comment the iteration's commit message claims to have updated —
it caught CPUID but not the two MMIO siblings.

**To be clear this is comment-only:** line 80 ("each retires exactly once") is *correct* and should
NOT be changed — it describes plain conditional/unconditional branches (`I`-macro, non-exiting),
which do retire once. The bug is confined to lines 21–24.

**Recommended fix** — bring the MMIO lines in line with line 20:

```asm
;   MMIO read           — pv-clock VNS (0xD000_0000+0x08), exits; retires
;                         ZERO (measured; RIP-skipped host-side)
;   MMIO write          — debug-serial THR mirror (0xD000_6000+0x08),
;                         byte 'M', exits; retires ZERO (measured)
```

(Touching counting.asm forces a nasm rebuild; I confirmed the rebuild + `counting_smoke` still
pass, so this edit is safe.)
