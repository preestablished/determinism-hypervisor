# Action Items

Verdict: **APPROVE**. Nothing below blocks merge.

### Critical

*(none)*

### Important

*(none)*

### Suggestions

- [ ] **S1 — Bake a full-range wide-margin spot-check into the suite.** Add a ~2s test that lands ~20 fixed targets spanning [1000, 98_999_999] at margins {8192/1024, 64/64} across two boots and asserts `Vec<Boundary>` equality. Captures in CI the strongest form of the §3.2 contract (verified by hand this review); the shipped test's wide-spread evidence is confined to the 100 smallest targets. File: a new small `#[test]` in `tests/determinism/tests/landing_precision.rs`.

- [ ] **S2 — Soften the `Boundary.rcx` doc-comment.** `crates/dh-vmm/src/boundary.rs:51-52` says "DIAGNOSTICS ONLY"; `rcx` is in fact part of `#[derive(PartialEq)]` and the cross-boot identity. Reword to note it is included in equality as a deterministic free check. Comment-only.

- [ ] **S3 — Watch the `kvm-intel` lane budget.** Lane runs `cargo test --workspace` (`.github/workflows/ci.yaml:108`); this iteration adds ~71s (lab). If the lane crosses ~8 min, gate the 10k-target leg behind an env flag for PR runs (full on `main`/nightly) or reduce `LANDING_TARGETS` on the PR path. No action unless the lane is actually slow.

- [ ] **S4 — One-line comment tying `TARGET_FLOOR` to the RCX detector.** Note at the floor or the `rcx ∈ {64,0}` assert that the floor must exceed one `rep_loop` iteration so RCX is always controlled before any landing. Protects a future editor who lowers the floor. File: `tests/determinism/tests/landing_precision.rs`.

- [ ] **S5 — (optional) Record the skid tail.** 50k samples showed one outlier at 39 (rest ≤ 31). If "max skid 31" is quoted as hard fact, note the rare 39. Margins still have ample headroom; informational only.
