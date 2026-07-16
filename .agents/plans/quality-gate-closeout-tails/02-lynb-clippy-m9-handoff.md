# Package 02 — `lynb`: clippy findings in `m9_handoff.rs`

Bead: `determinism-hypervisor-lynb`
Filed: `.agents/requests/lease-semantics-doc-and-orphan-slot-warn/04-resolution.md`
(2026-07-10): "three pre-existing `unnecessary_lazy_evaluations` findings in
`crates/dh-worker/src/m9_handoff.rs:1392-1406`".

## Expected outcome (read first)

**Likely already fixed.** Commit `2bca5d8` ("Fix hypervisor strict CI checks",
2026-07-11) contains exactly the three fixes at those lines — each
`.parent().unwrap_or_else(|| args.<x>.as_path())` became
`.parent().unwrap_or(args.<x>.as_path())` (snapstore UDS parent, handoff env
parent, snapstore config parent, inside `validate_created_private_layout`).
`grep -n unwrap_or_else crates/dh-worker/src/m9_handoff.rs` returns nothing at
HEAD. Expect to verify-and-close.

## Step 1 — Verify

On the Linux gate host (same session as package 01 is fine):

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

This is the CI bar verbatim (`ci.yaml`, both host lanes). Note: when the bead
was filed, clippy "reached the same CLI error" as `mmra` first — so if package
01 is somehow still red, clippy will be blocked behind it; do package 01 first.

## Step 2a — If clean (expected)

```bash
bd close determinism-hypervisor-lynb -r "No longer reproduces at HEAD <sha>: the three unnecessary_lazy_evaluations at m9_handoff.rs:1392-1406 were fixed by 2bca5d8 (2026-07-11). cargo clippy --workspace --all-targets -- -D warnings clean on <host>. No code change."
```

## Step 2b — If clippy is still red

Fix **only** lints the run actually cites, and only in the cited locations.
Rules:

- Mechanical conversions only (e.g. `unwrap_or_else` → `unwrap_or`,
  redundant clone, needless borrow). No mass refactor, no `#[allow]` unless a
  lint is a true false-positive — and then the `#[allow]` gets a one-line
  justification comment.
- If a cited lint sits in execution-path code (anything under
  `crates/dh-vmm/src/` or the runtime paths of `crates/dh-worker/src/`), the
  fix must be semantics-preserving and you must re-run the determinism suite
  before closing (see Acceptance). `m9_handoff.rs` itself is
  provisioning/handoff tooling, not the guest execution path.
- If clippy reports something non-mechanical (a real logic smell), stop:
  annotate `lynb`, file a separate bead, don't improvise a behavior change
  under a hygiene bead.

## Acceptance

```bash
cargo clippy --workspace --all-targets -- -D warnings   # exits 0
cargo test --workspace --all-targets                    # still green (no regression from lint fixes)
```

Plus, only if an execution-path file was modified in 2b:

```bash
cargo test -p determinism-tests   # on the KVM reference host
```

Expected for the verify-and-close path: no source change, so the determinism
clause is vacuous — state that explicitly in the closure.

## Failure guidance

- **New lints from a newer stable clippy** than the one that ran on
  2026-07-11: they are still in scope for the CI bar (CI tracks stable). Fix
  mechanically per 2b, but list them in the bead note as "new since filing" so
  the closure is honest about scope growth.
- **Lints inside sibling path-dep crates**: out of scope — CI clippy runs with
  `--workspace`, which covers only this repo's members. If sibling code trips
  the gate, that indicates a workspace-membership problem; annotate and stop.
- **No Linux gate host reachable**: follow 00-overview's "Where To Execute" —
  the HEAD CI run's clippy lane (`--all-targets`) is first-line evidence;
  otherwise record advisory macOS results in the plan dir and stop without
  closing the bead.
