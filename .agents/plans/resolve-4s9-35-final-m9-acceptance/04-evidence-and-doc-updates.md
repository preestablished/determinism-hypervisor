# Evidence And Documentation Updates

## Evidence Record

Create a concise evidence summary from the command transcripts. The summary
should be suitable for a Beads comment and for a short final docs addendum.

Required fields:

- UTC date and time range of the final suite.
- `tested_code_sha`: the Git commit SHA before the long suite started.
- `final_evidence_sha`: the Git commit SHA after docs/evidence updates are
  committed, if that differs from `tested_code_sha`.
- A statement that any difference between those SHAs is docs/evidence only,
  or a rerun note if code changed after testing.
- Hostname, kernel, CPU model, microcode.
- `ci/check-determinism-class.sh` result.
- `/dev/kvm`, `nasm`, `dh-workerd --preflight`, and slot-core preflight result.
- Artifact paths and BLAKE3 hashes for `bzImage`, `initramfs.cpio`,
  `base.img`, and `game.img`.
- `DH_M9_IMAGE_CACHE` path and the artifact cache keys present under it.
- A command/result table for every command in `03-acceptance-runbook.md`.
- Confirmation that filtered/ignored test transcripts showed expected named
  tests and did not report `0 tests`.
- Any GitHub workflow run links used as supporting evidence.

Keep raw logs under `target/` or another local scratch location. Do not commit
large command logs unless the user explicitly asks. The durable record is the
summarized evidence in Beads and docs.

## Suggested Evidence Table Shape

Use a table like this in the Beads comment:

```markdown
Final M9 acceptance evidence, <UTC date>.

Tested code SHA: `<tested_code_sha>`.
Final evidence/docs SHA: `<final_evidence_sha>` (<docs-only delta or same SHA>).

Host: <hostname>, Linux <kernel>, <cpu>, microcode `<microcode>`.
Determinism class: PASS, <n>/<n> keys matched.
Artifacts:
- bzImage `<hash>` at `<path>`
- initramfs.cpio `<hash>` at `<path>`
- base.img `<hash>` at `<path>`
- game.img `<hash>` at `<path>`
- image cache `<path>`
- image-cache keys `<hashes>`

| Gate | Command summary | Result | Key evidence |
|---|---|---|---|
| Workspace | `cargo test --workspace` | PASS | ... |
| Nanokernel Phase 1 | `cargo run -p dh-cli -- gate --runs 100` | PASS | 100/100 zero divergence |
| Linux Phase 1 | `DH_M9_ALLOW_SKIP=0 cargo run -p dh-cli -- gate --linux --runs 100 ...` | PASS | ready hash/config/post-ready hash |
| Linux timer | `linux_timer_determinism` | PASS | vector/list/final hash |
| Linux landing | `linux_landing_counting` | PASS | 100 exact targets |
| Linux M4 | `m4_transparency linux` | PASS | reg/page diffs |
| Linux M5 frames | `m5_frame_scheduling linux` | PASS | frame tables |
| Linux M5 pv-blk loopback | `m5_net_loopback linux` | PASS | checksum/dirty clusters |
| Linux M5 corpus | `linux_m5_record_replay_post_ready_corpus_reverifies` | PASS | epoch hashes/end hash |
| Linux worker API | `linux_worker_api` | PASS | CreateVm through VerifyReplay |
| Nanokernel M5/M7 | documented nanokernel commands | PASS | corpus/full/cross-slot |
| Linux M7 full | 1000-child Linux command | PASS | verified/divergence/unique/epoch counts |
| Linux M7 cross-slot | targeted cross-slot command | PASS | same-seed refs/logs match |
```

## Phase Docs

`4s9.35` may update these files with final evidence only:

- `docs/phase-1-exit-gate.md`
- `docs/phase-2-exit-gate.md`

Prefer a short final addendum over rewriting the existing dated producer
rollups. The existing M9 sections are useful history from the prerequisite
beads. Add a new section such as:

```markdown
## M9 final acceptance suite (<date>)
```

In `docs/phase-1-exit-gate.md`, summarize:

- workspace result if relevant to Phase 1;
- default nanokernel Phase 1 gate result;
- Linux Phase 1 CLI gate;
- Linux timer/IRQ determinism;
- Linux landing/counting;
- final artifact hashes and any expected difference from earlier Phase 1
  producer artifacts.

In `docs/phase-2-exit-gate.md`, summarize:

- Linux M4/M5 results;
- Linux M5 corpus reverify;
- Linux worker API result;
- nanokernel M5/M7 preservation result;
- Linux M7 full and cross-slot results;
- nightly-equivalent or GitHub nightly supporting evidence if used.

Do not alter `docs/upstream-divergences.md` unless final acceptance reveals a
new accepted drift that has gone through review. A test failure is not accepted
drift by itself.

## Beads Notes

Post the evidence summary before closing `4s9.35`:

```bash
bd comment determinism-hypervisor-4s9.35 --stdin <<'EOF'
<paste final evidence summary>
EOF
```

Then close with a specific reason:

```bash
bd close determinism-hypervisor-4s9.35 \
  --reason "Full M9 acceptance suite passed on the reference Linux/KVM host with no skip-enabled Linux evidence; final Phase 1/Phase 2, Linux M4/M5/M7, worker API, and nanokernel preservation evidence published."
```

After closing `4s9.35`, confirm whether the parent epic is complete:

```bash
bd show determinism-hypervisor-4s9
```

If all child beads are closed, add a parent evidence comment and close the
epic:

```bash
bd comment determinism-hypervisor-4s9 --stdin <<'EOF'
All M9 child beads are closed. Final acceptance evidence is recorded on 4s9.35.
EOF
bd close determinism-hypervisor-4s9 \
  --reason "All M9 child beads are closed and final M9 Linux/nanokernel acceptance evidence has been published."
```

If any child is unexpectedly open or blocked, do not close the epic. Record
the blocker in `4s9.35` or the relevant child bead.
