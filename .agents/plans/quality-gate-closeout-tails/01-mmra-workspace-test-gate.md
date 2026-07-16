# Package 01 — `mmra`: workspace test gate (`ops.rs` RestoreSnapshotRequest.baseline)

Bead: `determinism-hypervisor-mmra`
Filed: `.agents/requests/lease-semantics-doc-and-orphan-slot-warn/04-resolution.md`
(2026-07-10): "`cargo test --workspace --all-targets` stops on a pre-existing
missing `RestoreSnapshotRequest.baseline` field in `tools/dh-cli/src/ops.rs:178`".

## Expected outcome (read first)

**Likely already fixed.** At HEAD `776a80f`, `tools/dh-cli/src/ops.rs:178-182`
reads:

```rust
.restore_snapshot(proto::RestoreSnapshotRequest {
    snapshot: Some(proto::SnapshotRef { hash: snapshot }),
    entropy_seed,
    baseline: None,
})
```

Commit `dd49ebf` ("Restore hypervisor CI compatibility", 2026-07-11 — one day
after the bead was filed) added exactly one line to `ops.rs`. The most probable
disposition is: gate is green, close the bead with evidence. Do not "fix"
anything until step 1 proves otherwise.

## Step 1 — Verify (reproduce or refute)

On a Linux host with the sibling checkouts (`../control-plane`, `../guest-sdk`,
`../snapshot-store`) present and `nasm` installed, run each as a separate,
individually-checked command:

```bash
cargo check --workspace --all-targets    # fast signal first
cargo test --workspace --all-targets
```

- If `cargo check` already passes, the `mmra` compile failure no longer
  reproduces. Still run the full test command — the bead's bar was the test
  gate, and closing needs the whole gate green, not just the one file
  compiling.
- KVM tests self-skip without `/dev/kvm`; that is fine for this gate (CI's
  hosted lanes have no KVM either). Record skip counts honestly.

## Step 2a — If green (expected)

Close with evidence:

```bash
bd close determinism-hypervisor-mmra -r "No longer reproduces at HEAD <sha>: ops.rs:181 has baseline: None since dd49ebf (2026-07-11). cargo test --workspace --all-targets green: <N passed / M skipped> on <host>. No code change."
```

Note in the closure that no source was touched (hence no determinism-suite
obligation).

## Step 2b — If still red

Only if the failure is the cited one (a `RestoreSnapshotRequest` construction
missing `baseline`), fix it minimally: add `baseline: None` to the offending
struct literal(s). All struct-literal construction sites (audited at HEAD)
carry the field — across `tools/dh-cli`, `crates/dh-worker` tests,
`crates/dh-worker/src/service.rs`, `crates/dh-worker/src/m9_handoff.rs`.

If the failure is something else entirely, **stop and re-scope**: do not fix
unrelated test failures under this bead. Annotate `mmra` with what actually
failed and file a new bead for it (`bd create` per conventions).

`tools/dh-cli` is operator tooling, not the execution path — a `baseline: None`
addition needs no determinism-suite rerun. Say so in the commit message.

## Acceptance

All must pass on the Linux gate host (separate commands, each checked):

```bash
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
# CI-shaped fmt (see 00-overview for why not --all):
members=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[].name')
cargo fmt --check $(printf -- '--package %s ' $members)
```

(The clippy/fmt lines double as the verification runs for packages 02/03 — one
gate sweep dispositions all three beads.)

## Failure guidance

- **Compile error in a sibling crate** (`determinism-proto`, `detguest-*`,
  `snapstore-*`): the workspace tracks sibling HEAD with no rev pinning (per
  `ci.yaml` comments). Update the sibling checkout to its current main and
  retry; if still red, the break is upstream — annotate the bead, do not patch
  siblings from this repo.
- **`nasm` missing**: install it; `tests/nanokernel` build.rs needs it.
- **Flaky/timing test failure**: rerun once; if it repeats, it is a real
  finding — file a separate bead, do not bury it in `mmra`'s closure.
- **No Linux gate host reachable**: follow 00-overview's "Where To Execute" —
  lean on the HEAD CI run as first-line evidence, or record advisory macOS
  results in the plan dir and stop without closing the bead.
