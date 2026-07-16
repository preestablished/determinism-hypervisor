# Suggestions

### S-1 — Permissive flag-byte decoding silently coerces corruption to `None`

**File:** `crates/dh-devices/src/detchannel.rs` — `restore()`.

The three presence flags are decoded as exact equality with `1`:

```rust
let inject_iseq      = (bytes[12] == 1).then(|| u32_at(13));
let last_quiesce_ack = (bytes[17] == 1).then(|| u32_at(18));
let (channel, ...)   = if bytes[22] == 1 { ... } else { (None, None, None) };
```

The writer only ever emits `0` or `1` (`is_some() as u8` / a literal `1`/`0`), so
any other value can only arise from a **corrupt or mismatched-version section**.
The current code treats a flag byte of, say, `2` as "absent" rather than refusing
the restore. For the `channel` flag this is partly masked — a corrupt section that
flips `bytes[22]` from `1` to `2` would drop the channel entirely (and tests cover
the *bad-header-at-GPA* refusal, not this one) — but for `inject_iseq` and
`last_quiesce_ack` a corrupted flag silently discards a latch with no signal.

This is consistent with the version+length gate philosophy ("malformed → refuse"),
so the more defensive form is to refuse on an out-of-range flag:

```rust
fn flag(b: u8) -> Result<bool, crate::RestoreError> {
    match b { 0 => Ok(false), 1 => Ok(true), _ => Err(crate::RestoreError) }
}
```

Judged a **Suggestion** rather than Important: a corrupt section that also passes
the version+length gate is an unlikely, narrow failure mode, and "silently absent"
is a safe (non-attaching, non-garbage) degradation. But strict refusal matches the
"refuse loudly" posture the bad-header path already takes and costs little.

### S-2 — `EVTC_LEN` / `EVTC_VERSION` as inherent consts force turbofish at call sites

**File:** `crates/dh-devices/src/detchannel.rs`.

`EVTC_LEN` and `EVTC_VERSION` are associated consts on the generic
`DetChannelHost<M, P>`, so callers that don't have a value in hand must write the
full turbofish — visible in the tests themselves:

```rust
DetChannelHost::<SharedMem, LogFaultPlan>::EVTC_LEN
DetChannelHost::<SharedMem, LogFaultPlan>::EVTC_VERSION
```

The values are independent of `M` and `P` (a fixed wire layout), so this is pure
ergonomic noise. Consider free module-level consts:

```rust
pub const EVTC_LEN: usize = 4 + 4 + 4 + 5 + 5 + 1 + 16;
pub const EVTC_VERSION: u16 = 1;
```

and have the methods reference them. Minor; the dh-snapshot framing layer will be
the real consumer, and it will appreciate not threading `M, P` to read a length.

### S-3 — `restore()` does not reset observability metrics

**File:** `crates/dh-devices/src/detchannel.rs` — `restore()`.

`restore()` only ever *increments* `metrics.manifest_read_failures` (on a failed
re-read) and leaves all other counters as they were on the receiving host. In the
intended flow `restore()` runs on a freshly-constructed host (metrics all zero), so
this is correct. But if `restore()` were ever called on a reused host instance, the
metrics would carry pre-restore values plus the new failure — a subtle
observability footgun. Either document "restore assumes a fresh host (metrics
zeroed by `new`)" alongside the existing precondition note, or zero the metrics at
the top of the validated-assignment block. Low priority; the `inject_in_without_out`
and drain counters are genuinely boot/observability-scoped and correctly excluded
from the section either way.
