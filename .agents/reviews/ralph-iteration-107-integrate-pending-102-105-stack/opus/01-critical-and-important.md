Critical: none found.

Important:

- `crates/dh-worker/src/service.rs`: `DestroyVm` released slot-manager state before removing the runtime table entry. That exposed a window where introspection could report the slot free while the old runtime still existed, and a failed runtime removal could return an error after publishing the slot as Empty.

Suggested fix:

```rust
manager.check_destroy(&lease, now_ms)?;
let runtime = runtimes.take(lease.slot_id)?;
manager.destroy(&lease, now_ms)?;
```

The fix should restore the runtime if the final manager commit unexpectedly fails.
