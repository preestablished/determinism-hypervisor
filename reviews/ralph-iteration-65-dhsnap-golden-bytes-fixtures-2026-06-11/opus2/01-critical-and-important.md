# Critical & Important Findings

**None.**

I attempted to break this change on every skeptic angle in the brief and found no
Critical or Important issue:

- **Bytes match the spec.** Independent Python parse of both fixtures against
  API.md §4 passed on every field: header magic/version/count/pad, section header
  layout, spec-table tag order, and zeroed alignment + section `_pad`. No surprise
  bytes.
- **Hashes are reproducible.** A throwaway test using the crate's own `blake3` dep
  recomputed both pinned constants exactly. The pins are real, not stale.
- **Minimal fixture is independently derivable.** `v1_minimal.dhsnap` equals
  `ContainerWriter::new().finish()` byte-for-byte (16 B, count 0) and parses to
  zero sections — confirmed by hand-packing the expected header in Python.
- **gitattributes lands.** `git check-attr binary` returns `binary: set` for both
  fixtures, and `git ls-files --eol` shows `i/-text w/-text attr/-text` (treated as
  binary, no EOL munging). File modes are `100644` (non-executable).
- **Anti-laundering note present and on par with bp9.** The module doc and the
  hash-constant doc-comment both warn that touching the constants and the fixtures
  in the same PR is laundering a format break; wording mirrors
  `dh-inputlog/tests/golden.rs`.
- **No drift.** The diff is exactly the bead: `.gitattributes` (+1), `Cargo.lock`
  (+1, only `blake3` under `dh-snapshot`), `Cargo.toml` (+2), two fixtures,
  `golden.rs` (+193). Nothing unrelated.

## On the VCPU copy-paste-drift angle (the brief's key question)

The builder writes `(0u8..200).map(|i| i ^ 0xA5)` and the parse test asserts the
**same** expression. Taken alone, the round-trip leg (`build == fixture`) plus the
reader-half assert would *both* shift together if someone typo'd the expression
(e.g. `^ 0xA4`), silently re-pinning the wrong bytes.

This hole **is closed**, but only by the independent leg: `KITCHEN_SINK_BLAKE3` is a
literal constant. Change the expression and the writer's bytes change → the
checked-in fixture no longer matches → `kitchen_sink_fixture_is_frozen`'s hash
assert fails (assuming no same-PR regen, which the doc forbids). So:

- The duplicated expression in `kitchen_sink_fixture_parses_to_pinned_sections` is a
  **readability convenience, not an independent pin** — it does not strengthen the
  freeze, and it does not weaken it either, because the hash constant is the anchor.
- The `build_kitchen_sink() == fixture` assert plus the literal hash constant are
  what actually hold the line. Both are present. The freeze is sound.

This is a correctness *clarification*, not a defect — captured as a non-blocking
suggestion in `02-suggestions.md` (consider a comment, or a single shared `const`
for the VCPU pattern, to make the dependency on the hash leg explicit).
