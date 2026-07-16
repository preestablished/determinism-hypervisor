# Positive Notes

## P1 — Three-layer invariant correctly identified and aligned

The change recognizes that the empty-frame invariant already existed at the
device (`net.rs:158` rejects `len == 0`; `net.rs:105` faults `tx_len == 0`) and
brings the writer and reader into agreement, rather than papering over the gap.
The `net.rs:61-64` comment now makes the three-layer agreement explicit and
cross-references bead 206. This is the right fix direction — close the codec
to match the device, not invent empty-delivery semantics.

## P2 — Defense-in-depth at the writer despite an unreachable path

`net_rx` is only ever called from `recording.rs:203`, *after* `apply_net_rx`
(line 201) has already rejected `len == 0`. The writer guard is therefore
unreachable in the live recording path — but adding it is exactly correct: it
makes the codec self-enforcing so any future caller (or a hand-built writer in
a test/tool) can't slip an unreplayable record past it. The reader half closes
the same invariant against hostile/corrupt logs.

## P3 — Format-freeze discipline respected

The golden v1 fixtures and their BLAKE3 pins are untouched, and the golden test
passes. The author correctly judged that tightening reader validation of a
*never-produced* degenerate record does not require a format-version bump — the
freeze pins valid-log decode, and no previously-valid fixture decodes
differently. The kitchen-sink NET_RX (`golden.rs:84`) is 5 bytes, comfortably
inside the new `1..=2048` window.

## P4 — Error-variant hygiene

`WriteError::EmptyNetRx` is added cleanly: the enum derives
`Clone, Copy, Debug, PartialEq, Eq` (`dhilog.rs:100`), so the `==` assertions
in the new test compile and are meaningful. No exhaustive `match` on
`WriteError` exists in the workspace, so the new variant introduces no
compile breakage — confirmed by a clean `cargo build --workspace`.

## P5 — Test coverage is precise

`net_rx_frame_bounds_at_the_writer` exercises the full boundary set: empty
(rejected, `EmptyNetRx`), 1 (the new floor, accepted), 2048 (the cap, accepted),
2049 (`PayloadTooLong`). The reader test flips the zero-length case to assert
the exact `ReadError::BadPayloadLayout { kind: KIND_NET_RX, seq: 0 }` shape
(the real output of `validate_kind` failure at `reader.rs:547`) and adds a
1-byte lower-bound acceptance case. Both halves of the codec are covered.

## P6 — Ledger entry #19 is rigorous

The entry matches the established `### #N` / **Found** / **Why** / `Old` / `New`
template, cites the originating review and bead, names the authority files, and
records the decision (forbid-at-codec vs invent-empty-delivery-semantics). The
`Old` quote is verbatim against `git show main:...API.md`.
