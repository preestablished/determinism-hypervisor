# Tests

Executed:

- `cargo test -p dh-vmm linux_lapic`
  - Passed: 6 tests.
  - Covered `LocalApic` reset values, MMIO read/write, interrupt queue helpers, timer/x2APIC rejection, APIC base rejection, and the synthetic `DeviceRail::service_exit` LAPIC path.

- `cargo test -p determinism-tests --test linux_boot_trace trace_tests`
  - Passed: 3 tests.
  - Covered JSON/acceptance helper behavior only, not live Linux boot.

- `cargo test -p dh-worker --test replay_engine replay_reproduces_the_recording_bit_identically`
  - Passed: 1 test.
  - Existing replay coverage; not LAPIC-specific.

Attempted but not useful:

- `cargo test -p determinism linux_boot_trace::trace_tests`
  - Failed because the package is named `determinism-tests`.

- `cargo test -p dh-worker replay_engine`
  - Built successfully but matched 0 tests.

- `cargo test -p dh-worker --test replay_engine replay_segment_reseals_successful_log`
  - Matched 0 tests.

- `cargo test -p dh-worker --test replay_engine replay_rejects_vectored_inputs_until_injection_wired`
  - Matched 0 tests.

Coverage gaps:

- No test exercises the production `dh-workerd` `service_exit_with_detchannel` path with LAPIC MMIO/MSR exits.
- No test proves LAPIC state survives multiple worker `Run` calls in one slot.
- No test proves LAPIC state survives TakeSnapshot/RestoreSnapshot/fork/bisection checkpoints.
- The ignored Linux boot trace was not run with real `DH_M9_BZIMAGE`/`DH_M9_INITRAMFS` artifacts.
- No negative test covers side-effectful ICR writes.
