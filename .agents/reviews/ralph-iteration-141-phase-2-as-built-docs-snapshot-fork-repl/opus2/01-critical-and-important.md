# Critical And Important Issues

## Critical

No critical issues found.

## Important

### IMPORTANT: DHILOG row cites the wrong freeze anchors for NET_RX bounds and lineage

Path: `docs/phase-2-exit-gate.md:67`

Description: The DHILOG table row says the header, record kinds, encoder fingerprint, END byte, NET_RX lower bound, and lineage assumptions are all "pinned by checked-in bytes." The golden fixtures do pin the header, writer-emitted record shapes, encoder fingerprint, and END byte, but the NET_RX lower-bound tightening is enforced by reader validation, and lineage/splice assumptions are enforced by `crates/dh-inputlog/src/splice.rs` tests. Leaving this as-is makes the exit-gate record point future maintainers at the wrong evidence when they audit DHILOG compatibility.

Suggested fix snippet:

```markdown
| DHILOG v1.0 | `crates/dh-inputlog/tests/golden.rs`; `crates/dh-inputlog/tests/fixtures/v1_minimal.dhilog`, `v1_kitchen_sink.dhilog`; `crates/dh-inputlog/src/splice.rs` | Header, writer-emitted record kinds, encoder fingerprint field, and END stop-reason byte are pinned by checked-in bytes. NET_RX lower-bound validation is covered by reader validation, and lineage splicing is covered by the splice tests without changing the frozen DHILOG v1 record format. |
```

### IMPORTANT: M7 evidence overstates what the local commands exercised

Path: `docs/phase-2-exit-gate.md:103`

Description: The gate says "M7 fork/VerifyReplay harness remains runnable" but the listed commands are `cargo test ... -- --nocapture`, which only runs non-ignored coverage in that test target, and `cargo test ... --release --no-run`, which compiles but does not execute the ignored M7 fork/VerifyReplay harness. The next row correctly says the full slot-core gates remain operator-run, so this should be reworded to avoid implying local execution of the acceptance path.

Suggested fix snippet:

```markdown
| 6 | M7 fork/VerifyReplay harness remains buildable and discoverable in this constrained shell | `cargo test -p dh-worker --test m7_fork_verify -- --nocapture` covers non-ignored helper tests, and `cargo test -p dh-worker --test m7_fork_verify --release --no-run` compiles the ignored acceptance target on 2026-06-16 |
```

If you want to keep the skip-mode guard evidence in this row, include the exact skipped command and keep the "does not replace the 2-5 slot-core operator run" wording from row 7.
