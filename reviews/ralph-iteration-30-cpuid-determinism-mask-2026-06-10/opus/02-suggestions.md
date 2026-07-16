# Suggestions (non-blocking)

### S1 — Consider clearing `WAITPKG` (CPUID.7.0:ECX[5]) — umwait/tpause are timing primitives

ARCHITECTURE §7.2 is explicitly "non-exhaustive… the code is the source of truth." The
mask handles MONITOR/MWAIT (the legacy C-state wait pair) but does **not** touch
`WAITPKG` (CPUID.7.0:ECX.WAITPKG[bit 5]), which advertises `UMONITOR`/`UMWAIT`/`TPAUSE`.
`UMWAIT`/`TPAUSE` take a **TSC deadline operand** and are user-mode timed-wait
primitives — the same class of host-clock-coupled nondeterminism the mask exists to
close (and a sibling of the already-cleared MONITOR and TSC_DEADLINE). On this Coffee
Lake box WAITPKG is not advertised (it appears on Tremont/Tiger Lake and later), so it
does not show up in the live diff and is harmless *here* — but as a fleet grows to
newer parts, an unmasked WAITPKG would let a guest spin on `TPAUSE`/`UMWAIT` against
the host TSC. Low-cost, high-consistency add: clear `1 << 5` in the `(7, 0)` ECX arm
with a comment ("umwait/tpause: host-TSC-deadline user wait"). Mirrors §4's TSC-as-
radioactive stance. (Flagged here rather than Important because it is a forward-fleet
hardening, not a defect on the pinned lab host.)

### S2 — Consider masking `TSC_ADJUST` (CPUID.7.0:EBX[1])

`IA32_TSC_ADJUST` lets a guest offset its TSC; §4 already treats the TSC as radioactive
and aligns it per-entry, and the MSR filter default-denies the MSR. Advertising
`TSC_ADJUST` in CPUID while denying the MSR is a small inconsistency — a guest that
reads the CPUID bit and then traps on the MSR write gets a #GP it "shouldn't" per its
own feature probe. Clearing CPUID.7.0:EBX.TSC_ADJUST[bit 1] keeps the advertised
feature set honest with the MSR policy. Low priority; the guest contract (guest-sdk)
already forbids touching TSC plumbing, and verification mode is the backstop.

### S3 — `cpuid_table_hash` `Vec::with_capacity(len * 24)` undercounts by the flags field

`cpuid.rs` line 128 reserves `entries.len() * 24` bytes, but each entry now serializes
**7** u32 = 28 bytes (function, index, flags, eax..edx). The capacity hint is 4 bytes/
entry short, so the `Vec` will do one reallocation. Purely cosmetic (correctness
unaffected), but since the comment implies an exact preallocation, bump it to `* 28`.

### S4 — `cpuid-diff` only reports the supported→masked direction; add an explicit "no masked-only entries" assertion

The diff loop iterates `sup` and looks each key up in `msk`. By construction
`masked ⊆ supported` (the mask only clears bits and `retain`s a subset), so entries
present in `masked` but absent from `supported` are impossible — and the tool silently
relies on that. Consider a one-line trailing check (`debug_assert` or a printed
"masked-only entries: 0") so a future regression that *adds* a leaf (e.g. a synthesized
PV leaf) is visible in the acceptance dump rather than silently dropped. Diagnostic
hardening only.

### S5 — `hex()` helper duplicates a common util; consider centralizing

`tools/dh-cli/src/cpuid.rs` defines a local `hex(&[u8;32]) -> String`. There is likely
an existing hex/`payload_digest` rendering path elsewhere (e.g. the JSON boot output or
`dh-inputlog`). Minor: fold into a shared helper to avoid drift in how hashes are
rendered across CLI subcommands. Not worth blocking.
