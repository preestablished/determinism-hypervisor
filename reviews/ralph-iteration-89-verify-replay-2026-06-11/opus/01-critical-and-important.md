# Critical and Important findings

## Critical

None.

---

## Important

### I1 — `first_bad_epoch = at_icount / epoch_len` is nonsense for 5 of the 6 divergence kinds

**File:** `crates/dh-worker/src/verify_replay.rs:76-82`

```rust
report.push(VerifyProgress::Divergence {
    first_bad_epoch: at_icount / machine_config.epoch_len.max(1),
    at_icount,
    what,
    expected,
    got,
});
```

The wrapper applies one arithmetic formula to *every* `ReplayError::Divergence`,
but the engine emits six distinct divergence `what`s, and `at_icount` does not
mean the same thing in all of them. From `replay_engine.rs`:

| `what` (replay_engine.rs) | `at_icount` value | `at_icount / epoch_len` |
|---|---|---|
| `"EPOCH_HASH chain value"` (L206) | the epoch's icount | **correct** — names the bad epoch |
| `"EPOCH_HASH the recording does not have"` (L215) | the epoch's icount | correct-ish |
| `"end_vns"` (L327) | `header.end_icount` (L330) | names `total_epochs`, an epoch that **matched** |
| `"EPOCH_HASH count ..."` (L338) | `header.end_icount` (L340) | same — names a matched epoch |
| `"end_state_hash"` (L346) | `header.end_icount` (L350) | same — all epochs matched, yet reports `first_bad_epoch = 10` |
| `"resealed log bytes ..."` (L379) | **first differing BYTE OFFSET** (L381) | **pure nonsense** — a byte offset divided by an instruction count |

The unit being divided is not always an icount. For the resealed-log case the
engine itself documents `at_icount = first differing byte offset` in the `what`
string, so dividing it by `epoch_len` produces a meaningless small integer that
will be reported to the operator as "the first bad epoch." For `end_state_hash` /
`end_vns` / `EPOCH_HASH count`, the division is dimensionally fine but semantically
dishonest: every epoch link matched (the engine only reaches the END checks *after*
the epoch loop succeeded), yet the report claims `first_bad_epoch = total_epochs`,
pointing the reader at the last epoch that was actually verified OK.

The prompt's own framing confirms this is the intended scrutiny: an
`end_state_hash` divergence with all 10 epochs matching should not be labeled
`first_bad_epoch: 10`.

**Why it matters now:** cw2 (the 1000x M7 acceptance harness) and rfv (the RPC) are
the direct consumers. A child that diverges only at `end_state_hash` (a real
failure mode — e.g. a device-state mismatch that does not perturb the absolute
epoch grid) will be reported with a confidently-wrong epoch index, sending whoever
triages the 1000x run to the wrong place.

**Fix options (any one):**
- Make the mapping `what`-aware: only `EPOCH_HASH chain value` /
  `EPOCH_HASH the recording does not have` get `at_icount / epoch_len`; the END-class
  divergences get `first_bad_epoch = expected_epochs.len()` *with `what` making clear
  all epochs matched and the divergence is post-epoch*; the resealed-byte case must
  NOT divide a byte offset — report `first_bad_epoch` as a sentinel (e.g. `u64::MAX`)
  or change the field to `Option<u64>`.
- Simpler and more honest: change `first_bad_epoch` to `Option<u64>` (proto
  `first_bad_epoch` is a plain `uint64`, but the *model* is internal and can carry
  `None` for "divergence is not epoch-localized"), and only populate it for the two
  epoch-chain `what`s.

The existing live test only exercises the `EPOCH_HASH chain value` path (the
poisoned-RAM recording diverges at the first epoch link), so it passes while
hiding the bug for the other five. See S4.

---

### I2 — The model's `Divergence` does not mirror proto §2.7, contrary to its doc comments

**Files:** `crates/dh-verify/src/verify.rs:11,21` and `proto/hypervisor.proto:340-349`

The module doc and variant docs claim proto fidelity:

```rust
/// One verification event — mirrors proto `VerifyReplayProgress`.   // L11
...
/// Terminal mismatch (proto `Divergence`, P0 by convention). The
/// bisection fields (icount_lo/hi, rip pair) arrive with M8.          // L21-22
```

But the actual fields do not correspond. Proto `Divergence` (hypervisor.proto:340):

```proto
message Divergence {
  uint64 first_bad_epoch   = 1;
  uint64 icount_lo         = 2;
  uint64 icount_hi         = 3;
  uint64 rip_expected      = 4;
  uint64 rip_actual        = 5;
  bytes  reg_diff          = 6;
  repeated uint64 diff_page_idx = 7;
  string suspected_cause   = 8;
}
```

Model `Divergence` (verify.rs:23):

```rust
Divergence {
    first_bad_epoch: u64,   // matches proto field 1
    at_icount: u64,         // NO proto counterpart
    what: &'static str,     // NO proto counterpart (closest is suspected_cause, a String hint)
    expected: [u8; 32],     // NO proto counterpart — proto carries NO hash pair
    got: [u8; 32],          // NO proto counterpart
}
```

