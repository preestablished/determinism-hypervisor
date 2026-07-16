Critical:

- crates/dh-worker/benches/perf_gates.rs:82
  The benchmark still called the old fork_slot signature, breaking `cargo check -p dh-worker --benches`.
  Status: fixed at crates/dh-worker/benches/perf_gates.rs:90.

Important:

- crates/dh-worker/src/fork_engine.rs:135
  The engine treated `Some([0; 32])` as a fresh zero-seeded stream, even though the public contract says all-zero means continue.
  Status: fixed by filtering all-zero seeds before reseeding at crates/dh-worker/src/fork_engine.rs:135, with a regression in crates/dh-worker/tests/fork_engine.rs:164.

