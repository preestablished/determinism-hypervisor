# Suggestions

### S-1 — `budget = deadline + ε` reads cleaner than `budget == deadline` + the FIRES-1 caveat

**File:** `tests/determinism/tests/timer_determinism.rs:32,39-43`

Today the segment budget equals the deadline, so the final segment's vector is
**queued but never entered** before the segment stops — hence the assertion
`count != FIRES - 1` and the comment "budget == deadline merges the points."
The off-by-one is correct but subtle, and it makes the ISR-count check read as
a special case.

**Verified experiment (live, /dev/kvm).** I changed the budget to
`deadline + 1000` and the expectation to `count == FIRES`, then ran 5 cold-boot
runs:

- Result: PASS. The ISR table count became **10 (= FIRES)** — the extra 1000
  icount per segment lets the queued vector deliver into the guest before the
  segment stops, so every fire is observed including the last.
- The `delivered`-icount list stayed **byte-identical** across runs and the
  gate still passed.

So `budget = deadline + small_slack` makes the ISR count equal the natural
`FIRES` and removes the "merge" caveat, at the cost of a tiny bit more guest
execution per segment. The delivered-icounts (the actual determinism property)
are unchanged. This is a readability/clarity win, not a correctness fix — the
current code is right. Take it or leave it; if left as-is, the comment already
explains the merge, which is acceptable.

### S-2 — `dh-cli gate` cmdline `'1000000000'` first byte `'1'` could collide with a future mode letter

**Files:** `tools/dh-cli/src/gate.rs:469,485`; `tests/nanokernel/asm/timer_guest.asm:66-78`

`cold_fingerprint` boots `timer_guest_elf()` with cmdline `"1000000000"`. The
guest's mode dispatch switches on the **first byte only** (`'m'/'a'/'d'`),
so `'1'` falls through to `.open_window` (STI + spin) — correct today. But the
mode select is "first byte == letter", and `'1'` is a perfectly valid first
byte for some future mode. If a new mode ever keys on a digit, this silently
changes behavior. Nit-level: either pass an explicit empty/`b""` cmdline (as
the `timer_determinism` test does — it relies on the same STI fallthrough via
the empty path) or comment in `gate.rs` that the digits are an intentional
no-op string chosen only because they're not a mode letter.

### S-3 — No cross-boot ("statistical honesty") note exists for the 100-run claim

**Files:** `crates/dh-verify/src/gate.rs:24` (doc); `tests/determinism/tests/{timer_determinism,if0_deferral}.rs` headers

I grepped beads (`bd list`) and `.agents/docs/determinism-hypervisor/` and found
**no** note covering this. The "100 runs, zero divergence" claim runs all 100
iterations in **one process on one boot of the host**. That meaningfully samples
*within-boot* host-state variation — PMI/skid timing, scheduler interference,
cache/TLB residency, page-cache warmth — which is exactly the noise that could
perturb the landing/injection machinery, so the gate is genuinely valuable. The
guest is deterministic by construction, so ASLR of the guest is not a variable;
host-side KASLR, microcode/MSR power-on defaults, P-state/turbo, and SMT-sibling
contention are **not** sampled because they're fixed for the life of the
process. The gate therefore **cannot** catch a host-reboot-dependent divergence
(e.g., a determinism leak that only appears under a different microcode rev or
KASLR slide). That's correctly the dedicated cross-boot runner's job — but the
limitation should be stated so nobody over-reads the green checkmark.

**Recommendation:** add one sentence to the gate doc (or a bead) noting that
zero-divergence here is *within a single host boot* and that cross-boot identity
is delegated to the dedicated runner / CI matrix. See A-2.

### S-4 — `regression.rs` re-implements `kvm_usable`/`gettid` instead of reusing `common`

**File:** `tests/determinism/tests/regression.rs:24,36` vs `tests/determinism/tests/common/mod.rs:170`

`common::kvm_usable` now exists and is the canonical probe (it even classifies
`PermissionDenied` distinctly and panics on unexpected errno). `regression.rs`
predates the rig and carries its own `kvm_usable`/`gettid`. Not in this diff's
scope, but consolidating onto `common` would remove the duplicate probe and
keep one definition of "is KVM usable" for the whole suite. Low priority.

### S-5 — Gate harness fingerprints are `String`; fine for v1, document the choice

**File:** `crates/dh-verify/src/gate.rs:33-37,67-71`

Fingerprints are `format!`-built `String`s compared with `!=`. Collision risk is
nil (these are full hex hashes plus structured fields), and the artifact wants
strings anyway. The only cost is allocation per run, irrelevant at 100 runs.
Worth a one-line doc that the fingerprint is an *opaque equality token* so a
future caller doesn't try to parse it. No change needed.

### S-6 — `if0_deferral` comment says "~12k masked instructions"; the loop is 2000 × 6 ≈ 12k — keep them in sync

**Files:** `tests/determinism/tests/if0_deferral.rs:17` ("~12k"); `tests/nanokernel/asm/timer_guest.asm:96` ("2000 iterations x 6 instructions")

These agree (2000 × 6 = 12000), and the deferral cost (~7k steps/run) sits well
inside `INJECT_DEFER_BUDGET = 65536`, so `WindowNeverOpened` can't false-fire.
The only fragility is that the two magic numbers live in two files: if the
`.defer_mode` iteration count is ever raised past where the deferral step count
exceeds 65536, the test flips from PASS to a *loud* `WindowNeverOpened` (good —
fails closed, not silently). Consider a comment in the asm pointing at the
budget constant, or a const shared via `nanokernel` so the relationship is
explicit. Minor.
