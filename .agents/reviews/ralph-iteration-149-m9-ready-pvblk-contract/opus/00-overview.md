# Overview

Branch: `ralph/iteration-149-m9-ready-pvblk-contract`
Base: `main` (`f2628efd530e418b50662aa5f21a6b89ef00d12a`)
Head: `9d1c9fdce876a489bc3450151267232cae2ae229`
Date: 2026-06-18
Reviewer: Claude Opus
Verdict: REQUEST_CHANGES

Summary: This branch adds M9 bzImage boot foundations, Linux cmdline canonicalization, BzImage image-cache resolution tests, artifact docs, and an opt-in Linux entry smoke test. The main issue is in the bzImage loader: it copies the compressed payload subrange to the Linux load address, then jumps to `load + 0x200`. A real bzImage expects the protected-mode kernel image, starting at `(setup_sects + 1) * 512`, to be loaded there.

Stats: 1 commit, 18 files changed, 2590 insertions, 8 deletions.

Review inputs inspected: `git diff main...HEAD`, changed-file list, commit log, and the relevant changed files with line numbers.

Tests run:
- PASS: `git diff --check main...HEAD`
- PASS: `cargo test -p dh-vmm linux_bzimage --lib`
- PASS: `cargo test -p dh-vmm bzimage_plan_writer_copies_linux_payloads --lib`
- PASS: `cargo test -p dh-vmm bzimage_cmdline --lib`
- PASS: `cargo test -p dh-worker image_resolver::`
- PASS: `cargo test -p dh-worker proto_map::`
- EXPECTED ENV FAILURE: `cargo test -p determinism-tests linux_entry_smoke --test linux_boot_trace -- --ignored` failed because `DH_M9_*` artifacts are not staged.
