# Suggestions

## Clarify the Full M7 Operator Command for the Four-Slot Runner

- File: `docs/ops/test-partitioning.md`
- Lines: 57-58
- What to change and why: The new nightly row correctly pins the 100-child canary to `DH_M7_ACCEPT_SLOT_CORES=2-5`, matching the `kvm-intel` box. The adjacent full M7 acceptance row still shows the default command, which defaults to `2-65` inside the test harness and will not match this four-slot runner. This is not a blocker for the nightly job, but spelling out the operator command avoids a predictable prerequisite failure when someone runs the full 1000-child acceptance gate on the documented runner.

Suggested snippet:

```markdown
| M7 fork/VerifyReplay acceptance | `DH_M7_ACCEPT_SLOT_CORES=2-5 cargo test -p dh-worker --test m7_fork_verify --release -- --ignored --nocapture` | operator-run; 1000 jobs by default |
```
