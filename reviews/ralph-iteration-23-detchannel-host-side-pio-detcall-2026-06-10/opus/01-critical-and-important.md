# Critical & Important Findings

**None.**

I scrutinized every item on the review checklist and confirmed each is correct or a
sound, documented design decision. The detail of each verification is below so the
"none" verdict is auditable.

## 1. Per-port semantics vs guest-sdk API.md §5 — CORRECT

- **IDENT (`0xD370` IN):** returns `0xD37E_0001` (`IDENT_ANSWER`), matches §5 row
  (`magic 0xD37E << 16 | proto 1`). Logged as `PIO_ANSWER`. ✓
- **INIT_LO/HI (`0xD374`/`0xD378`):** OUT latches the half into `init_lo`/`init_hi`;
  IN echoes the last latched value. Matches "latched / last latched value". ✓
- **INIT_GO state machine (`0xD37C`):** `channel_init` (detchannel.rs:263) checks
  already-attached → `AlreadyAttached(3)`; wrong committed size → `BadMagicVersion(2)`;
  2 MiB misalignment → `BadGpa(1)`; then `Channel::attach`, which maps its `AttachError`
  via `init_status()` (BadGpa for `Mem`, BadMagicVersion otherwise). This matches §5's
  `0 OK / 1 bad GPA / 2 bad magic-version / 3 already attached` and channel.rs:59-65.
  The "failed commit then valid commit" path is exercised (init_status_codes test:
  bad-size → 2, misaligned → 1, unmapped → 1, then valid → 0, second → 3) and is sound:
  failed commits never set `self.channel`, so a later valid commit still attaches. ✓
- **Pre-commit sentinel `INIT_STATUS_NEVER_COMMITTED = u32::MAX`:** SOUND. The ABI
  defines only post-commit statuses `0..=3`. Returning `u32::MAX` for an `IN 0xD37C`
  before any `OUT 0xD37C` is deliberately none of them, so a guest cannot misread a
  stale `Ok`. This value is itself logged as a canonical `PIO_ANSWER`, so replay
  re-answers the same sentinel — deterministic. The guest SDK's own `InitStatus::from_u32`
  returns `None` for it (ports.rs:51), i.e. the guest treats it as "not a status",
  which is the intended contract. ✓
- **DOORBELL (`0xD380`):** empty mask → `doorbell_empty_mask` metric + no drain; any
  nonzero mask → `drain()` (both rings). `detguest_host::drain_events` has no per-ring
  selector (drain.rs:211 always drains A then W), so draining the superset on a
  single-ring request is **legal**: rings are drained unconditionally at every pause
  boundary regardless (ARCH §6.6 "Rings are also drained at every pause boundary"), so
  an early extra drain of the un-requested ring changes nothing the guest can observe
  beyond what a pause would have done, and every cons-bump it produces is logged and
  replayed identically. ✓
- **INJECT (`0xD384`):** OUT latches `inject_iseq` and drains ring W inside the exit
  (so the matching `InjectQuery` is folded into `pending_injects` before the answer);
  IN calls `inject_answer` → `responder.answer`, which reports via the sink's
  `pio_answer` (hence the early `return` in `pio_in` to avoid double-logging). This is
  exactly the §5 sequencing rule ("host drains ring W first if the matching InjectQuery
  is not yet seen") and the unmatched-iseq → Proceed(0) + metric rule (inject.rs:57-60).
  Splitting drain into the OUT and answer into the IN is consistent with the spec: the
  drain is "inside the INJECT exit", and the OUT/IN pair *is* that exit. ✓
- **QUIESCE_ACK (`0xD388`):** OUT latches low-32 token; IN returns 0. Matches §5. ✓
- **RAZ/WI elsewhere:** unknown ports ignore OUT (metric) and answer 0 on IN (still
  logged as a canonical `PIO_ANSWER`, since the guest can read it). Matches §5's
  "Unknown ports in the range are RAZ/WI" and the repo rule that every IN return value
  is a canonical record. ✓

## 2. SDK_EVENT digest-by-re-encoding — CONSISTENT (one forward-compat caveat)

`sdk_event_digest` (detchannel.rs:489) decodes nothing new; it re-encodes the *already
decoded* `OwnedPayload` through `encode_event` and digests `buf[RECORD_HEADER_LEN..n]`.
Because both recording and replay run this identical code over the identical decoded
payload, the digest is deterministic and verification's compare holds. This is consistent
with API.md §3.3's SDK_EVENT semantics (digest of the payload; payload bytes live in the
gRPC stream, not the log). The caveat (encoder must not change between record and replay
builds) is a real but out-of-scope forward-compat risk — see 02-suggestions.md #1. Not a
defect in this change.

## 3. DEV_EVENT byte encodings vs API.md §3.3 — CORRECT

