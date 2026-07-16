# Suggestions

## S1 - Pin Malformed EVTC v2 Pending-Table Rejections

**File:** `crates/dh-devices/src/detchannel.rs:365`

**Rationale:** `evtc_pending_injects` now carries important format validation for v2: exact `count * 8` length, no pending entries when detached, and strictly increasing `iseq` order. The happy path and v1 compatibility are covered, but explicit negative tests would make those new compatibility constraints harder to regress.

**Suggested snippet:**

```rust
#[test]
fn evtc_restore_rejects_malformed_v2_pending_tables() {
    let host: DetChannelHost<SharedMem, LogFaultPlan> =
        DetChannelHost::new(channel_page(), LogFaultPlan::default());
    let mut detached = Vec::new();
    host.snapshot(&mut detached);

    let assert_bad = |section: &[u8], msg: &str| {
        let mut restored: DetChannelHost<SharedMem, LogFaultPlan> =
            DetChannelHost::new(channel_page(), LogFaultPlan::default());
        assert!(
            restored
                .restore(
                    section,
                    DetChannelHost::<SharedMem, LogFaultPlan>::EVTC_VERSION,
                    LogFaultPlan::default(),
                )
                .is_err(),
            "{msg}"
        );
    };

    let mut detached_pending = detached.clone();
    detached_pending[DetChannelHost::<SharedMem, LogFaultPlan>::EVTC_V1_LEN
        ..DetChannelHost::<SharedMem, LogFaultPlan>::EVTC_LEN]
        .copy_from_slice(&1u32.to_le_bytes());
    detached_pending.extend_from_slice(&7u32.to_le_bytes());
    detached_pending.extend_from_slice(&11u32.to_le_bytes());
    assert_bad(&detached_pending, "detached EVTC cannot carry pending injects");

    let mut count_mismatch = detached.clone();
    count_mismatch[DetChannelHost::<SharedMem, LogFaultPlan>::EVTC_V1_LEN
        ..DetChannelHost::<SharedMem, LogFaultPlan>::EVTC_LEN]
        .copy_from_slice(&1u32.to_le_bytes());
    assert_bad(&count_mismatch, "pending count must match entry bytes");
}
```
