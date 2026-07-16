# Suggestions

### Add a short comment explaining the unhashed canonical-input boundary

- File: `crates/dh-worker/tests/m5_net_loopback.rs:156-158`

What to change and why: `hash_final_stop: false` is the key behavioral change in the M5 fix, and it is easy to misread as weakening the hash assertion. A short comment would preserve the replay-contract reasoning for the next maintainer.

Suggested snippet:

```rust
            RunOptions {
                // This quantum stops only to land a canonical NET_RX; replay
                // uses the same unhashed intermediate boundary before applying
                // the record.
                hash_final_stop: false,
                ..RunOptions::default()
            },
```

### Echo resolved M7 canary settings in the workflow log

- File: `.github/workflows/nightly-drift.yaml:147-148`

What to change and why: Scheduled runs and manual dispatches both flow through the same job, and the job allows operator overrides. Printing the resolved job count and core set before the test makes later incident triage easier without changing behavior.

Suggested snippet:

```yaml
      - name: show M7 canary settings
        run: |
          echo "DH_M7_ACCEPT_JOBS=${DH_M7_ACCEPT_JOBS}"
          echo "DH_M7_ACCEPT_SLOT_CORES=${DH_M7_ACCEPT_SLOT_CORES}"
```

### Clarify the full M7 acceptance command for the four-slot lab runner

- File: `docs/ops/test-partitioning.md:58`

What to change and why: The harness default slot core list is `2-65`, while the documented lab runner has isolated slot cores `2-5`. The new nightly row is explicit about `2-5`; the adjacent full acceptance row would be less surprising if it also showed the lab-box override for the 1000-child operator run.

Suggested snippet:

```markdown
| M7 fork/VerifyReplay acceptance | `DH_M7_ACCEPT_JOBS=1000 DH_M7_ACCEPT_SLOT_CORES=2-5 cargo test -p dh-worker --test m7_fork_verify --release -- --ignored --nocapture` | long; lab-box four-slot run |
```
