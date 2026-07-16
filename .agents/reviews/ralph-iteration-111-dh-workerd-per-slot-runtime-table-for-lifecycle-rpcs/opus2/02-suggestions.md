# Suggestions

### Narrow the fork builder surface before rfv wiring

- File: `crates/dh-worker/src/service.rs:483`

`build_runtimes` receives the full `&WorkerRuntimeTable`, which gives future rfv code enough authority to `insert`, `take`, or mutate slots outside the intended parent-read/child-build flow. The current helper then has to recover from a much larger class of internal misuse. Consider passing a narrower context that exposes only the parent runtime operation the fork engine needs.

```rust
pub(crate) struct ForkBuildContext<'a> {
    runtimes: &'a WorkerRuntimeTable,
    parent: Lease,
}

impl ForkBuildContext<'_> {
    pub fn with_parent<R>(
        &self,
        f: impl FnOnce(&SlotRuntime) -> Result<R, Status>,
    ) -> Result<R, Status> {
        self.runtimes
            .with(self.parent.slot_id, f)
            .map_err(runtime_error_to_status)?
    }
}
```

### Make KVM-backed test skips visible to CI

- File: `crates/dh-worker/src/service.rs:914`

The KVM-backed service tests silently return when KVM or dirty rings are unavailable. That is convenient locally, but it can also hide the fact that CI never exercised the new runtime lifecycle hooks. A small env-gated strict mode would keep local ergonomics while making the intended CI contract explicit.

```rust
fn require_runtime_tests_available() -> bool {
    let available = runtime_tests_available();
    if !available && std::env::var_os("DH_REQUIRE_KVM_TESTS").is_some() {
        panic!("KVM runtime tests were required but unavailable");
    }
    available
}
```

### Use a set for duplicate detection in `insert_many`

- File: `crates/dh-worker/src/runtime.rs:82`

The current `seen.contains(slot_id)` scan is fine for small slot counts, but a `HashSet` makes the invariant clearer and keeps the method from quietly becoming quadratic if slot counts grow.

```rust
use std::collections::HashSet;

let mut seen = HashSet::with_capacity(runtimes.len());
for (slot_id, _) in &runtimes {
    let entry = slots
        .get(*slot_id as usize)
        .ok_or(RuntimeError::NoSuchSlot(*slot_id))?;
    if entry.is_some() || !seen.insert(*slot_id) {
        return Err(RuntimeError::Occupied { slot_id: *slot_id });
    }
}
```

### Add direct status-detail coverage for runtime errors

- File: `crates/dh-worker/src/service.rs:328`

`slot_error_to_status` has coverage for code classes, but `runtime_error_to_status` is now part of the lifecycle RPC surface and carries structured `ErrorDetail`. A small unit test that decodes details would lock the public error contract before rfv starts depending on it.
