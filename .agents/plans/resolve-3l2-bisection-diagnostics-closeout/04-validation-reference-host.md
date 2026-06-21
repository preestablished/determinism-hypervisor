# Validation And Reference Host Evidence

This repository is currently on the Linux/KVM reference host. Use that fact.

## Fast Compile And Formatting

Run after any edits:

```bash
cargo fmt --check
cargo test -p dh-inputlog bisection
cargo test -p dh-worker bisection
cargo test -p dh-worker verify_replay_divergence_mapping_is_honest_about_bisection
cargo test -p dh-cli bisect
```

If code changes, also run the normal CI-style gates:

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
cargo test --workspace
```

If no code changes are needed, `cargo test --workspace` is still required
before closeout; clippy/build are optional but useful if time permits.

## Reference Host Preflight

Run these before the KVM-backed focused tests:

```bash
test -c /dev/kvm && test -r /dev/kvm && test -w /dev/kvm
cargo run -p dh-worker --bin dh-workerd -- --preflight
bash ci/check-determinism-class.sh
```

Failure here means this reference host is not in the expected state. Do not
close `3l2` unless the failure is understood and resolved.

## Focused Parent Acceptance Tests

Run these even if no code changes are needed:

```bash
DH_REQUIRE_KVM_TESTS=1 cargo test -p dh-worker verify_replay_rpc_streams_divergence_for_semantically_bad_log -- --nocapture
DH_REQUIRE_KVM_TESTS=1 cargo test -p dh-worker verify_replay_rpc_streams_done_for_bisection_checkpoint_log -- --nocapture
DH_REQUIRE_KVM_TESTS=1 cargo test -p dh-worker verify_replay_rpc_streams_bisection_divergence_with_checkpoint_evidence -- --nocapture
DH_REQUIRE_KVM_TESTS=1 cargo test -p dh-worker verify_replay_rpc_rejects_invalid_bisection_checkpoint_gap -- --nocapture
cargo test -p dh-worker --test replay_engine lapc_verify_replay_bisection_reports_lapic_reg_diff_on_mutation -- --nocapture
```

These are KVM-backed on this host. Treat any skip output, dirty-ring
unavailability, or KVM unavailability as a closeout failure.

Run the lower-level field-population tests as well:

```bash
cargo test -p dh-worker rip_mismatch_produces_postcard_reg_diff
cargo test -p dh-worker page_hash_mismatch_reports_page_index
cargo test -p dh-worker page_hash_mismatches_are_limited_to_first_64_indices
```

## Workspace Gate

Run:

```bash
cargo test --workspace
```

If this fails only in unrelated long-running or host-specific tests, capture
the exact failure and rerun the strongest relevant subset. Preferred closeout
is a full workspace pass.

## Linux Artifact Gates

The parent bead is not specifically a Linux artifact gate, but this is the
reference host and staged M9 artifacts are expected. Run the documented Linux
fixture/READY gates with the same artifact environment:

```bash
DH_M9_ALLOW_SKIP=0 \
DH_M9_BZIMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/bzImage \
DH_M9_INITRAMFS=/home/infra-admin/.cache/dh-m9/reference-workload/initramfs.cpio \
DH_M9_BASE_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/base.img \
DH_M9_GAME_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/game.img \
DH_M9_IMAGE_CACHE=/home/infra-admin/.cache/dh-m9/image-cache \
cargo test -p determinism-tests --test linux_fixture_contract -- --ignored --nocapture

DH_M9_ALLOW_SKIP=0 \
DH_M9_BZIMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/bzImage \
DH_M9_INITRAMFS=/home/infra-admin/.cache/dh-m9/reference-workload/initramfs.cpio \
DH_M9_BASE_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/base.img \
DH_M9_GAME_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/game.img \
DH_M9_IMAGE_CACHE=/home/infra-admin/.cache/dh-m9/image-cache \
cargo test -p determinism-tests --test linux_ready --release -- --ignored --nocapture
```

Also run this supporting worker Linux VerifyReplay smoke if time permits:

```bash
DH_M9_ALLOW_SKIP=0 \
DH_M9_BZIMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/bzImage \
DH_M9_INITRAMFS=/home/infra-admin/.cache/dh-m9/reference-workload/initramfs.cpio \
DH_M9_BASE_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/base.img \
DH_M9_GAME_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/game.img \
DH_M9_IMAGE_CACHE=/home/infra-admin/.cache/dh-m9/image-cache \
cargo test -p dh-worker --test replay_engine linux_boot_once --release -- --ignored --nocapture
```

The worker `linux_boot_once` command is supporting Linux VerifyReplay evidence,
not a substitute for the focused bisection tests above.

## Evidence To Record

The Beads closeout comment should include:

- Whether code changes were needed after the audit.
- Exact focused test commands and results.
- `cargo fmt --check`.
- preflight and determinism-class checks.
- `cargo test --workspace`.
- No-skip Linux/KVM commands run on the reference host.
- The final commit SHA.
