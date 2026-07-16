# Critical & Important Findings

## Critical

None. The layout is correct, version dispatch is sound, v1 decode is unchanged, and all
tests pass.

---

## Important

### I1 — Spec divergence (72-byte ENTR v2) is recorded only in a code comment

**Severity:** Important
**Location:** `crates/dh-snapshot/src/dhsnap.rs:363-366` (the `EntrSectionV2` doc) vs
`.agents/docs/determinism-hypervisor/API.md:618`

API.md §4 is normative and says:

> `ENTR` | ChaCha20 PRNG state, **exactly** `seed: [u8;32], stream: u64, word_pos: u128`
> **(56 bytes)** …

There is no `sec_version 2` row, and the word "exactly" forecloses a 72-byte body. This
branch makes the snapshot engine *write* a 72-byte ENTR section. That is a genuine
producer-vs-spec divergence — precisely the drift this project's golden-bytes discipline
exists to catch. Right now it is documented only in the `RESOLVED (bead 6yl)` and
`EntrSectionV2` doc comments inside `dhsnap.rs`. A reader of the spec (or the
snapshot-store team, or a future reimplementer) has no way to learn that v2 exists.

The review prompt asks whether this needs a "veu" divergence note (divergence #7). I
searched the repo: there is **no `veu` bead/ledger and no divergence-ledger file** in
this tree (`grep -rln veu` / `divergence #` returns nothing outside `dh-verify`'s
runtime "fingerprint divergence" usage). So the divergence is currently recorded
*nowhere durable*.

**Position:** This should not merge-and-forget. Take one of:

1. **(Preferred)** Update API.md §4 to make ENTR explicitly versioned: change the row to
   read "v1 (56 bytes) = PRNG state; v2 (72 bytes) = PRNG state ‖ pv-entropy regs
   `{buf_gpa u64, len u32, status u32}`", and note the engine writes v2. This makes the
   producer spec-conformant rather than divergent — the cleanest outcome, since the
   section header already carries `sec_version: u16` (line 607), so versioned ENTR is
   *within* the format, not a violation of it.
2. If the spec is owned upstream and cannot be edited here, record the divergence in
   whatever durable ledger the project uses for spec deltas (the prompt calls it "veu
   divergence #7"). If that ledger does not exist yet, file a bead to create it and link
   6yl — do not let the only record be a struct doc comment.

**Fix (option 1) snippet for API.md:618:**

```
| `ENTR` | `sec_version 1` (56 bytes): ChaCha20 PRNG state `seed:[u8;32], stream:u64, word_pos:u128` — `rand_chacha` exportable state; restore reproduces next draws bit-identically (M4 golden). `sec_version 2` (72 bytes): the v1 PRNG state followed by the pv-entropy device's MMIO regs `buf_gpa:u64, len:u32, status:u32`; the engine writes v2 so the device regs travel with the snapshot. Readers MUST decode by `sec_version`. |
```

---

### I2 — No frozen golden-bytes fixture for ENTR v2 (§4 mandates one "per version")

**Severity:** Important
**Location:** `crates/dh-snapshot/tests/golden.rs` (v2 absent) vs
`.agents/docs/determinism-hypervisor/API.md:25-26`

API.md states the rule for *every* format:

> Golden-bytes tests pin every format: a checked-in fixture file **per version** must
> parse to a checked-in debug representation, and re-serialize byte-identically.

`golden.rs` pins only `EntrSection` v1 (`golden.rs:73-77`, `:175-180`). The new v2
72-byte layout is exercised *only* by `entr_roundtrip.rs`, which **generates** the bytes
at runtime via `EntrSectionV2::encode()` and round-trips them through itself. That proves
internal self-consistency but does NOT pin the on-disk byte layout against an external
checked-in fixture — so a future accidental reordering of the v2 fields (e.g. swapping
`len` and `status`, or moving `device_regs` ahead of `word_pos`) would still pass every
test in this branch, because both the encoder and the assertion would shift together.
That is exactly the failure mode the golden discipline is designed to prevent, and v1 is
already protected this way.

**Fix:** Add a v2 fixture to `golden.rs` — a checked-in 72-byte hex/byte literal with
known field values, asserted to (a) parse to the pinned `EntrSectionV2` debug repr and
(b) re-encode byte-identically. Mirror the existing v1 ENTR block at `golden.rs:73-77` /
`:175-180`. A `blake3` pin of the full container (as the kitchen-sink fixture does) is
the established pattern here.
