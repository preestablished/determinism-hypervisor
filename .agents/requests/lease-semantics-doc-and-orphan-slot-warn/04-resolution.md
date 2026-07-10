# Resolution

Resolved 2026-07-10 on branch
`codex/lease-semantics-orphan-slot-warn`.

## Implementation

The first implementation commit is
`8488ec8fbf4e372ebc79490ef69b6f312cc8e30c` (`worker: document lease
semantics and warn on orphan signature`).

Owner artifacts:

- `.agents/docs/determinism-hypervisor/INTEGRATION.md` §1, lines 22-52;
- `.agents/docs/determinism-hypervisor/API.md` "Lease lifecycle policy",
  lines 176-189;
- `docs/decisions/lease-reclamation-activation.md`;
- corrected source header at `crates/dh-worker/src/slot_manager.rs:16-29`.

`crates/dh-worker/src/service.rs:1132-1212` now owns a structurally
token-free classifier, deterministic lowercase-hex formatter, sink-injected
emission/status core, and thin `eprintln!` adapter. It emits one `WARN:` only
after `NoFreeSlot` when the table is nonempty, every slot is `Paused`, and all
icounts equal the first row. The line includes the shared icount, every slot id,
each full base snapshot id or `none`, the legitimate-fan-out caveat, and
`rom-operator-bridge-72o`. It does not change the original
`RESOURCE_EXHAUSTED` response.

Production allocation mappings were audited at:

- `service.rs:4188-4190`: shared CreateVm/RestoreSnapshot allocation;
- `service.rs:4282-4284`: Fork preflight (`check_fork`);
- `service.rs:4288-4290`: Fork commit (`fork`);
- `service.rs:5368-5370`: VerifyReplay temporary allocation.

All other state-transition, validation, rollback, and cleanup errors retain
`slot_error_to_status` directly.

## Validation

Passing focused and scoped gates:

- `cargo test -p dh-worker --lib possible_orphan -- --nocapture`: 4 passed;
- `cargo test -p dh-worker --lib slot_errors_map_to_api_status_classes`: 1 passed;
- `cargo test -p dh-worker --lib slot_manager::tests`: 16 passed;
- `cargo test -p dh-worker --lib`: 176 passed.

The shared production core tests at `service.rs:7486-7552` prove one-line
emission, all required payload fields, absence of the actual minted lease token,
unchanged `RESOURCE_EXHAUSTED`, silence for differing icounts and a Running row,
and the recommended empty-table/non-NoFreeSlot edges.

Broader gates were run and reported truthfully:

- `cargo test --workspace --all-targets` stops on a pre-existing missing
  `RestoreSnapshotRequest.baseline` field in `tools/dh-cli/src/ops.rs:178`;
  follow-up `determinism-hypervisor-mmra` tracks it.
- `cargo clippy --workspace --all-targets -- -D warnings` reaches the same CLI
  error and three pre-existing `unnecessary_lazy_evaluations` findings in
  `crates/dh-worker/src/m9_handoff.rs:1392-1406`; follow-up
  `determinism-hypervisor-lynb` tracks the Clippy findings.
- `cargo fmt --all -- --check` was already red on clean main in
  `crates/dh-vmm/src/runctl.rs` and
  `crates/dh-worker/tests/rss_regression.rs`; follow-up
  `determinism-hypervisor-jyp4` tracks the formatting drift. Task files pass
  `git diff --check`, and `service.rs` was formatted directly with rustfmt.

Source searches confirmed `reclaim_expired` and `LeasePolicy::with_ttl` have no
production call sites, `WorkerConfig` uses `LeasePolicy::default()`, and there
is no `RenewLease`, `reclaim_session`, or disconnect hook in proto or worker
source.

## Exploration-Orchestrator Handback

Delivered 2026-07-10 by annotating the owning workflow bead
`exploration-orchestrator-w1v` (its NOTES update is the concrete delivery
reference). No sibling implementation file was edited.

The delivered note records:

1. **Trigger:** `FakeHypervisor::reclaim_session` reclaims on simulated client
   disconnect. The real worker has inactive TTL-shaped mechanics, no production
   sweep, and no disconnect hook.
2. **Sweep shape:** real `reclaim_expired` is single-pass, so
   `Running -> Faulted -> Empty` and last-child thaw followed by parent reclaim
   require later sweeps. The fake runs a fixpoint loop and empties its session
   pool in one call.
3. **Events:** the real reaper publishes the `Frozen -> Paused` parent-thaw
   transition. The fake suppresses the unfreeze event and emits only `Empty`.

The note links the INTEGRATION owner section, API lease-lifecycle subsection,
and decision record, says the fake is useful for M6 but aspirational in trigger
and one-call completion, and leaves any fake edit to the orchestrator.

## Activation Decision And Operator Notice

The accepted decision defers TTL activation, disconnect/session teardown, and
privileged tokenless reconciliation. It names the bridge dangling-intent
residual and worker restart as operator recovery. The actual non-blocking
operator/work-order escalation was delivered as human-queue decision bead
`determinism-hypervisor-h96a` on 2026-07-10; no response or sign-off is required
for deferral.

Accepted deferral creates no activation implementation bead. The three quality
gate follow-ups above are unrelated repository-local maintenance discovered by
validation.

## Beads Disposition

`determinism-hypervisor-umay` is closed by this resolution with warning, docs,
decision, tests, handback, and operator-notice evidence complete.
