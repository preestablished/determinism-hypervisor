# Suggestions (non-blocking)

These are spec-acceptable gaps and polish items. None blocks the merge. Several are
deliberate "weigh this" questions from the review brief; my call on each is stated.

## S-1 — END `stop_reason` is an unvalidated `u8` (probe P11: `stop_reason = 200` accepted)

- **File:** `reader.rs:172–175` (`body()`), `reader.rs:426–437` (END validation)
- API.md §3.3 says END `stop_reason` "mirrors proto StopReason." The reader accepts
  any `u8` (I parsed `stop_reason = 200` and it round-tripped through `end()`).
- **My call: acceptable, leave as-is for this bead.** Validating against the proto
  enum range would couple the byte-level codec to a generated proto range that can
  grow in later minors, and an out-of-range `stop_reason` does not threaten replay
  totality (the replayer reads `end_state_hash`, not `stop_reason`, to terminate).
  An unknown stop reason is the kind of additive change the version field exists to
  absorb. If a future bead wants a "known stop reason" lint it belongs in a
  higher-level validator, not the total decoder. Worth a one-line `// stop_reason is
  not range-checked: forward-compatible per §3.3` comment so the next reader does not
  read the omission as an oversight.

## S-2 — Zero-length NET_RX frame is accepted (probe P8b)

- **File:** `reader.rs:487` (`KIND_NET_RX => payload.len() <= MAX_NET_RX_FRAME`)
- A 0-byte NET_RX payload passes (the bound is only an upper bound). API.md §3.3 says
  "`payload_len` is the frame length, ≤ 2048" — it states an upper bound and is silent
  on a lower bound. A real Ethernet frame has a 60-byte minimum, so a 0-length frame is
  physically meaningless.
- **My call: acceptable for the codec layer, but note it.** The decoder's job is
  framing + spec bounds, and the spec gives no lower bound, so rejecting zero here would
  be the reader inventing a rule. Whether an empty NET_RX is *semantically* valid is a
  replay-layer question (does the NIC model accept a 0-length injected frame?). If the
  team wants the stricter rule it should first land in API.md §3.3 as a normative lower
  bound, then here. Until then, accepting it is the spec-faithful choice. A short
  comment at reader.rs:487 noting "spec gives no lower bound" would prevent a future
  reviewer re-litigating this.

## S-3 — Unsealed logs rejected wholesale; inspection tooling may want to read them

- **File:** `reader.rs:348–350` (`NotSealed`)
- §3.4.4 says an unsealed crash artifact MUST NOT be *replayed*. It says nothing about
  *inspection*. `parse()` rejects unsealed entirely, so crash-artifact tooling (e.g. a
  post-mortem that wants to walk the records emitted before the crash) cannot use this
  reader at all.
- **My call: rejecting wholesale is the right default for this bead** — `parse` is the
  replay-grade gate and the body_hash/end_state_hash are zero in an unsealed log, so
  most of the validation battery is meaningless on one. But a follow-up is warranted:
  file a bead for a `parse_unsealed` / `inspect` entry point that skips the SEALED,
  body_hash, END-present, and end-cross-check steps but still does framing/totality
  validation, returning records best-effort up to the truncation point. Flag it
  explicitly as "inspection only, never feed to replay." This keeps the replay path
  strict while unblocking diagnostics later.

## S-4 — `SeqMismatch.expected` lies after the (unreachable) 2^32-record saturation

- **File:** `reader.rs:384` (`seq_for_err = u32::try_from(count).unwrap_or(u32::MAX)`),
  used at `reader.rs:408`
- The actual seq check (`reader.rs:406`, `u64::from(seq) != count`) is **correct** — it
  compares in `u64`, so it never wraps. Only the *reported* `expected` field saturates
  to `u32::MAX` once `count > u32::MAX`. At 2^32 records (× ≥ 32 bytes each = 128+ GiB)
  this is unreachable in practice, and the writer caps `seq` at `u32` via `SeqOverflow`
  anyway, so no valid log even reaches that count.
- **My call: cosmetic, leave it.** The check is sound; only the error's diagnostic
  field could mislead in an impossible regime. Not worth complicating the hot path.
  Mentioning here only because the brief asked.

## S-5 — `end()` re-walks the entire body via `.records().last()`

- **File:** `reader.rs:283–293`
- `end()` iterates every record to get the last one each call. Functionally correct
  (parse guarantees END is last, hence the `unwrap`/`unreachable!`), but O(n) per call.
  If `end()` is hot (it likely is — replay reads `stop_reason`/`end_state_hash` to
  terminate), consider caching the END payload offset (or the parsed `(stop_reason,
  end_state_hash)`) in `LogReader` at parse time, since validate_records already visits
  the END record. Low priority; only matters if `end()` is called in a loop.
