# Action items

### Critical

- [ ] **C1 — Decide where the encoder fingerprint is emitted, and move it off
  the device attach event.** Today it fires only on `pio_out(PORT_INIT_GO)`
  (`detchannel.rs:329-339`). `DetChannelHost::restore` (`detchannel.rs:219`)
  re-attaches without that port write, so any segment that begins from a
  snapshot/restore — or a §8.4 fork child — produces **zero** ENCODER_FP
  records, directly violating the "emitted ONCE per segment" invariant the new
  DHILOG doc asserts (`dhilog.rs:237-250`). No verifier consumes the record
  yet, so the fix is cheapest now.
  - **Recommended:** emit the fingerprint at segment/log construction — fold it
    into `LogWriter::new` (or the segment header), passing
    `wire_encoder_fingerprint()` in from the caller so `dh-inputlog` keeps no
    `detguest-wire` dependency. Then every segment carries exactly one
    ENCODER_FP by construction, regardless of how the channel got attached.
  - **Alternative (explicit scope cut):** if the team intends boot-path-only,
    best-effort fingerprinting, amend the DHILOG doc to drop "ONCE per segment"
    and state restore-origin segments are unguarded — and file a follow-up bead
    for the verifier to tolerate the absence. This is a deliberate decision, not
    a default.
  - **Add the missing test** that would have caught this: drive a
    snapshot → `restore` → inspect the new segment's log and assert the
    ENCODER_FP presence/absence matches the agreed contract (S2).

### Important

- [ ] **I2 — Fix the misattributed doc comment in `ctx.rs:126-136`.** Move the
  `/// Log an AUX FRAME_MARK ...` line back onto `log_frame_mark` (currently
  undocumented) and remove it from `log_encoder_fingerprint`'s doc block (where
  it is wrong). One-line move; purely a doc-correctness fix.

- [ ] **I3 — Remove the VMM-crash tripwire in `wire_encoder_fingerprint`
  (`detchannel.rs`).** The `encode_event(...).expect("probe set must always
  encode")` runs inside a guest-triggered vCPU exit. It is unreachable for the
  current fixed probe set, but its safety rests on `MAX_NAME` / `MAX_RECORD_LEN`
  / `record_len` constants in the **sibling** `detguest-wire` repo — the same
  HEAD-wins sibling the whole feature defends against. A future sibling bump
  could turn it into a hypervisor-process panic on a guest path. Either
  const-assert the probe lengths / size the buffer from `record_len` so it is
  infallible by construction, or return `Result` and route a failure through the
  existing `ctx.log_fault` latch (consistent with `DevCtx::record`'s
  panic-free discipline).

### Suggestions

- [ ] **S1 — Soften the `wire_encoder_fingerprint` doc** from "Any wire-format
  change ... flips this value" to "any change to the probe-set encodings," and
  note the residual ~2^-64 truncated-digest false-agreement. `digest8`'s
  8-byte width is fine for a skew tripwire; just don't over-claim totality.

- [ ] **S2 — Align the fingerprint test with its name.** Either rename
  `encoder_fingerprint_is_deterministic_and_logged_at_attach` to drop
  "and_logged_at_attach," or actually exercise the attach path and assert one
  `KIND_ENCODER_FP` record was logged (and pair it with the restore-path test
  from C1).

- [ ] **S3 — Introduce a `const LEAF_ENCODED_LEN = 28`** (or `7 *
  size_of::<u32>()`) shared by `CpuidLeaf::encode_into`'s contract and the
  `cpuid_leaves_hash` capacity hint, so the two literal `28`s cannot drift if a
  future leaf field lands. Hygiene only; current code is correct.

- [ ] **S4 — (FYI only)** `wire_encoder_fingerprint` allocates a 4 KiB `buf`
  plus a growing `Vec` per call. Negligible (attach/segment-start is cold); no
  change required. Noted so it is not later mistaken for a hot path.
