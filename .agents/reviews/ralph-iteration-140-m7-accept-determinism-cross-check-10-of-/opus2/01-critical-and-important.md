# Critical And Important

## Issue 1

Severity: Important

File/line: `crates/dh-worker/tests/m7_fork_verify.rs:624`

Description: The cross-slot acceptance test always forks exactly two same-seed children. Because the slot manager allocates the first empty slots and `run_child` destroys both children before the next sampled index, a normal `DH_M7_ACCEPT_SLOT_CORES=2-5` run will keep reusing the same two child slots for every check. This proves that two concurrent same-seed twins on that slot pair agree; it does not prove the stronger rerun-on-different-slot property implied by the test/docs, and it leaves additional child slots unexercised. A slot-local bug on the third available child slot, or a lifecycle bug that appears only after rotating allocation pressure, could still pass this gate.

Suggested fix snippet:

```rust
async fn cross_check_child_on_distinct_slots(
    svc: &WorkerService,
    root_lease: &proto::Lease,
    root_snapshot: &proto::SnapshotRef,
    store: &snapstore_client::blocking::SnapstoreClient,
    index: usize,
    child_count: usize,
) -> TestResult<()> {
    let seed = child_seed(index);
    let forked = svc
        .fork(Request::new(proto::ForkRequest {
            parent: Some(root_lease.clone()),
            count: child_count as u32,
            entropy_seeds: std::iter::repeat(seed).take(child_count).collect(),
        }))
        .await
        .map_err(|e| format!("cross-slot child {index} Fork same-seed children: {e}"))?
        .into_inner()
        .children;

    if forked.len() != child_count {
        return Err(format!(
            "cross-slot child {index} Fork returned {}, expected {child_count}",
            forked.len()
        ));
    }

    let children = run_child_twins(svc, index, forked).await?;
    // Run lineage + VerifyReplay for every child, then compare every verified
    // record against the first so all available child slots participate.
    let mut verified = verify_batch(svc, root_snapshot, children).await?;
    verified.sort_by_key(|record| record.slot_id);
    let first = verified.first().ok_or_else(|| format!("cross-slot child {index} had no records"))?;
    for other in verified.iter().skip(1) {
        if first.snapshot.hash != other.snapshot.hash
            || first.state_hash != other.state_hash
            || first.input_log_id != other.input_log_id
        {
            return Err(format!(
                "cross-slot child {index} diverged between slots {} and {}",
                first.slot_id, other.slot_id
            ));
        }
    }
    Ok(())
}
```

If the intended scope is only twin equivalence on one slot pair, narrow the test name and docs so operators do not treat it as broad cross-slot rerun acceptance.

## Issue 2

Severity: Important

File/line: `crates/dh-worker/tests/m7_fork_verify.rs:640`

Description: After a successful `Fork`, several error paths can exit without destroying the newly leased children, and any panic inside the per-index loop skips the final root cleanup at line 844. The service rollback path covers failed `Fork` RPCs, but not harness assertions after the children are already published. In this lifecycle-focused acceptance test, a post-fork failure should still destroy child leases and attempt to thaw/destroy the root so that cleanup failures are visible rather than masked by the first assertion failure.

Suggested fix snippet:

```rust
let forked = /* successful Fork response */;
let result = async {
    if forked.len() != 2 {
        return Err(format!("cross-slot child {index} Fork returned {}, expected 2", forked.len()));
    }
    if forked[0].slot_id == forked[1].slot_id {
        return Err(format!(
            "cross-slot child {index} twins landed on the same slot {}",
            forked[0].slot_id
        ));
    }

    let children = run_child_twins(svc, index, forked.clone()).await?;
    // lineage, VerifyReplay, comparisons...
    Ok(())
}
.await;

if result.is_err() {
    for lease in forked {
        destroy_best_effort(svc, Some(lease)).await;
    }
}
result?;
```

The outer acceptance body should similarly preserve the first failure, call `destroy_best_effort(&svc, Some(root_lease)).await`, inspect `GetWorkerInfo` if possible, and only then panic with the original failure plus any cleanup failure.
