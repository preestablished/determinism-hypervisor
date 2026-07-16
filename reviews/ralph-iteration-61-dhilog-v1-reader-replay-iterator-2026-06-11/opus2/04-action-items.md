# Action Items

## Critical

- [ ] None. The parser passed all 20 adversarial probes and 25 in-tree tests with no
      panic and no incorrect acceptance/rejection. Nothing blocks merge on code grounds.

## Important

- [ ] **Fix the stale API.md §3.1 reserved-bytes row.** `.agents/docs/determinism-hypervisor/API.md:520`
      still lists `| 240 | 16 | reserved |`, but bead 4ld (closed 2026-06-10) repurposed
      `[240..248)` as the `u64 encoder_fingerprint`, which this branch's reader
      (`reader.rs:368`), writer (`dhilog.rs:299`), header doc-comment (`reader.rs:96–99`),
      and test (`reader_validation.rs:224–238`) all implement. Replace the single 16-byte
      `reserved` row with two rows:
      ```
      | 240 | 8 | `encoder_fingerprint` | u64 detguest-wire fingerprint (bead 4ld); 0 ⇒ no SDK digests. Verifiers compare before SDK_EVENT digests to detect encoder skew. |
      | 248 | 8 | `reserved` | zeros; readers MUST reject nonzero (reserved-means-zero rule) |
      ```
      State in the change that **the code is authoritative and correct — only the table
      was wrong.** If editing the normative spec is out of scope for bead ecv, instead
      `bd create` a docs bead (`-l docs -p 1`) referencing 4ld + iteration 61 so the
      divergence is tracked, not silently carried into bp9.

## Suggestions

- [ ] **(S-1)** Add a one-line comment at `reader.rs:172`/`reader.rs:426` noting END
      `stop_reason` is intentionally an unvalidated `u8` (forward-compatible per §3.3),
      so the omission does not read as an oversight. No range check needed in the codec.
- [ ] **(S-2)** Add a comment at `reader.rs:487` noting the spec gives no NET_RX lower
      bound, so a 0-length frame is accepted by design. If a minimum frame length is
      wanted, land it in API.md §3.3 first, then enforce here.
- [ ] **(S-3)** File a follow-up bead for an inspection-only `parse_unsealed` / `inspect`
      entry point: framing/totality validation without the SEALED, body_hash, END-present,
      and end-cross-check gates, returning records best-effort up to truncation, explicitly
      marked "never feed to replay" (§3.4.4). Keeps the replay path strict while unblocking
      crash-artifact diagnostics. Not needed for this bead.
- [ ] **(S-4)** (Cosmetic, optional) `SeqMismatch.expected` saturates to `u32::MAX` after
      an unreachable 2^32-record count (`reader.rs:384`). The check itself (`reader.rs:406`,
      compared in `u64`) is correct; only the diagnostic field could mislead in an
      impossible regime. Safe to leave.
- [ ] **(S-5)** (Optional perf) Cache the END payload (or its offset) in `LogReader` at
      parse time so `end()` (`reader.rs:283–293`) does not re-walk the whole body on each
      call. Only matters if `end()` is called in a loop.

## Re: bead bp9 (byte-identical re-serialization)

- [ ] No action — the reader is **sufficient** for bp9's assertion. `LogReader` holds the
      original `body: &[u8]` and a fully-parsed `Header`; the original buffer *is* the
      validated bytes (padding included), so a writer re-run can be compared
      byte-for-byte against the input slice directly. No extra padding/exact-byte accessor
      is required.