Verified against `dh-inputlog/dhilog.rs`:
- `dev_event` prepends `device_id u16 | event_type u16 | data_len u32` (dhilog.rs:157-161),
  then the `data`. So the `data` payload starts at byte offset 8 of the DEV_EVENT payload —
  matching the tests' `p[8]` ring-id assertions and `p[12..16]` new_prod assertion.
- `RING_PUSH` data: `ring_id u8 | _pad[3] | new_prod u32 LE | record bytes` — CtxSink
  (detchannel.rs:363-372) builds exactly this. ✓
- `CONS_BUMP` data: `ring_id u8 | _pad[3] | new_cons u32 LE` (8 bytes) — detchannel.rs:374-381. ✓
- `PIO_ANSWER`: `port u16 | _pad u16 | value u32` via `LogWriter::pio_answer`
  (dhilog.rs:166-183). ✓
- `SDK_EVENT` AUX: `stream u16 | _pad u16 | len u32 | digest8 u64` via
  `LogWriter::sdk_event` (dhilog.rs:221-234). ✓
- Ring-id mapping `0=C,1=I,2=A,3=W` (`ring_id_byte`, detchannel.rs:353) matches API.md
  §3.3's `ring_desc` order. ✓

## 4. No channel-memory mutation outside the sink — HOLDS

The only guest-RAM **writes** in the consumed library are `drain_ring`'s
`write_u32(cons_gpa, pos)` immediately followed by `sink.cons_bump` (drain.rs:316-317),
and the ring producers' writes in `push_command`/`push_workload_ctrl` (which call
`sink.ring_push`). `Channel::attach`, `read_manifest`, and `drop_counters` only *read*.
The host struct never calls `gm.write` directly; all writes pass through `CtxSink`, which
logs each one. The `channel()` accessor is `&Channel` (read-only). Invariant holds. ✓

## 5. AlreadyAttached + post-attach latch writes — FINE

After attach, `channel_init` short-circuits on `self.channel.is_some()` → returns
`AlreadyAttached` without re-reading `init_lo/init_hi`. Subsequent `OUT 0xD374/0xD378`
still mutate the latches (and IN echoes them), but `channel_init` never consults them
again once attached, so the live channel is unaffected. Echo-only mutation of a latch is
harmless and matches the "latched / last latched value" ABI. ✓

## 6. Drain failure → empty vec + metric — DETERMINISTIC

`drain_events` returns `WireError` only for guest-state-derived conditions:
`CorruptIndices` (prod−cons > size, drain.rs:236), `Decode(BadLen)` (bad length /
len-crosses-ring-end, drain.rs:253/262), other decode errors, or `Mem` (a `GuestMem`
read failing — itself a deterministic function of the mapping). None depend on host
wall-clock, host RNG, or host I/O, so "deterministic function of guest state" is true for
every `WireError` variant reachable here. Counting and returning empty is a defensible
local policy; escalation to FAULTED is explicitly left to run control via the metric (a
correct separation — see suggestion 02 #2 about making the caller honor it). ✓

## 7. Failed-attach mem clone + manifest read failure — SOUND

`Channel::attach(self.mem.clone(), gpa)` clones the handle; on `Err` the clone is dropped
(the `M: Clone` bound's stated purpose, detchannel.rs:24). `M` is a cheap handle clone in
the VMM (and an `Rc<RefCell<>>` in tests), so this is correct and inexpensive. Manifest
read failure at attach is non-fatal: attach still succeeds, `manifest` stays `None`,
`manifest_read_failures` increments. This is reasonable — ARCH §6.6 says the host re-reads
the manifest seqlock-consistently and "after any restore re-reads it"; a transient
unreadable manifest at CHANNEL_INIT (e.g. guest mid-registration with an odd generation
→ `SeqlockLivelock`) should not refuse the whole channel, and the capture engine simply
has no region resolution until a later successful read. ✓

## 8. Test quality — STRONG (gaps noted as suggestions, not blockers)

Byte-level assertions match the dhilog encodings (`p[8]` ring id, `p[12..16]` new_prod,
`sdk_payloads[1][0..2]` stream). The 11 tests cover: IDENT, RAZ/WI OUT+IN, attach +
manifest, the full INIT status matrix (sentinel / bad-size / misaligned / unmapped /
ok / double-commit), zeroed-page magic failure, doorbell drain (empty mask + W drain +
cons-bump + 2 SDK digests + IN answer), inject flow (matched via plan + logged once,
IN-without-OUT, unmatched iseq), quiesce-ack latch, drain-before-attach + push-error,
ring-C push, and pause-drain == doorbell-drain. Coverage gaps (ring-A drain, drop
counters, RegionRegister, drain-failure metric, restore/re-attach producer-seq path) are
genuine but are hardening, not correctness gaps in the code under review — see
02-suggestions.md #3.
