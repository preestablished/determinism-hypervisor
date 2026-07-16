# Positive Notes

1. **The no-mutation-outside-the-sink invariant is genuinely enforced, not just claimed.**
   Every guest-RAM write in the consumed library (`drain_ring`'s `cons_bump`, the ring
   producers' `ring_push`) is paired with a `ChannelWriteSink` call, and `DetChannelHost`
   never touches `gm.write` directly — the only handle it holds is fed to `Channel::attach`
   and thereafter accessed read-only. The `channel()` accessor returns `&Channel`, closing
   the obvious back-door. This is the load-bearing determinism invariant (ARCH §6.6) and it
   is structurally upheld.

2. **Pre-commit sentinel is the right call.** `INIT_STATUS_NEVER_COMMITTED = u32::MAX` is
   deliberately outside the ABI's `0..=3`, and guest-sdk's own `InitStatus::from_u32`
   returns `None` for it — so a guest that reads status before committing gets an
   unambiguous "no status yet" rather than a stale OK. It is still logged as a canonical
   PIO_ANSWER, so replay is faithful. Subtle and correct.

3. **The INJECT OUT-drain / IN-answer split exactly honors the §5 sequencing rule** and
   avoids the double-logging trap with a clean early `return` in `pio_in`, with the
   reasoning written down at both the `pio_in` site and the `CtxSink::pio_answer` impl. The
   `inject_flow` test's `record_count == before + 4` is a precise guard that the answer is
   logged exactly once.

4. **Forward-compatibility is handled deliberately at both seams.** `event_kind` and
   `wire_payload` both fall through `non_exhaustive` `OwnedPayload` to `None` and the drain
   loop counts (rather than drops) an undigestable future variant via
   `sdk_digest_failures` — so a newer guest-sdk variant degrades to a counted miss, not a
   silent loss or a panic.

5. **Metrics are honest about determinism.** The doc comment states they are "deterministic
   functions of guest behavior … never fed back to the guest", and every metric increment
   in the file (raz_wi, doorbell_empty_mask, drain_failures, manifest_read_failures,
   inject_in_without_out, sdk_digest_failures) is in fact derived only from guest state or
   ABI misuse. This keeps the observability surface off the execution path.

6. **Byte-level test assertions are tied to the real encoder layout**, not magic numbers
   in isolation: `p[8]` (ring id) and `p[12..16]` (new_prod) are derived from the documented
   `device_id u16 | event_type u16 | data_len u32` DEV_EVENT preamble, and the SDK_EVENT
   stream assertion reads `EventKind::FrameMark as u16`. A layout regression in `dhilog.rs`
   would actually break these.

7. **Manifest read failure policy is well-reasoned.** Treating an unreadable manifest at
   CHANNEL_INIT as non-fatal (attach succeeds, metric increments) correctly distinguishes
   "the channel page is a valid v1 channel" from "the manifest happened to be mid-write",
   and aligns with ARCH §6.6's "after any restore the host re-attaches and re-reads it"
   (the read is retried later, not a one-shot gate).

8. **Cargo.toml dep promotion is minimal and honest.** Moving `detguest-host`/`detguest-wire`
   from dev-deps to deps is exactly the manifest change required to consume them from
   non-test code, with no version churn and (per the change description) no lock churn. The
   crate's `#![forbid(unsafe_code)]` and the `no_host_ambient_authority` deny-list gate
   still apply to the new module.
