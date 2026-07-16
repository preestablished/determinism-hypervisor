# Critical & Important findings

## C1 (CRITICAL design / data-correctness, latent) — the encoder fingerprint is emitted by the device attach event, not the segment; restore produces a fingerprint-less segment, violating the invariant the code itself asserts

### What the code does

`detchannel.rs` `pio_out(PORT_INIT_GO)`:

```rust
self.init_status = self.channel_init(value) as u32;
if self.init_status == InitStatus::Ok as u32 {
    ctx.log_encoder_fingerprint(wire_encoder_fingerprint());
}
```

`dhilog.rs` documents the record as:

> ENCODER_FP ... emitted **ONCE per segment** at channel attach ... a
> verifier replaying an old log with a changed encoder detects the skew by
> comparing fingerprints.

So the asserted contract is: **each segment's log carries its writer's
encoder fingerprint exactly once.**

### Why the placement breaks that contract

`DetChannelHost::restore` (detchannel.rs:219, iteration 42) re-attaches the
channel *directly* — `Channel::attach(...)` + `restore_producer_seqs(...)` —
and **never calls `pio_out(PORT_INIT_GO)`**. The fingerprint is emitted only
on the live-attach path. Therefore:

- **Parent run** (boots, guest writes `OUT PORT_INIT_GO`): segment log gets
  one ENCODER_FP record at the attach icount. Correct.
- **Replay-from-restore** (or a fork child, §8.4 — the comment at
  detchannel.rs:216 calls `new()` + `restore` "the fork path's seam"): the
  host is reconstructed from EVTC, the channel is live again, but no
  `PORT_INIT_GO` ever fires, so the new segment's log has **no ENCODER_FP
  record at all.**

### M4-replay walk — concrete failure mode

The record exists to let a verifier *detect encoder skew before comparing
SDK-digest streams*. Walk it end to end:

1. Original capture produces segment S0 with `[ENCODER_FP(fp_a), SDK_EVENT...]`.
2. A snapshot is taken mid-S0; capture stops; EVTC is serialized.
3. Later, replay (or a divergence investigation, or a fork) restores from the
   snapshot. The restored host runs segment S1. S1's log is
   `[SDK_EVENT..., (no ENCODER_FP)]`.
4. A future verifier (the consumer the doc promises) loads S1's log to compare
   its SDK digests against a re-execution. It looks for the ENCODER_FP guard
   record to confirm the encoders match. **It is not there.** Two outcomes,
   both bad:
   - The verifier *requires* the guard → it false-rejects every
     restore-origin segment as malformed/unguarded.
   - The verifier *tolerates* a missing guard → restore-origin segments lose
     the skew protection entirely, which is the exact "spurious SDK-digest
     divergence" the feature was built to prevent. The protection silently
     evaporates on precisely the path (snapshot/restore/fork) where a SIBLING
     encoder version is MOST likely to be in play (a parent captured by an old
     build, replayed by a new one).

This is the worst shape of latent bug: it compiles, every test is green
(no test exercises restore-then-inspect-the-log for ENCODER_FP), and it will
be discovered only when someone writes the verifier against the documented
invariant and finds half the segments don't honor it.

### Where the emission belongs — my recommendation

The fingerprint is a property of **the encoder that wrote this log**, not of
**the device's attach gesture**. The two coincide only on the live-boot path.
The correct home is the **log/segment**, not the device:

- **Best: emit in `LogWriter::new` (or fold into the segment header / first
  record at segment open).** Then *every* segment — boot-origin, restore-origin,
  fork-child — carries exactly one ENCODER_FP, unconditionally, by
  construction. The invariant becomes structurally true instead of
  incidentally true. `LogWriter` already owns `digest8`; it can own the
  fingerprint of the encoder it was compiled against.
- The fingerprint value is a pure function of the wire encoder
  (`wire_encoder_fingerprint()` is in `dh-devices`/`detguest-wire` land, not
  `dh-inputlog`). To keep the layering clean, the caller that constructs the
  `LogWriter` for a segment can pass the fingerprint in (e.g. a
  `SegmentHeader`/`new_with_encoder_fp` parameter), so `dh-inputlog` stays
  free of a `detguest-wire` dependency. That preserves the current crate
  boundary while moving the *emission trigger* from "device attach" to
  "segment start."

