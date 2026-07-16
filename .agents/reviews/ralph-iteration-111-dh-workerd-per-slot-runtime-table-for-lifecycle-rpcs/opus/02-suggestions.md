# Suggestions

## `crates/dh-worker/src/runtime.rs:82`

`insert_many` uses `seen.contains` inside the validation loop. Slot counts are probably small, but the all-or-nothing path is easier to read and remains linear if duplicate tracking uses a `HashSet`.

```rust
let mut seen = std::collections::HashSet::with_capacity(runtimes.len());
for (slot_id, _) in &runtimes {
    let entry = slots
        .get(*slot_id as usize)
        .ok_or(RuntimeError::NoSuchSlot(*slot_id))?;
    if entry.is_some() || !seen.insert(*slot_id) {
        return Err(RuntimeError::Occupied { slot_id: *slot_id });
    }
}
```

## `crates/dh-worker/src/service.rs:366`

`rollback_lifecycle_leases` is doing subtle transaction repair across two independent tables. Consider extracting a small lifecycle transaction helper that records which leases have been manager-destroyed and always includes the original failure in the rollback failure message. That would make later RFV wiring less likely to accidentally mask the engine error or restore the wrong subset.

```rust
fn rollback_or_internal(
    method: &'static str,
    original: Status,
    rollback: Result<(), Status>,
) -> Status {
    match rollback {
        Ok(()) => original,
        Err(rollback) => Status::internal(format!(
            "{method} failed with {}; rollback also failed with {}",
            original.message(),
            rollback.message()
        )),
    }
}
```

## `crates/dh-worker/src/service.rs:483`

`build_runtimes` receives `&WorkerRuntimeTable`, which gives future fork-engine code full table authority while the lifecycle helper is trying to preserve table/manager consistency. If the fork engine only needs to inspect the parent runtime, prefer a narrower purpose-built interface or at least document that the closure must not insert, take, or mutate lifecycle ownership.

```rust
pub(crate) async fn install_forked_runtimes(
    &self,
    parent: Lease,
    count: usize,
    build_runtimes: impl FnOnce(&WorkerRuntimeTable, &[Lease]) -> Result<Vec<SlotRuntime>, Status>
        + Send
        + 'static,
) -> Result<Vec<Lease>, Status> {
    // Contract: build_runtimes may inspect existing parent runtime state, but
    // must not change runtime-table ownership. This helper owns publication.
}
```

## `crates/dh-worker/src/service.rs:329`

The branch adds runtime-specific status mapping but only tests the slot-manager mapping. Add a small unit test that decodes the `ErrorDetail` for `RuntimeError::Empty`, `Occupied`, and `NoSuchSlot` so the API codes remain pinned.

```rust
#[test]
fn runtime_errors_map_to_api_status_details() {
    let status = runtime_error_to_status(RuntimeError::Empty { slot_id: 7 });
    assert_eq!(status.code(), tonic::Code::FailedPrecondition);
    let detail = proto::ErrorDetail::decode(status.details()).unwrap();
    assert_eq!(detail.slot_id, 7);
    assert_eq!(detail.code, "runtime_missing");
}
```
