# Action Items

## Critical

_None._

## Important

- [ ] **I-1 — Pin every record body through the reader's typed view.**
  In `crates/dh-inputlog/tests/golden.rs`, function `kitchen_sink_fixture_parses_to_expected_structure`, add typed-view assertions for the currently-unpinned records: `records[0]` (PadSet), `records[2]` and `records[3]` (the detchannel `DEV_EVENT`s — assert `device_id`, `event_type`, and the full `data` slice including ring_id/new_prod/new_cons sub-encoding), `records[4]` (`PIO_ANSWER` DEV_EVENT), `records[6]` (`Entropy { len:64, digest8:0x0102_0304_0506_0708 }`), `records[8]` (`SdkEvent { stream:5, len:256, digest8:0x1111_2222_3333_4444 }`), `records[9]` (`FrameMark { frame_index:42 }`). Snippet in `01-critical-and-important.md` §I-1; verify the `EVENT_*`/`DEVICE_ID_DETCHANNEL` constant names against the writer's `dhilog.rs` exports. Rationale: legs (1) hash-pin and (2) writer re-serialization fully freeze the *writer*, but the *reader* decode (`reader.rs:157-205` `body()`) — the layer `bik`/`pee` consume — is only spot-pinned (3 of 10 bodies); a reader-offset regression on the unpinned kinds passes all three current legs.

- [ ] **I-2 — Make the module-doc freeze claim true (or soften it).**
  `crates/dh-inputlog/tests/golden.rs:69-72` asserts "the reader parses the fixtures to the expected structure. Any layout change breaks all three." This is false for reader-side layout changes given the partial pinning. Preferred: land I-1 so the claim holds. Fallback if I-1 is deferred: reword to "(3) the reader parses the fixtures and a representative body of each record class decodes correctly; full per-record reader pinning tracked in `<follow-up-bead>`." Do not ship a true-sounding claim the tests don't back, since two downstream beads trust this contract.

## Suggestions

- [ ] **S-1** — Add `*.dhilog binary` to `.gitattributes`. Git auto-detects the current fixtures as binary (verified via `git ls-files --eol` → `-text`), so no present risk, but an explicit entry survives future content changes and `core.autocrlf=true` Windows checkouts. One line.
- [ ] **S-2** — In the kitchen-sink parse test, assert `r.canonical().count() == 6` and `r.aux().count() == 5` to freeze each kind's `rflags.AUX` classification through the partition API M6 uses (today only indirectly frozen via the byte hash).
- [ ] **S-3** — Add a direct raw-header-window assertion (`&fixture[0..16] == b"DHILOG\x00\x01\x00\x01\x00\x00\x03\x00\x00\x00"`) so the §3.1 magic/version/header_len/flags prefix is frozen against a reader header-parse regression independently of the BLAKE3 hash.
- [ ] **S-4** — Document the regen→hash-update step (the `*_BLAKE3` consts are hand-maintained; a legitimate v1.1 regen fails the hash assert until the const is updated). Either a doc note or have the regen branch print the computed digest.
- [ ] **S-5** — (Optional follow-up bead) Add a hand-rolled `v1_unsealed.dhilog` and assert `LogReader::parse(..) == Err(ReadError::NotSealed)` to freeze the §3.4.4 crash-artifact rejection contract. Out of scope for the byte-freeze itself (unsealed logs have no stable hash) but valuable for the reader's security contract.