The current code comment ("A re-attach re-emits — each record is truthful for
the encoder that wrote it") defends *multiple* emissions on re-attach, which
is fine. But it does not address *zero* emissions on restore, which is the
actual hole. A per-segment placement makes both concerns moot.

### Severity rationale

Marked CRITICAL because it is a correctness defect in a *determinism/verifier*
feature whose entire purpose is correctness, it is invisible to the current
test suite, and the fix is cheapest now (no consumer to migrate). If the team
consciously decides ENCODER_FP is "best-effort, boot-path-only" and amends the
"once per segment" doc to say so, this downgrades to a doc fix — but that is a
deliberate scope reduction that should be made on purpose, hence the overall
NEEDS_DISCUSSION verdict.

---

## I2 (IMPORTANT, real bug) — misattributed doc comment in `ctx.rs`; `log_frame_mark` left undocumented

`crates/dh-devices/src/ctx.rs:126-136`:

```rust
/// Log an AUX FRAME_MARK at this boundary (pv-pad FRAME_COUNTER write).
/// Log the one-time detguest-wire encoder fingerprint (bead 4ld;
/// emitted by the detchannel at successful attach).
pub fn log_encoder_fingerprint(&mut self, fingerprint: u64) { ... }

pub fn log_frame_mark(&mut self, frame_index: u32) { ... }
```

The new fn was inserted *between* `log_frame_mark`'s original doc line and
`log_frame_mark` itself. Result:

- `log_encoder_fingerprint` now carries a wrong first doc line ("Log an AUX
  FRAME_MARK ... FRAME_COUNTER write") that has nothing to do with it.
- `log_frame_mark` is now **completely undocumented**.

Fix: move the FRAME_MARK doc line back down onto `log_frame_mark`, and drop it
from `log_encoder_fingerprint`'s doc block:

```rust
/// Log the one-time detguest-wire encoder fingerprint (bead 4ld;
/// emitted by the detchannel at successful attach).
pub fn log_encoder_fingerprint(&mut self, fingerprint: u64) { ... }

/// Log an AUX FRAME_MARK at this boundary (pv-pad FRAME_COUNTER write).
pub fn log_frame_mark(&mut self, frame_index: u32) { ... }
```

Important (not just cosmetic) because this is a determinism codebase where the
doc comments *are* the spec annotations reviewers rely on; a misattributed one
will mislead the next reader about what record `log_frame_mark` writes.

---

## I3 (IMPORTANT, robustness) — `.expect` in `wire_encoder_fingerprint` is a VMM-process panic on a guest-triggered path; it is currently unreachable but only by accident

`wire_encoder_fingerprint()` runs inside `pio_out(PORT_INIT_GO)` — i.e. inside
a vCPU exit, on a path the **guest triggers** by writing the detcall port. It
calls:

```rust
let n = encode_event(&mut buf, i as u32, 7, 0, p).expect("probe set must always encode");
```

`encode_event` can return two errors: `BufferTooSmall` (when `buf.len() <
record_len(payload_len)`) and `FieldTooLong` (when a `NameIntern.name >
MAX_NAME`). For the current fixed probe set this is genuinely unreachable:
`buf` is `MAX_RECORD_LEN` (4096) and the largest probe is the 19-byte
`b"dh-encoder-fp-probe"` NameIntern (`MAX_NAME = 256`). So today the `.expect`
never fires.

The hazard is **maintainability/coupling**, not a live crash:

- The safety of the `.expect` depends on an invariant held in a *different
  repo* (`guest-sdk/crates/detguest-wire`: `MAX_NAME`, `MAX_RECORD_LEN`,
  `record_len`). The "HEAD-wins sibling dep" model that this whole iteration is
  built around (the comment block literally describes encoder skew between
  sibling versions) means those constants can move under this code. If a future
  detguest-wire bump shrinks `MAX_NAME` below the probe length, or grows the
  record header so `record_len` exceeds the buffer, **the `.expect` becomes a
  panic inside a vCPU exit — a VMM crash on a guest-reachable path.**
- A panic here is not a clean error: it aborts the whole hypervisor process
  while a guest is running.

Recommendation (any of):
1. **Make it infallible by construction** — `const`-assert the probe lengths at
   compile time (`const _: () = assert!(PROBE_NAME.len() <= /* local copy */)`),
   or size `buf` from `record_len`/`max_record_len(probes)` so the buffer can
   never be too small. A compile-time check turns a possible runtime panic into
   a build break in the sibling-bump PR.
2. **Or return `Result`/`Option`** from `wire_encoder_fingerprint` and have the
   caller route a failure through the existing `ctx.log_fault` latch (the same
   graceful-degradation path every other record uses — see `DevCtx::record`,
   which latches `WriteError` instead of panicking). This keeps a wire-format
   regression from ever crashing the VMM.

Given the rest of the logging path is panic-free by design (`record()` latches
errors), the lone `.expect` on a guest-triggered path is an inconsistency worth
closing. Severity IMPORTANT, not CRITICAL, because it is presently unreachable —
but it is a tripwire pointed at the VMM.
