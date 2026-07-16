# Suggestions (non-blocking)

### S-1 — Add an explicit `*.dhilog binary` entry to `.gitattributes`

**File:** `.gitattributes` (currently only has `ci/determinism-class.lock` and `ci/*.sh` LF entries).

I checked: `git check-attr` / `git ls-files --eol` already report the fixtures as `i/-text w/-text` (git's content auto-detection saw the NUL bytes and is treating them as binary), so today there is **no** CRLF-mangling risk and this is not a defect. But auto-detection is heuristic and content-dependent; a degenerate future fixture (e.g. an all-printable-ASCII tiny log) could be misclassified as text on a `core.autocrlf=true` Windows checkout and silently corrupted, which is the classic golden-file failure. A one-line belt-and-suspenders entry makes the binary contract explicit and survives content changes:

```gitattributes
# DHILOG golden fixtures are byte-exact; never EOL-normalize.
*.dhilog binary
```

Cheap insurance for files whose entire value is byte-for-byte stability.

### S-2 — Assert `r.aux().count()` / `r.canonical().count()` on the kitchen sink

The parse test checks the full `kinds` vector but never exercises the `canonical()` / `aux()` partition on the rich fixture (only `minimal` checks `canonical().count() == 0`). Add `assert_eq!(r.canonical().count(), 6)` and `assert_eq!(r.aux().count(), 5)` (PAD_SET×2, DEV_EVENT×3, NET_RX = 6 canonical; ENTROPY, TIMER_FIRE, SDK_EVENT, FRAME_MARK, END = 5 aux) to freeze the AUX-flag classification of every kind — currently `rflags.AUX` per record is only indirectly frozen via the byte hash, not asserted through the reader's partition API that M6 consumers use.

### S-3 — Name the magic offsets the test re-derives, or pin the raw header window

`kitchen_sink_fixture_parses_to_expected_structure` trusts `LogReader` to surface header fields but never independently pins a raw header byte window. Since this file is the §3.1 freeze, consider one direct `assert_eq!(&fixture[0..16], b"DHILOG\x00\x01\x00\x01\x00\x00\x03\x00\x00\x00")` so the magic/version/header_len/flags prefix is frozen against *both* a reader regression and a writer regression independently (defense in depth; today only the BLAKE3 hash guards it against writer drift, and nothing guards the reader's header parse offsets).

### S-4 — Document the regen→hash-update workflow gotcha

`load_or_regen` (`:172-184`) writes new bytes when `DHILOG_REGEN_GOLDEN` is set, but the two `*_BLAKE3` consts are hand-maintained. After a regen the hash assertion will still fail until someone manually copies the new digest. The module doc says "Regenerate ... into new file names" for a new version, which is right, but a maintainer doing a legitimate v1.1 regen will hit a confusing failure. A one-line note — "after regen, update the `*_BLAKE3` consts from the test's failure output" — or better, have the regen branch print the freshly-computed hash, would save a debugging cycle.

### S-5 — Consider a frozen-unsealed (crash-artifact) fixture

The minimal fixture pins the degenerate *sealed* log (zero entropy_seed, zero fingerprint, zero end_snapshot). §3.4.4 makes `flags.SEALED == 0` a first-class, security-relevant state ("MUST NOT be replayed"), and `reader.rs` has a dedicated `NotSealed` error path. There is no golden fixture pinning an unsealed log's rejection. Not in scope for *format freeze* (an unsealed log has no stable `body_hash`/`end_state_hash` by definition, so it can't be a byte-pinned writer fixture), but a small hand-rolled `v1_unsealed.dhilog` asserting `LogReader::parse(..) == Err(ReadError::NotSealed)` would freeze that rejection contract. File as a follow-up if desired.
