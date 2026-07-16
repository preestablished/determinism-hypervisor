# Critical & Important Findings

## Critical
None. No documented command failed; no number was wrong in a way that
misdirects an operator.

## Important

### I1. "bit-identical across margins 8192 vs 128" oversimplifies the landing-precision design
**File:** README.md, "Measured numbers" → Landing bullet.
**Claim:** "10,000 random targets ... bit-identical across margins 8192 vs 128
(§3.2 margin-independence proven live)."

**What the test actually does** (`tests/determinism/tests/landing_precision.rs`):
- `LANDING_TARGETS = 10_000`, `PRODUCTION_PREFIX = 100`.
- **First boot** (`pass_a_margins`): the first 100 targets use `Margins::default()`
  (skid_margin 8192); targets 100..10_000 — the overwhelming bulk — use
  **skid_margin/resync_slack = 256**, NOT 8192.
- **Second boot** (`pass_b_margins`): uniformly **128**.
- The assertion is that the full landed tuple sequence is identical across the
  two boots.

So the live proof is "8192-prefix + 256-bulk" replayed bit-identically as
"uniformly 128" — three distinct margins are in play, and only 100 of the
10,000 targets ever see 8192. The README phrasing "across margins 8192 vs 128"
reads as "all targets at 8192 on one boot vs all at 128 on the other," which is
not what runs. The source's own doc comment (lines 14–22, 118–120) is careful
about this exact distinction; the README flattened it.

**Why it matters:** This is a normative-correctness claim about the §3.2
margin-independence contract. An operator/reviewer reading the README would
believe the strongest possible statement (full 8192 vs full 128) is proven,
when the measured proof is the prefix/bulk design. The claim is *defensible* as
written only if "8192 vs 128" is read as "the production-margin prefix vs the
tight second boot" — but that requires reading the source to know.

**Fix (wording only):** e.g. "bit-identical across mixed margins (production
8192-prefix + 256 bulk on boot A, uniform 128 on boot B) — §3.2
margin-independence proven live on real targets." Or simpler: "bit-identical
under three different margin schedules (8192/256/128)."

**Severity rationale:** Important not Critical because the test genuinely passes
and genuinely exercises margin-independence; the gap is precision of the
public-facing claim, not a false test result.
