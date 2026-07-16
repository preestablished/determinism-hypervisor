# Positive notes

## P1 — nq5 closes a real pre-existing fork, not just a hypothetical one

On `main`, the two CPUID encodings had **already diverged**:

- `cpuid::cpuid_table_hash` hashed 7 fields per leaf (it included
  `e.flags.to_le_bytes()`), 28 B/leaf.
- `MachineConfig::canonical_encode` hashed 6 fields per leaf (no `flags`),
  24 B/leaf.

They were not yet wired to feed the same consumer (`cpuid_table_hash` had no
production caller; the 8jx wiring is pending), so the fork was latent — but it
was a loaded gun: the moment 8jx fed the masked table into `MachineConfig`, the
config preimage and the table hash would have disagreed on `flags`. nq5 extracts
the single `CpuidLeaf::encode_into` and routes *both* paths through it, so the
fork can never re-open. This is exactly the right fix made at the right time
(before the consumer lands). The "ONE canonical preimage" framing in the doc
comments is accurate.

## P2 — `to_leaves` is reorder-safe and decouples the kvm type from the config type

`cpuid::to_leaves` constructs `config::CpuidLeaf` with **named fields**, not
positionally, so neither a field reorder in `kvm_cpuid_entry2` nor in
`CpuidLeaf` can silently misalign the mapping — it would be a compile error.
Good defensive construction for a struct whose byte layout is a determinism
preimage.

## P3 — `flags` correctly excluded from the sort/dedup key

`validate` sorts and dedups on `(function, index)` only (config.rs:170), which
is the right uniqueness key for KVM CPUID subleaves; `flags` is a *property* of
that slot (SIGNIFICANT_INDEX etc.), not part of its identity. Including `flags`
in the encoded preimage while keeping it out of the sort key is the correct
distinction, and the doc comment explains *why* `flags` is machine behavior.

## P4 — the logging path stays panic-free by design

`DevCtx::record` latches the first `WriteError` into `log_fault` rather than
unwrapping (ctx.rs:94), so even if the new ENCODER_FP record hit
`IcountRegressed`, it degrades gracefully like every other record — devices
"need not handle them." The new `log_encoder_fingerprint` slots cleanly into
this existing discipline. (The one exception, the `.expect` inside
`wire_encoder_fingerprint`, is flagged as I3 — but the *record-writing* side is
correctly fault-latched.)

## P5 — golden-vector update is explicit and self-documenting

The config encoding test's expected tail was updated with inline comments
(`// index`, `// flags (bead nq5)`, `[0u8; 24]` for "index/flags..edx all
zero") rather than an opaque byte bump. A reader can see exactly which 4 bytes
moved and why. The +4 B/leaf change to `machine_config_hash` is intentional,
tested, and traceable.

## P6 — m1 record-count bump is reasoned, and the FP value need not be compared

The m1 5→6 change carries an accurate inline rationale (the new one-time
fingerprint at attach). The run-twice determinism gate compares the *count* of
log records, not the fingerprint *value* — which is correct: the fingerprint is
a pure function of the encoder (verified by the dedicated purity test), so it is
deterministic by construction and does not need to appear in the cross-run
tuple. Sound reasoning, correctly not over-tested.

## P7 — clean build/lint on both architectures

Full workspace tests, x86_64 clippy, and aarch64 clippy (with the cross
toolchain env) are all clean, zero warnings. The change does not perturb any
existing test, and the new device test plus the updated golden vector both pass.
