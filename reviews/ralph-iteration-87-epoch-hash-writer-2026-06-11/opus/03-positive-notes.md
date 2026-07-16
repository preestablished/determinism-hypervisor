# Positive Notes

## P1 — The payload shape matches the frozen reader decode byte-for-byte

`epoch_hash()` writes `epoch_index` LE @0..8 then `chain_value` @8..40 (dhilog.rs:36-39), exactly
what the reader decodes (`reader.rs:183-186`: `epoch_index: u64at(0)`, `chain_value: p[8..40]`).
The 40-byte total is also what the validation table demands (`reader.rs:539`). Writer, validator,
and decoder agree — no negotiation, no drift.

## P2 — Zero call-site churn via the delegation shim

`run_segment` becomes a one-line delegation to `run_segment_with_epochs(..., &mut Vec::new())`
(runctl.rs:209-211). Every existing caller keeps working untouched, the throwaway `Vec` allocates
nothing meaningful for callers that don't care, and the new capability is opt-in. This is exactly
the right way to add a sink without a churn blast radius.

## P3 — The exactly-once invariant is enforced structurally, not by discipline

`finish()` has no `epoch_sink` parameter (runctl.rs:412-419), so it is *type-level impossible* for
the stop path to double-sink. Combined with `already_hashed` guarding the double-link, the
"exactly one link and one sink per coinciding stop/epoch boundary" property is structural rather
than something a future editor could accidentally violate. Excellent design choice.

## P4 — The freeze policy was respected and is proven by passing tests

The golden module (`tests/golden.rs`) explicitly excludes EPOCH_HASH / FLAG_EPOCH_HASHES from the
v1.0 freeze "no writer emission until M5". Because the fixtures never call `epoch_hash()`,
`wrote_epoch_hash` stays false and the serialized bytes are unchanged — confirmed by
`cargo test -p dh-inputlog` passing all 29 unit + golden tests after the change. The writer was
added without a format-version bump precisely because it cannot touch frozen bytes.

## P5 — The live test is a genuine end-to-end byte proof, not a mock

`epoch_hashes_flow_from_quantum_to_sealed_log` (recording.rs:558-) runs a real KVM landing loop
with `epoch_len=30_000` so one 100k quantum crosses three epochs, asserts the sink indices
`(1,30k),(2,60k),(3,90k)`, asserts the chains are nonzero, then seals, re-parses with the real
`LogReader`, and asserts the decoded EPOCH_HASH bodies match the sink byte-for-byte AND the header
flag is set. It exercises the entire producer path (sink → `log_epoch_hashes` → `epoch_hash` →
`seal` → reader) rather than stubbing any boundary.

## P6 — Crisp, spec-anchored doc comments at every new surface

The new `epoch_hash`, `run_segment_with_epochs`, `log_epoch_hashes`, and both sink sites carry
short comments that cite the relevant spec sections (§3.3, §8.5) and — importantly — explain the
*negative* space: why final-pause links at non-epoch boundaries are deliberately NOT sinked (they
travel in `END.end_state_hash`). That non-obvious exclusion is exactly the kind of thing future
readers need spelled out, and it is.
