# Positive Notes

- **nq5 achieves true preimage unity, verified empirically.** `encode_into` is
  now the SOLE leaf serializer; a tree-wide grep for any second inline
  `function.to_le_bytes()` / `e.eax.to_le_bytes()` encoding returns nothing. Both
  `MachineConfig::canonical_encode` (`config.rs:226`) and `cpuid_table_hash`
  (`cpuid.rs:160`, via `cpuid_leaves_hash` over `to_leaves`) route through the
  one function. This is exactly the "never fork on representation" the bead
  demanded.

- **Hash continuity preserved across the refactor — confirmed byte-for-byte.**
  The old `cpuid_table_hash` already serialized `function, index, flags,
  eax..edx` in that order; `encode_into` uses the identical order, so the masked
  table hash is unchanged. Live `dh-cli cpuid-diff` produced
  `f19610e179617f2c8f103d1bf2d6791ffb63b3d4876d254b81bb16033fb4738e`, matching the
  committed `docs/ops/cpuid-diff-infra-control.txt` exactly. (Continuity isn't
  required pre-M4, but getting it for free while still unifying the preimage is
  the best outcome.)

- **`to_leaves` sort is correctly stable.** `sort_by_key((function, index))` is
  documented-stable, so duplicate-key entries (were they to occur) retain KVM's
  relative order deterministically. The determinism is real, not accidental.

- **Golden canonical-bytes test consciously and correctly updated.** The +4
  bytes/leaf (`flags`) is reflected in both leaves of `golden_canonical_bytes`
  (explicit `// flags (bead nq5)` for the f=0 leaf, and the `[0u8; 24]` widening
  for the f=1 leaf). The test's own doc-comment correctly frames this as a
  deliberate preimage change. It passes.

- **4ld kind numbering is clean.** `KIND_ENCODER_FP = 0x46` slots between
  FRAME_MARK (0x45) and END (0x7F) with no collision; `record()`
  (`dhilog.rs:315`) whitelists no kinds, and no in-tree reader/replayer rejects
  unknown kinds, so introduction is safe. `RFLAG_AUX` is set correctly, so a
  minimal replayer that skips AUX records ignores it as intended, and
  `has_aux`/`FLAG_HAS_AUX` is set on seal.

- **Encoder fingerprint is genuinely pure.** `wire_encoder_fingerprint()` takes
  no inputs and depends only on the encoder; the digest is over canonical
  encodings, so it is reproducible across processes (verified by running
  m1_acceptance repeatedly and by the dedicated purity test).

- **Determinism is intact end-to-end.** The m1_acceptance test's built-in
  cold-boot run-twice bit-identical comparison passes WITH the new 6th record;
  re-running the whole test across separate process invocations is stable; the
  full workspace battery is green; clippy is clean on BOTH x86_64 and aarch64.
  The +1 record count change (5 -> 6) is matched by an accurate, well-commented
  assertion update.

- **Excellent doc-comments.** The new code carries clear, honest rationale —
  especially the HEAD-wins sibling-dep skew explanation on both
  `wire_encoder_fingerprint` and `LogWriter::encoder_fingerprint`, and the
  "machine behavior / one preimage" note on `CpuidLeaf::flags`. The "re-attach
  re-emits, each record truthful for its encoder" comment is precise (its only
  gap is the restore case, S-1).

- **`non_exhaustive` discipline holds.** `wire_view`'s `_ => return None`
  correctly counts-not-drops unknown future SDK variants, so an out-of-build
  payload can't be silently mis-digested — it just isn't fingerprinted (which is
  the right conservative behavior).
