## Action Items

### Critical
- None.

### Important
- [ ] Guard `pin_current_thread` against core ids outside `cpu_set_t` capacity.
- [ ] Reject duplicate or overlapping slot core maps.
- [ ] Make UDS cleanup remove only stale socket files.

### Suggestions
- [ ] Avoid long-running runtime-table closures under the global mutex.
- [ ] Make lease TTL arithmetic overflow-safe.
- [ ] Keep lifecycle RPC success blocked until runtime ownership and lossless `MachineConfig` wire mapping are resolved.
