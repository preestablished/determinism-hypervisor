Critical:

- crates/dh-worker/benches/perf_gates.rs:82
  The benchmark still called the old fork_slot signature, so `cargo check -p dh-worker --benches` failed with E0061. Fix: add `None` before `&mut bus_c`.
  Status: fixed at crates/dh-worker/benches/perf_gates.rs:90.

Important:

- crates/dh-worker/src/service.rs:413
  Fork remains unimplemented, so the new helper is not yet enforced at the RPC boundary. This is acceptable for this bead because it is contract and engine prework before the blocked lifecycle wiring bead wires Fork; rfv/8kb still own runtime RPC service wiring.
  Status: accepted as non-blocking scope boundary.

