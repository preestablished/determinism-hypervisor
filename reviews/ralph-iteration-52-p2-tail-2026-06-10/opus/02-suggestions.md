# Suggestions (non-blocking)

### S-1 (4ld): the fingerprint binds to attach, not to segment — reconsider emit site for M4 replay

`wire_encoder_fingerprint()` is emitted exactly once, in the live `channel_init`
path at `PORT_INIT_GO` (`crates/dh-devices/src/detchannel.rs:330-338`). It is
the only emit site. `restore()` (`detchannel.rs:219`) re-attaches via
`Channel::attach` directly, takes no `DevCtx`/log handle, and therefore emits NO
fingerprint record.

A restored checkpoint begins a NEW DHILOG segment log. That segment can contain
AUX SDK_EVENT digests (the guest keeps draining events) but will have no
KIND_ENCODER_FP record, because the guest does not re-run CHANNEL_INIT after a
restore. The fingerprint's contract is "record and replay compare it before
comparing digests" — but a post-restore segment offers no fingerprint to compare,
so the verifier has nothing to gate on for that segment's digests.

Conceptually the fingerprint describes the *encoder that wrote this segment's
digests*, which is a per-SEGMENT property, not a per-attach event. Suggest, when
the replay/segmenting infra lands (post-M4): emit the fingerprint at SEGMENT
START (alongside or near `LogWriter::new(SegmentHeader{..})`) rather than (or in
addition to) at attach. That guarantees every segment that can contain SDK_EVENT
digests is self-describing. The current "re-attach re-emits, each record truthful
for its encoder" doc-comment is accurate for the attach event but does not cover
the restore-into-new-segment case. Not actionable today (no replayer/segmenter
exists), so this is a design note to carry into the M4 replay bead.

### S-2 (4ld): consider hoisting the fingerprint into the SegmentHeader instead of a record

Adjacent to S-1: since the fingerprint is a fixed 8-byte value that is constant
for a given encoder build, it is arguably segment *metadata* rather than a
timestamped input event. Putting it in `SegmentHeader` (sealed into the DHILOG
header, like `machine_config_hash` at offset 112) would make it unconditionally
present once per segment with no dependence on whether/when CHANNEL_INIT runs,
and a minimal replayer reads it from a fixed offset rather than scanning for an
AUX record. This is a bigger change (header layout = a DHILOG-v1 compat event,
per the existing dhilog doc-comment) so it is a deliberate trade, not a quick
edit — raise it when the replay format is being finalized. The current record
approach is perfectly fine for the write-only stage.

### S-3 (4ld): the deterministic test only checks self-equality, not a frozen golden value

`encoder_fingerprint_is_deterministic_and_logged_at_attach`
(`detchannel.rs:859`) asserts `wire_encoder_fingerprint() ==
wire_encoder_fingerprint()` — i.e. purity, which is trivially true for a function
of no inputs. It does NOT pin the value, so an accidental encoder change (the
very thing the fingerprint exists to detect) would silently pass this test while
changing the emitted record. Consider asserting against a frozen golden u64 (a
"golden_canonical_bytes"-style guard), with a comment that a deliberate
wire-format bump updates both the golden and the DHILOG version. Note the test
name claims "...and_logged_at_attach" but the body never exercises the attach
emit path — the actual attach+record-count coverage lives in m1_acceptance; the
name overpromises slightly.

### S-4 (nq5): `Vec::with_capacity(... * 28)` undercounts now that leaves are 28 bytes — verify, then leave a note

`cpuid_leaves_hash` reserves `leaves.len() * 28` (`config.rs:53`). Each leaf is
now 7 x u32 = 28 bytes via `encode_into`, so the capacity hint is exactly right
(it was `* 28` in the old cpuid.rs impl too, which was already correct since the
old impl also hashed 7 fields). No bug — just confirming the `28` is the intended
"7 fields x 4 bytes" and not a stale "6 fields" leftover. Worth a one-word inline
comment `// 7 u32 fields` so the next reader doesn't have to recompute it.
