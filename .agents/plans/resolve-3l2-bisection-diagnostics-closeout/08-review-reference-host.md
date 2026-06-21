# Reference Host Review

Reviewer: Ampere

## Findings

1. KVM-focused tests can pass by returning early when KVM or dirty rings are
   unavailable. The plan now adds hard preflight commands and runs service
   tests with `DH_REQUIRE_KVM_TESTS=1`.

2. The initial optional Linux gate was useful but mislabeled. The plan now
   distinguishes documented Linux fixture/READY gates from the supporting
   worker `replay_engine linux_boot_once` VerifyReplay smoke.

3. The Beads/Git closeout sequence was not safe enough. The plan now pulls and
   rebases before final validation, commits intended files before Beads
   evidence, records the actual `git rev-parse HEAD`, and then pushes Beads and
   Git.

4. Code-change cases need usual CI gates. The plan now requires clippy, build,
   and workspace tests when implementation changes code.

5. CLI validation was under-scoped. The plan now uses `cargo test -p dh-cli
   bisect` to cover parser, request, and rendering behavior.

## Checks Run By Reviewer

The reviewer did not edit files. They verified:

- `bd show determinism-hypervisor-3l2` still reports the parent blocked.
- All seven dependency beads are closed.
- `/dev/kvm` is readable and writable.
- `dh-workerd --preflight` passed.
- `ci/check-determinism-class.sh` matched the lock.
- `cargo fmt --check` passed.
- The staged M9 `linux_fixture_contract` passed with the plan's artifact paths.

## Assessment

The plan is operationally safe after the accepted edits. Closeout must still be
performed by a coding agent against current state, not assumed from this review.
