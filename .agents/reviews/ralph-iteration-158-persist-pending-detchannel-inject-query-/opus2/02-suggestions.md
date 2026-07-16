# Suggestions

## S1 - Add malformed EVTC v2 pending-table tests

**File:** `crates/dh-devices/src/detchannel.rs:365`

**Rationale:** The validator has useful checks for v2 pending tables: exact length, zero pending count when detached, and strictly increasing `iseq`. The current tests exercise the happy path and generic bad-prefix cases, but not those new v2-specific rejection paths. Pinning them now would make the format rules harder to weaken accidentally.

**Suggested snippet:**

```rust
let mut bad = attached.clone();
bad[DetChannelHost::<SharedMem, LogFaultPlan>::EVTC_V1_LEN
    ..DetChannelHost::<SharedMem, LogFaultPlan>::EVTC_LEN]
    .copy_from_slice(&1u32.to_le_bytes());
assert_bad(&bad, "pending count without entry refuses");

let mut unsorted = attached_with_two_pending_entries();
// Second iseq <= first iseq.
unsorted[DetChannelHost::<SharedMem, LogFaultPlan>::EVTC_LEN + 8
    ..DetChannelHost::<SharedMem, LogFaultPlan>::EVTC_LEN + 12]
    .copy_from_slice(&first_iseq.to_le_bytes());
assert_bad(&unsorted, "pending entries must be sorted and unique");
```

## S2 - Avoid unchecked length arithmetic in the worker EVTC shape helper

**File:** `crates/dh-worker/tests/linux_worker_api.rs:837`

**Rationale:** This is test code over trusted snapshots, so it is not a product bug. Still, the helper is validating container shape and should not be able to panic or wrap if it ever inspects a malformed EVTC section.

**Suggested snippet:**

```rust
let entries_len = pending_count
    .checked_mul(8)
    .ok_or_else(|| "EVTC pending table length overflow".to_string())?;
let expected_len = EVTC_V2_BASE_LEN
    .checked_add(entries_len)
    .ok_or_else(|| "EVTC length overflow".to_string())?;
evtc.contents.len() == expected_len
```

## S3 - Make the pending-count cast explicit

**File:** `crates/dh-devices/src/detchannel.rs:312`

**Rationale:** A `BTreeMap<u32, _>` cannot practically reach a length that matters here, but the format field is `u32` and the code currently uses a silent `as` cast. An explicit conversion documents the invariant and turns any impossible future violation into a loud failure.

**Suggested snippet:**

```rust
let pending_count =
    u32::try_from(self.pending_injects.len()).expect("pending inject table exceeds EVTC u32 count");
out.extend_from_slice(&pending_count.to_le_bytes());
```
