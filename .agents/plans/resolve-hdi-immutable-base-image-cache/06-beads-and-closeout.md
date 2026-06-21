# Beads And Closeout

## Beads Start

This plan intentionally makes the blocked design decision. The implementation
agent should move the bead out of `BLOCKED` before editing:

```bash
bd show determinism-hypervisor-hdi
bd update determinism-hypervisor-hdi --status open \
  --append-notes "Accepted bounded copy-once base-image cache contract: resolver caps base-image length, copies and hashes bytes into owned immutable FileBase backing, then pv-blk reads from owned bytes instead of the cache inode."
bd update determinism-hypervisor-hdi --claim
```

Do not create child beads unless implementation uncovers a genuinely separate
piece of work. This bead is small enough to close in one focused change.

## Expected Git Changes

Expected files:

- `crates/dh-vmm/src/blkfile.rs`
- `crates/dh-worker/src/image_resolver.rs`
- `crates/dh-worker/src/service.rs` only if service tests/error mapping need edits
- `docs/decisions/base-image-cache-contract.md`
- `.agents/plans/resolve-hdi-immutable-base-image-cache/` only during the
  planning session

Do not stage:

- M9 artifact bytes
- image-cache entries
- raw logs under `target/`
- unrelated local worktree changes

## Closeout Evidence Comment

Before closing `determinism-hypervisor-hdi`, post a concise evidence comment:

```bash
bd comment determinism-hypervisor-hdi --stdin <<'EOF'
Implemented bounded copy-once base-image cache contract.

Contract:
- MAX_BASE_IMAGE_BYTES = 512 MiB.
- Worker resolver rejects oversized base-image cache entries before hashing.
- Worker resolver copies verified base-image bytes into owned immutable FileBase backing.
- Worker resolver uses fallible allocation for owned base-image bytes.
- pv-blk no longer reads from the mutable cache inode after verification.

Validation:
- cargo fmt --check
- cargo test -p dh-vmm blkfile
- cargo test -p dh-worker image_resolver
- cargo test -p dh-worker --lib
- cargo test --workspace
- <Linux/KVM no-skip command and result>
EOF
```

Then close:

```bash
bd close determinism-hypervisor-hdi \
  --reason "Base-image cache contract is explicit, bounded, and enforced by owned verified pv-blk backing with tests and reference-host evidence."
```

## Required Push Sequence

Repository instructions require both Beads and Git state to be pushed before
handoff:

```bash
git status --short --branch
git pull --rebase
bd dolt push
git push
git status --short --branch
```

The final status must be clean and up to date with origin.

If `git pull --rebase` changes code after validation, rerun affected tests. If
only docs or plan files replay, record the tested code SHA and final docs SHA in
the Beads comment.

If `bd dolt push` or `git push` fails, resolve and retry. Do not leave this
contract fix stranded locally.