Only `first_bad_epoch` is shared. The doc says "the bisection fields (icount_lo/hi,
rip pair) arrive with M8" — implying those are the *only* missing fields and that
everything present mirrors the proto. In fact:

- The four present fields beyond `first_bad_epoch` (`at_icount`, `what`,
  `expected`, `got`) have **no** proto counterpart.
- The proto carries **no expected/got hash pair at all**, yet the model's headline
  divergence payload is exactly that pair. The bead description ("Divergence{first_divergent_epoch, hashes}")
  justifies the hash pair as the *library* contract — so the model is faithful to the
  **bead**, not to the **proto**. That is a fine decision, but the doc comment
  claiming proto fidelity is false and will mislead whoever writes rfv's RPC
  translation expecting a 1:1 mapping.

Separately, `EpochOk` and `VerifyDone` *do* match the proto field-for-field
(`epoch_index`/`icount`; `total_icount`/`end_state_hash`) — credit there (see
positive notes). The fidelity problem is isolated to `Divergence`.

**Fix:** Correct the doc comments to state the truth: the model's `Divergence` is the
**library** verdict shape (bead 1py: first bad epoch + the hash pair that diverged),
and rfv will *translate* it into proto `Divergence` — populating `first_bad_epoch`,
leaving the M8 bisection fields zero/empty, and routing `what` into
`suspected_cause`. The hash pair (`expected`/`got`) has no proto home and is
library-only; say so. Optionally rename the variant doc references so future readers
don't expect a struct-for-message correspondence that does not exist.

---

### I3 — EpochOk-count invariant is pinned only in debug builds

**File:** `crates/dh-worker/src/verify_replay.rs:53-63`

```rust
let mut emitted = 0u64;
for rec in log.aux() {
    if let RecordBody::EpochHash { epoch_index, .. } = rec.body() {
        report.push(VerifyProgress::EpochOk { epoch_index, icount: rec.icount() });
        emitted += 1;
    }
}
debug_assert_eq!(emitted, outcome.epoch_hashes_verified);
```

The EpochOk stream is reconstructed **post-hoc** by re-parsing the log and counting
its `EPOCH_HASH` records, then cross-checked against the count the engine actually
verified (`outcome.epoch_hashes_verified`). The cross-check is the *only* thing
guaranteeing the reconstructed stream matches what was proven — and it is a
`debug_assert_eq!`, compiled out in release.

In a release build (which is what cw2's 1000x harness and rfv's production RPC will
run), if the re-parse ever counts a different number of epoch records than the
engine verified — e.g. a future change to `log.aux()` filtering, an `EPOCH_HASH`
record the engine skipped, or a parse that tolerates a trailing record the verifier
did not reach — the report would silently emit a wrong number of `EpochOk` events
and still claim `verified()`. The whole point of this layer is to be a trustworthy
verdict; a silently-wrong epoch count defeats it.

The cost of a hard check here is one comparison per verification run (not per epoch),
which is negligible against an actual replay. This is the verification crate's
correctness invariant, not a performance hot path.

**Fix:** Promote to a hard check that converts a mismatch into an error (it is an
*infrastructure* inconsistency — the log and the engine disagree about reality — so
returning `Err(ReplayError::...)` is the honest classification, not an `Ok`
verdict). For example:

```rust
if emitted != outcome.epoch_hashes_verified {
    return Err(ReplayError::Apply(format!(
        "epoch reconstruction mismatch: log has {emitted} EPOCH_HASH records, \
         engine verified {}", outcome.epoch_hashes_verified)));
}
```

(Or a dedicated variant.) Keep the `debug_assert_eq!` too if desired, but do not let
release builds skip the invariant.

---

## On the boundary classification (reviewed, no change needed)

The Ok-verdict / Err-infrastructure split is the correct design and the variants are
classified correctly:

- **`HeaderMismatch`** stays `Err` — **correct.** A wrong-config or wrong-snapshot
  log is not a verdict *about the recording's determinism*; it means the caller paired
  the wrong inputs. Returning a `Divergence` verdict here would falsely accuse a
  recording of non-determinism when the operator simply supplied a mismatched
  (snapshot, config). The engine refuses these *before* any restore (replay_engine.rs
  L101-114), and the existing test asserts exactly this (`HeaderMismatch("machine_config_hash")`).
- **`NotYetWired`** stays `Err` — **correct.** A record kind the executor cannot apply
  is a capability gap in the verifier, not a property of the recording. Emitting a
  verdict (pass or fail) would be a lie about coverage. Loud `Err` is right.
- **`Restore`, `Log`, `Apply`, `Run`** stay `Err` — **correct.** Store/parse/KVM
  failures are infrastructure.
- **`Divergence`** maps to `Ok(report)` — **correct in principle** (it *is* a verdict
  about the recording), but see I1: the `first_bad_epoch` it computes is wrong for 5
  of the 6 sub-kinds.
