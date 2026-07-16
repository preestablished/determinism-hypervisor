# Critical & Important Findings

**None.** No critical or important defects were found. This section instead records the
verification of each trap the review brief flagged — every one checks out, which is why they
are documented here rather than raised as findings.

---

## Verified: `wrote_epoch_hash` does NOT need the END-exclusion dance that `has_aux` got

**Files:** `crates/dh-inputlog/src/dhilog.rs:333-335` (the `has_aux` snapshot), `:267-268`
(`epoch_hash` sets the flag), `:404-405` (the generic AUX path).

The `has_aux` snapshot/restore around the END write exists because `record()` sets
`self.has_aux = true` for ANY record carrying `RFLAG_AUX` (lines 404-405), and END is written
with `RFLAG_AUX` (line 334) — so `seal()` must snapshot `has_aux` before writing END and restore
it after, to honor the spec rule "END does NOT count toward flags.HAS_AUX."

`wrote_epoch_hash` is immune to this problem by construction: it is set in exactly ONE place —
inside `epoch_hash()` (line 268), keyed on `KIND_EPOCH_HASH` — and is NOT touched by the generic
`RFLAG_AUX` branch in `record()`. Writing END is `record(KIND_END, RFLAG_AUX, …)`, which never
flips `wrote_epoch_hash`. **No snapshot needed; no spurious flag set. Correct.**

---

## Verified: no payload-length OOB gap in the reader (the classic trap)

**Files:** `crates/dh-inputlog/src/reader.rs:539` (length check), `:454` (validate runs before
decode), `:183-186` (decode slices `p[8..40]`).

The decode at `body()` does `chain_value: p[8..40].try_into().unwrap()` (line 185), which would
panic on a short payload. The length table at line 539 has the entry
`KIND_EPOCH_HASH | KIND_END => payload.len() == 40`, and `validate_kind()` runs at line 454
DURING `validate_records()`, which `parse()` calls at line 282 BEFORE any record body is ever
decoded. A short or long EPOCH_HASH payload is rejected with `BadPayloadLayout` at parse time, so
`p[8..40]` can never go out of bounds. **The classic "validation table missing the kind" gap is
NOT present — the entry exists. Closed.**

---

## Verified: sink-tuple ordering matches the consumer's destructure (no transposition)

**Files:** `crates/dh-vmm/src/runctl.rs:338,397` (push order), `crates/dh-vmm/src/recording.rs:78`
(destructure order), `:182-184` (live-test byte assertion).

- Sink push order: `(point.icount / epoch, point.icount, seg.chain.value())` =
  `(epoch_index, icount, chain_value)`.
- Consumer destructure: `for (epoch_index, icount, chain_value) in links` — same positional
  meaning, then `epoch_hash(*icount, boundary_rip, *epoch_index, *chain_value)` maps each to the
  correctly-named writer parameter.
- The live test asserts `links == vec![(1,30_000),…]` (index-first) AND the round-tripped record
  body byte-for-byte, so any transposition would fail the test. **No transposition. Correct.**

---

## Verified: exactly-once sink at a stop point that is also an epoch point

**Files:** `crates/dh-vmm/src/runctl.rs:333-339` (walk arm), `:344-369` (stop arms passing
`already_hashed=point.epoch_hash`), `:412-428` (`finish` has no sink param).

When a stop boundary (GoalSatisfied / Budget / HardCap) coincides with an epoch-grid point:
1. The `point.epoch_hash` arm (lines 333-339) pushes the chain link AND the sink entry — once.
2. The subsequent `finish(…, already_hashed=true)` skips re-linking the chain (line 424) and —
   critically — has **no `epoch_sink` parameter at all**, so it cannot double-sink.

Result: the chain is linked once and the sink receives exactly one entry for that boundary.
**Exactly-once for both link and sink. Correct.**

---

## Verified: pause roll-forward sink uses an in-scope, correct `epoch`

**File:** `crates/dh-vmm/src/runctl.rs:374-397`.

The pause branch declares its OWN `let epoch = seg.config.epoch_len.max(1);` at line 375 (the
agenda-walk arm declares its own separate `epoch` at line 337). Both are local; there is no
shared/leaked binding. The roll-forward target is `next_epoch = point.icount.div_ceil(epoch)
.max(1) * epoch` (line 376), a grid multiple, so `b.icount / epoch` (line 397) is an exact
epoch index. **The `epoch` in scope at the pause-sink site is the correct `epoch_len.max(1)`,
and `b.icount` is genuinely on the grid. Correct.**

---

## Verified: epoch-index arithmetic lands on the right index for the agenda grid

**Files:** `crates/dh-vmm/src/runctl.rs:337-338`, `crates/dh-vmm/src/config.rs:107` (`epoch_len: u64`).

`epoch_index = point.icount / epoch` with `epoch = epoch_len.max(1)` (both `u64`, integer
division). The agenda's epoch points land exactly on grid multiples by construction, so for
`epoch_len=30_000`, `point.icount=30_000 → 1`, `60_000 → 2`, `90_000 → 3` — verified by the live
test (lines 168-174). `.max(1)` guards against a divide-by-zero only as defense-in-depth; the
config layer already rejects `epoch_len == 0` (`config.rs:152-154`, `ConfigError::ZeroEpochLen`),
so the `.max(1)` path is effectively unreachable for validated configs. **Correct.**

---

## Verified: FLAG_EPOCH_HASHES cannot appear on a parseable unsealed log

**File:** `crates/dh-inputlog/src/reader.rs:375-376`.

`parse_header()` rejects any log without `FLAG_SEALED` (`if flags & FLAG_SEALED == 0 { return
Err(ReadError::NotSealed) }`) before any record is examined. Since `FLAG_EPOCH_HASHES` is only
ever written inside `seal()` (which by definition sets `FLAG_SEALED`), there is no path by which
a reader accepts an unsealed log carrying the epoch-hash flag. The reader also cross-checks
`has_epoch_hashes() == saw_epoch_hash` (line 508) and folds EPOCH_HASH into the HAS_AUX check
(line 505); since `epoch_hash()` writes with `RFLAG_AUX` (dhilog.rs:267), a log with epoch hashes
always also carries `FLAG_HAS_AUX`, satisfying that cross-check. **Flag semantics are consistent
end-to-end. Correct.**
