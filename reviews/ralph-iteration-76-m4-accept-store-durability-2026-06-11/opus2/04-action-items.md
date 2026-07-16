# Action Items

### Critical

None.

### Important

- [ ] **Soften the module-doc durability claim to match what an in-process
  restart can prove.** In `crates/dh-worker/tests/store_durability.rs:8-15`,
  the "acked before persisting … every ref the engine ever returned was a lie"
  framing over-claims: because instance 2 runs in the same OS process, its
  recovery scan reads through the same kernel page cache instance 1 populated, so
  the test would pass even against a store that acked `put_snapshot` *before*
  fsync. Rewrite the claim to say this proves the fresh instance rebuilds its
  in-memory index and resolves the full chain from the on-disk layout (re-open
  fidelity), and add one line pointing crash/fsync durability at the store's
  `failpoints`-gated unit tests (`manifest-fsync`, `pack-fdatasync`,
  `put_snapshot_durable_after_reopen`, `crash_during_rotation_*`). Doc-only; no
  behavior change. (See 01-critical-and-important.md I1.)

### Suggestions

- [ ] **Retune the `ref_b2 == ref_b1` comment** (`store_durability.rs:221-245`):
  the assertion is near-tautological given the byte-level restore equality
  already asserted at lines 200-219 plus take-snapshot determinism. Keep it as a
  cheap end-to-end backstop, but trim the "strongest form / content-addressed
  identity carried entirely by the persisted bytes" lead so the actual
  load-bearing check (the `delta.snapshot_ref` restore at line 186) is not
  understated. (S1.)

- [ ] **Reconsider whether the same-instance reference leg earns its length**
  (`store_durability.rs:141-172`): `slot_b1`/`ref_b1` exist almost solely to feed
  the near-tautological S1 assertion; the genuinely useful reuse is
  `outcome2.chain.value() == outcome1.chain.value()` (line 219). If S1 is trimmed,
  evaluate replacing the ~32-line reference leg with a chain comparison against a
  precomputed constant. Optional. (S2.)

- [ ] **Add a comment protecting the live-`slot_a` invariant**
  (`store_durability.rs:71`): the post-restart vCPU/byte comparisons
  (lines 200-218) silently depend on `slot_a` outliving instance 1's teardown.
  Note that it is kept live to the end so a future edit does not drop it early.
  (S3.)

- [ ] **Capture the last connect error in the readiness probe**
  (`common/mod.rs:68-78`): replace `client.expect("store ready")` with a path
  that surfaces the final `Err(_)` (currently discarded at line 75) in the panic
  message, so a future bind failure is self-diagnosing instead of an opaque
  panic. Zero runtime cost. (S4.)

- [ ] **(Awareness only, no change)** `MEM` and the dirtied/root GPAs were
  verified to fit: 2 MiB = 512 pages, and `0x9000` (36 KiB) < 2 MiB. The
  `// 512 pages` comment is accurate. (S5.)
