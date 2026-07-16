# Suggestions (non-blocking)

## S1 — Add a cheap full-range wide-margin spot-check to the suite

The shipped test's wide-spread (8192-vs-128) margin-independence evidence is confined to the 100 smallest targets (the sorted prefix). I verified independence holds at 128x spread over the *full* range by scratch experiment, but that evidence is not captured in CI. A cheap, decisive addition: a third small test that lands ~20 fixed targets *spanning the whole range* (e.g. 1000 … 98_999_999) at margins {8192/1024, 64/64} across two boots and asserts tuple equality. ~2s runtime (measured). This bakes the strongest form of the §3.2 contract into the suite at negligible cost, rather than relying on the prefix happening to be small targets.

Concretely it caught nothing wrong — it would be a regression guard against a future change that makes landing subtly margin-dependent only at large counts (where the prefix would never notice).

## S2 — `Boundary.rcx` doc-comment vs its role in the equality assertion

`crates/dh-vmm/src/boundary.rs:51-52` documents `rcx` as "DIAGNOSTICS ONLY (REP progress snapshot) — the canonical boundary identity is (icount, rip)." But `#[derive(PartialEq)]` includes `rcx`, and the test's `assert_eq!(first, second)` therefore makes RCX part of the cross-boot identity. This is *correct and desirable* (RCX is deterministic, so it's a free stronger check), but the "diagnostics only" wording could mislead a reader into believing RCX is excluded from boundary identity. Consider softening to "diagnostics-primary; included in PartialEq because it is deterministic and a free extra equality check." Comment-only.

## S3 — CI lane scheduling budget

The `kvm-intel` lane runs `cargo test --workspace` (`.github/workflows/ci.yaml:108`), i.e. the entire battery in one invocation. This iteration adds ~71s (lab) / 93s (this box) of new live test time on top of the existing heavy hardware tests (regression 1e9 ≈ 92s, skid/gate ≈ 32s, m1 acceptance, counting, timer, plus `cargo build --workspace`). A back-of-envelope total is plausibly 5–7 min of live KVM time. That is fine today, but the per-iteration acceptance tests are accreting; if the lane crosses ~8 min consider either (a) gating the 10k-target leg behind an env flag for PR runs and full only on `main`/nightly, or (b) reducing `LANDING_TARGETS` for the PR path. Suggestion only — do not act unless the lane is actually slow.

## S4 — One-line comment on the floor's role for the RCX detector

`TARGET_FLOOR = 1000` is documented generically ("targets stay above this"). Since the floor is what guarantees no target lands before the first `mov rcx,64` (and thus that the `rcx ∈ {64,0}` assert is meaningful — see `01`), a one-line note at the floor or at the assert ("floor > one rep_loop iteration so RCX is always controlled before any landing") would protect a future editor who lowers the floor. (Belt-and-suspenders: I measured RCX=0 at entry anyway, so lowering it would not actually break on this box — but the comment documents the intended invariant.)

## S5 — Skid outlier note

The 50k-sample skid run showed one sample at 39 (all others ≤ 31). Not a problem (margin 128 alert threshold is 64; 39 < 64 with 1.6x headroom, and overshoot needs skid > 128), but if the skid gate's recorded "measured max 31" is quoted as a hard fact anywhere, note that a rare 39 exists in the tail. The margins chosen still have ample headroom.
