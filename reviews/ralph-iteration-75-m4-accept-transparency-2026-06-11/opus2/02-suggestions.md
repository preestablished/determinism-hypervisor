# Suggestions (non-blocking)

### S1 — `H_0` is seeded with the raw seed `[7;32]`, not the real `machine_config_hash`

**File:** `crates/dh-worker/tests/m4_transparency.rs:158`
**Cross-ref:** `crates/dh-vmm/src/config.rs:237` (`config_hash()`), `restore_engine.rs:325`

`boot()` builds the chain with `StateHashChain::new(&[7; 32], &[7; 32])`. The first arg is
documented as the `machine_config_hash` (hash.rs H_0 formula), whose real value is
`config().config_hash()` — a BLAKE3 over the canonical encoding, not the raw seed material.
Because both legs use the same literal, H1==H2 still holds, and the restore leg's resumed
chain (`from_value(time.hash_chain)`) inherits the same literal base, so the test is
internally consistent and correct. But it is the only `StateHashChain::new` call in a
"real-machine" acceptance test that does not feed the genuine config hash (`m1_acceptance.rs:177`
uses `&mc_hash`). The seed literal happens to equal `BootSpec`'s `kernel_hash`/seed, which
makes it easy to mistake for "the config hash." A fully faithful gate would seed with
`cfg.config_hash().unwrap()` so H_0 matches what a real worker would compute. Low priority —
it does not affect the equality property — but it would make the test exercise the actual
H_0 preimage the production chain uses.

### S2 — Consider a parallel "uninterrupted 2e8" leg as a third reference

**File:** `crates/dh-worker/tests/m4_transparency.rs:202-206`

Both compared legs *pause/stop at 1e8* (control via two `run_more` calls; roundtrip via the
snapshot detour). Neither runs an uninterrupted 2e8. Because the epoch grid is absolute and
both legs link at identical absolute icounts (1.5e8, 2e8) regardless of where the
segment boundary falls, an uninterrupted 2e8 run *should* produce the same chain value — but
that is an assumption this test does not check. A cheap third leg (`boot()` then a single
`run_more(.., FULL)`) asserting equality to `h2` would prove the segment boundary itself is
invisible to the chain, independent of the snapshot machinery. Optional; it strengthens the
"the pause at 1e8 is not itself doing the work" guarantee. (Skip if runtime budget is tight —
this triples a ~2e8-instruction live run.)

### S3 — The `r1 == c1` assert relies on `SegmentOutcome`'s derived `PartialEq` including `state_hash`

**File:** `crates/dh-vmm/src/runctl.rs:57-70`, used at `m4_transparency.rs:215`

`SegmentOutcome` derives `PartialEq` over all fields including `state_hash: [u8;32]`, so
`assert_eq!(r1, c1)` is a full structural compare — good, that is the strong form. Worth a
half-line comment at the call site noting the assert covers the hash chain (not just
boundary/reason), so a future refactor that, say, excludes `state_hash` from the compare
does not silently weaken the gate. Trivial.

### S4 — `agenda_empty: true` is an unchecked caller attestation

**File:** `crates/dh-worker/tests/m4_transparency.rs:230`
**Cross-ref:** `snapshot_engine.rs:113-115` (engine trusts and re-checks the flag)

The test hard-codes `agenda_empty: true`. The segment that produced `r1` ran with
`injections: &[]` and `timer: None`, so the agenda genuinely is empty at the boundary, and
`take_snapshot` would reject `false` anyway. This is fine, but it is a hand-asserted
invariant; a one-line comment ("`run_more` ran with no injections/timer, so the boundary is
quiescent") documents *why* the attestation is sound rather than leaving it as a magic
`true`.

### S5 — `kvm_usable()` self-skip silently passes on machines without KVM

**File:** `crates/dh-worker/tests/m4_transparency.rs:197-200`

Consistent with `regression.rs` and every other live test in the repo, so not a defect —
but note that this milestone gate reports green (not "ignored") on any box without
`/dev/kvm`, including CI lanes that lose KVM. The repo's existing convention accepts this
(hardware-gated lane is the real gate). No change requested; flagging because a
"milestone-accept" test that no-ops to green is a known footgun class — the kvm-intel lane
must be the authority, and a CI assertion that the lane actually ran (not skipped) would be
worth confirming exists at the pipeline level.
