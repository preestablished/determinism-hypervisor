# Suggestions

1. Severity: suggestion
   Path: `crates/dh-worker/tests/m7_fork_verify.rs:624`
   Rationale: If `Fork` ever returns malformed success data, such as a wrong child count or duplicate slot IDs, `cross_check_child_on_distinct_slots` returns before best-effort destroying any returned child leases. That is already a failing service contract, but cleanup-hardening keeps acceptance failures focused on the original problem and avoids root cleanup being obscured by leaked live children.
   Suggested fix snippet:
   ```rust
   async fn destroy_many_best_effort(svc: &WorkerService, leases: Vec<proto::Lease>) {
       for lease in leases {
           destroy_best_effort(svc, Some(lease)).await;
       }
   }

   if forked.len() != 2 {
       let got = forked.len();
       destroy_many_best_effort(svc, forked).await;
       return Err(format!(
           "cross-slot child {index} Fork returned {got} children, expected 2"
       ));
   }
   if forked[0].slot_id == forked[1].slot_id {
       let slot_id = forked[0].slot_id;
       destroy_many_best_effort(svc, forked).await;
       return Err(format!(
           "cross-slot child {index} twins landed on the same slot {slot_id}"
       ));
   }
   ```

2. Severity: suggestion
   Path: `crates/dh-worker/tests/m7_fork_verify.rs:647`
   Rationale: Comparing `input_log_id` is a strong content-addressed check, but retaining the fetched log payloads and comparing them directly would make failures easier to diagnose and would document that identical replay artifacts are part of the acceptance contract.
   Suggested fix snippet:
   ```rust
   let mut logs = Vec::with_capacity(children.len());
   for child in &children {
       let log = tokio::task::block_in_place(|| fetch_log_payload(store, &child.input_log_id));
       validate_single_edge_lineage(root_snapshot, child, &log);
       logs.push((child.slot_id, log));
   }
   logs.sort_by_key(|(slot_id, _)| *slot_id);
   if logs[0].1 != logs[1].1 {
       return Err(format!(
           "cross-slot child {index} input log payloads diverged between slots {} and {}",
           logs[0].0, logs[1].0
       ));
   }
   ```

3. Severity: suggestion
   Path: `crates/dh-worker/tests/m7_fork_verify.rs:804`
   Rationale: `DH_M7_ACCEPT_JOBS` changes the sampled universe size and `DH_M7_CROSS_CHECKS` changes the sample count, but only the default command is shown in the test header and docs. A short note would make operator intent clearer when someone scales the cross-check up or down.
   Suggested fix snippet:
   ```rust
   //! Optional: DH_M7_ACCEPT_JOBS controls the sampled universe size and
   //! DH_M7_CROSS_CHECKS controls how many evenly-spaced indices are checked.
   ```

4. Severity: suggestion
   Path: `docs/ops/test-partitioning.md:61`
   Rationale: The documented `DH_M7_ACCEPT_SLOT_CORES=2-5` command provisions four slots, but the current allocator pattern uses one root slot and the first two free child slots for every sampled pair. If the operator expectation is coverage across every configured child slot, document that this is a two-slot cross-check or extend the test to rotate which child slots are occupied.
   Suggested fix snippet:
   ```markdown
   | M7 cross-slot rerun determinism | `DH_M7_ACCEPT_SLOT_CORES=2-5 cargo test -p dh-worker --test m7_fork_verify --release m7_accept_cross_slot_rerun_10_seeded_forks_identical_refs -- --ignored --nocapture` | operator-run; compares the two child slots selected by the fork allocator |
   ```
