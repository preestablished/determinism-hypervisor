# Positive Notes

## P1 — Fail-closed XSAVE2 guard instead of silent truncation

`capture()` (`112–119`) hard-errors when `KVM_CAP_XSAVE2` reports an area larger
than the fixed 4096-byte `KVM_GET_XSAVE` can carry, with a message that names the
host size and points at the documented XSAVE2 follow-up. This is exactly the
determinism-platform posture: on an AMX-class host you get a loud, actionable
error rather than a silently-truncated XSAVE blob that would hash-diverge or lose
guest FPU state. The guard is correctly placed in capture (where truncation could
happen) and the cap is `.max(0)`-clamped before the comparison.

## P2 — The §8.3 ordering is implemented *and* annotated with the "why"

Each ordering step carries a comment explaining the constraint it satisfies —
"XCRS then XSAVE — the reverse is wrong (§8.3): XSETBV after XRSTOR would re-init
enabled components", "FPU before XSAVE: XSAVE is authoritative for the x87/SSE
overlap". This is the kind of load-bearing comment that prevents a future
refactor from reordering the calls and silently breaking restore. The FPU-before-
XSAVE decision (unstated by the spec) is the correct one and is justified inline.

## P3 — TSC decision honored verbatim, not reopened

`restore()` uses the `KVM_VCPU_TSC_OFFSET` attribute via `crate::tsc::
set_tsc_offset` with `offset = vns − rdtsc()`, matching `tsc-alignment.md`
word-for-word, and the forbidden per-entry `SET_MSRS{IA32_TSC}` path is absent.
`RESTORE_MSR_LIST` deliberately omits IA32_TSC with a comment explaining the TSC
is written from vns at restore, never carried as captured data — the two
mechanisms are kept cleanly separate.

## P4 — Padding-clean codec, and the structs were chosen knowing it

The byte-copy codec works precisely *because* all six kvm-bindings structs have
only named padding/reserved fields (no compiler-inserted implicit padding). The
module doc and the `struct_bytes` SAFETY comment both correctly frame this as the
repr(C)-POD justification API.md §4 specifies. This is the same hazard class
iteration 69 fixed for XSAVE, and here it was reasoned about correctly up front
rather than discovered as a bug.

## P5 — Total, fail-loud decoder with a code-versioned cross-check

`decode_section` is genuinely total: bounds-checked struct reads, explicit XSAVE
length validation, MSR count + per-index validation against the code-versioned
`RESTORE_MSR_LIST`, nonzero-`_pad` rejection, and a trailing-byte check. The
`msr[i] is X, want Y (list is code-versioned)` error ties the wire format back to
the source-of-truth constant — a peer running a different MSR list fails loudly
instead of decoding garbage.

## P6 — `PartialEq` defined as byte-equality of the canonical encoding

`impl PartialEq for VcpuState` compares `encode_section(self) == encode_section
(other)` — "exactly the equality the determinism platform cares about." This is
the right semantic choice (raw struct `PartialEq` could differ on a reserved byte
that the canonical encoding normalizes), and it makes the live GET→SET→GET
fixed-point test assert the property that actually matters for the state hash.
