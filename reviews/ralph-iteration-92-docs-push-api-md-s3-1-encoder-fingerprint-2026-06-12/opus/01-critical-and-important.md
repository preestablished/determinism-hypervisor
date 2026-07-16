# Critical and Important Issues

## Critical

**None.**

This is a docs-only artifact that will be applied upstream verbatim, so the bar for
"critical" is a misquote or a wrong claim that would ship a bad spec. I verified every
old/new text and every authority claim and found no such defect.

Verification performed (all PASS):

- **#1** (`API.md §3.1` reserved → `encoder_fingerprint`+`reserved`): "Old" matches
  `d55ecc3:.agents/docs/.../API.md` line 520 byte-for-byte; "New" matches the two
  added rows in `git show c7e2b1a`. Offsets sound: `[240..248)` fingerprint (8B) +
  `[248..256)` reserved (8B) = the original 16B span.
- **#2** (`SlotState.PAUSED` → `PAUSED_S`): "Old"/"New" match `git show 8a22a56`
  exactly, including the C++-scoping comment block.
- **#7** (`ENTR` versioned, v1=56B / v2=72B): "Old"/"New" match `git show efa286f`
  exactly. v2 byte math checks: 56 + (buf_gpa u64 8 + len u32 4 + status u32 4 = 16)
  = 72. ✓
- **#9** (dirty-ring chaos 512 → 1024, both ARCH §8.2 and IMPL-PLAN M4): "Old" texts
  match `d55ecc3` (ARCH line 656, IMPL-PLAN line 79) exactly; "New" texts match
  `git show d94c605` exactly.
- **#10** (`NETL` regs-only, 36 bytes): "Old" matches `d55ecc3` line 619; "New"
  matches `git show 84e99cc`. Register math: 8+4+4+8+4+4+4 = 36. ✓ Cross-checked
  against `crates/dh-devices/src/net.rs` lines 54-78 — same 7 fields, same order, same
  "registers only / buffers no frames" contract.
- **#3** (`EVTC` row): authority `crates/dh-devices/src/detchannel.rs` confirms
  `EVTC_LEN = 4+4+4+5+5+1+16 = 39`, `EVTC_VERSION = 1`. The ledger's byte layout
  matches `restore()` offsets exactly: init_lo/hi/status @0/4/8; inject_iseq flag@12 +
  u32@13 (12..17); last_quiesce_ack flag@17 + u32@18 (17..22); channel flag@22 +
  gpa u64@23 + ring_c@31 + ring_i@35 (22..39). "Old" matches `d55ecc3` line 617.
- **#4** (lifecycle `Running → Frozen` should be `Paused → Frozen`): "Old" matches
  current ARCH lines 743-744. Authority confirmed by ARCH §8.4 (line ~700): "a paused
  parent with live children is `Frozen{children:n}`" — fork requires a paused parent.
- **#5** (`KVM_MEM_LOG_DIRTY_PAGES` required on both paths): both "Old" quotes match
  current ARCH (§2.2 lines 118-121; §8.2 lines 664-667). Authority confirmed by
  `crates/dh-vmm/src/dirty.rs` `enable_dirty_logging` (line 181) which sets the flag
  unconditionally, and the module doc states "Without the flag the ring stays empty"
  — the exact A/B empiric the ledger cites.
- **#6** (TSC aligned once at restore, not every entry): "Old" matches current ARCH
  §4 defense-4 lines 367-373. Authority `docs/decisions/tsc-alignment.md` confirms
  KVM_VCPU_TSC_OFFSET set once at restore, `guest_tsc = host_tsc + offset`, decided
  2026-06-10.
- **#8** (state-hash vCPU preimage ≠ DHSNAP VCPU section bytes): "Old" matches current
  ARCH §8.5 line 723. Authority `crates/dh-worker/src/snapshot_engine.rs` module doc
  (lines 15-21) states verbatim: artifacts STAY SEPARATE (option b), hash keeps
  field-selective padding-excluded `canonical_vcpu_blob`, section is raw-struct
  restore codec, folding would re-import the iteration-69 reserved-byte hazard.

Internal consistency also PASS: all five cited amendment commits exist and touch the
claimed docs (`git log main -- .agents/docs/`); every iteration↔commit pair matches
(#1/iter61/c7e2b1a, #2/iter63/8a22a56, #7/iter71/efa286f, #9/iter84/d94c605,
#10/iter85/84e99cc); all referenced beads (veu, 4ld, bcb, 28i, mmv) exist; the
intro's "five amended / five upstream-only" split matches the actual entries; code
fences are balanced (48, even).

## Important

**None blocking.**

The only borderline item worth flagging for the human applier (not a defect in the
ledger, but a quote-matching caveat for #6):

- **Provenance/quote-span nuance — #6** (`docs/upstream-divergences.md:247-256`,
  "Old" block): the quoted "Old" anchor stops mid-sentence at `prefer adjusting the
  **TSC offset**`, but the upstream caveat sentence actually continues
  `(KVM_VCPU_TSC_CTRL offset attribute) over MSR value writes; benchmark both in M3
  before freezing the mechanism.` (current ARCH lines 372-373). The "Proposed new"
  block replaces a *larger* span than the quoted "Old" shows (it drops the
  "benchmark both in M3" clause entirely). This is technically correct as a rewrite,
  but the old→new diff is not strictly quote-matchable end-to-end the way #1, #2,
  #7, #9, #10 are. **Suggested fix:** extend the "Old" quote to the end of item 4
  (through `before freezing the mechanism.`) so the applier can match-and-replace the
  whole item rather than guessing where the replacement ends. Severity: low — the
  proposed text is accurate; this is about making the patch mechanically applicable.
