# Package 05 — `i74w`: M9 record/replay corpus manifest re-baseline (feasibility-first)

Bead: `determinism-hypervisor-i74w`
Filed: `.agents/requests/phase4-oom-fix-and-capture-engine-proving/04-resolution.md`
(2026-07-07): "M9 record-replay corpus manifest unrunnable: baselined on the
pre-real-emulator initramfs; needs a reviewed re-baseline".

## THIS PACKAGE STARTS WITH A FEASIBILITY CHECK — it may end by documenting a gate

The re-baseline may be blocked on a real post-READY reference-workload fixture:

- `.agents/plans/resolve-m9-post-ready-reference-workload/` (the plan that
  defines the replacement-fixture contract) has **no EXECUTED banner** at HEAD
  — grep confirmed while drafting.
- Per the closeout context, the currently staged `DH_M9_INITRAMFS` is believed
  to be the M2 smoke image (boots to READY, then terminates).
- Countervailing records: `.agents/plans/resolve-4s9-27-linux-m5-corpus/`
  describes a deterministic post-READY workload (FRAME_MARKs, frame-0 pv-blk
  IO), and the checked-in manifest itself shows post-READY content
  (`frame_counter=5`, `dhilog_records=18`, `run_until=frame_budget:5`). And
  M9 final acceptance (`resolve-4s9-35-final-m9-acceptance`) was completed.

The records disagree, so **decide empirically on the reference host** — never
by trusting one record over another, and never by weakening the test to accept
boot-to-READY only (explicit prohibition inherited from the M9 plans).

## Grounded facts

- Manifest: `crates/dh-worker/tests/fixtures/record_replay_corpus/m9_linux_post_ready/expected.txt`
  — baselined once at commit `a6b940d` (2026-06-20), never updated since. Pins
  blake3 hashes for bzImage / initramfs / base image / game image /
  determinism-class lock, machine-config hash, READY+END snapshot refs, DHILOG
  hash, END state hash, frame counter, pv-blk checksum, every `EPOCH_HASH`.
- Reverify test: `linux_m5_record_replay_post_ready_corpus_reverifies`
  (`crates/dh-worker/tests/m5_record_replay.rs:123`).
- Regen test: `regenerate_m9_rr_corpus_manifest_for_reference_host`
  (`m5_record_replay.rs:158`); exact invocation with `DH_WORKER_REGEN_M9_LINUX_RR_CORPUS=1`
  and the `DH_M9_*` env in the fixture's `README.md`
  (reference host `infra-control` only; artifacts under
  `/home/infra-admin/.cache/dh-m9/reference-workload/`).
- Fixture manifest header rule (emitted into `expected.txt` by
  `m5_record_replay.rs`): "Re-baseline only with reviewed M9 fixture, host, or
  hash-contract changes." A re-baseline is a **reviewed** change — it goes
  through the standard review workflow, not a drive-by regen commit.

## Step 1 — Feasibility check (reference host, read-only)

Before any long KVM boot, run the established reference-host preflight:
`.agents/plans/resolve-4s9-35-final-m9-acceptance/02-reference-host-preflight.md`
(host identity/determinism-class checks; stop the actions runner service
before long KVM boots and restart it afterwards).

1. Hash the staged artifacts and diff against `expected.txt`:

   ```bash
   b3sum /home/infra-admin/.cache/dh-m9/reference-workload/{bzImage,initramfs.cpio,base.img,game.img}
   # compare to bzimage_blake3 / initramfs_blake3 / base_image_blake3 /
   # game_image_blake3 in expected.txt
   ```

2. Determine what the staged initramfs **is**: the pre-real-emulator (M2
   smoke / early workload) image, or the real-emulator reference-workload
   fixture satisfying `.agents/plans/resolve-m9-post-ready-reference-workload/`'s
   contract (post-READY execution, FRAME_MARKs, `/dev/vdb` IO, boot.toml
   control/region entries). Check the reference-workload repo's dist/release
   notes and hashes; do not guess from file size. Concrete lead: the
   2026-07-08 capture proof (`capture_engine_real_image.rs`, phase4 request
   item 5) demonstrably ran the **real image** on this host from the dist
   bundle `reference-workload/dist/workload-image-0.1.0/` — so a real dist
   bundle existed there, making Case B (artifacts present, re-baseline
   feasible) likelier than the "expected" Case C. Check that dist path first.
3. Run the reverify test as-is and record the exact failure mode (or pass):

   ```bash
   DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux \
   DH_M9_BZIMAGE=... DH_M9_INITRAMFS=... DH_M9_BASE_IMAGE=... DH_M9_GAME_IMAGE=... \
   DH_M9_IMAGE_CACHE=/home/infra-admin/.cache/dh-m9/image-cache \
   cargo test -p dh-worker --test m5_record_replay --release \
     linux_m5_record_replay_post_ready_corpus_reverifies -- --ignored --nocapture
   ```

## Step 2 — Branch on what Step 1 found

### Case A — staged artifacts match `expected.txt` and reverify passes

`i74w` no longer reproduces (the manifest is runnable). Close it with the
green reverify output and the hash comparison as evidence, noting the filing
predates later M9 acceptance work.

### Case B — real-emulator fixture IS staged/available, hashes differ from `expected.txt`

The re-baseline is **feasible**. Scope:

1. Regen on the reference host with the README's exact command
   (`DH_WORKER_REGEN_M9_LINUX_RR_CORPUS=1 ... regenerate_m9_rr_corpus_manifest_for_reference_host`).
2. Diff `expected.txt` — only pinned values may change; the key set is
   test-enforced ("M9 Linux expected.txt key set changed" assertion).
3. Immediately re-run the reverify test against the regenerated manifest —
   twice, to prove it is stable, since flakiness here is a determinism signal
   (P0 if seen: stop and file, do not re-regen until it passes).
4. Commit manifest + a bead note listing old/new artifact hashes. This is a
   reviewed change: run the standard review pass before commit.
5. Close `i74w` with the evidence.

No engine/production code changes are in scope. If reverify fails for an
engine reason after a clean regen — including the case where the staged
hashes MATCH `expected.txt` but reverify still fails deterministically —
that is a new P0-candidate determinism finding — file it, link it from
`i74w`, stop. `i74w` is never closed on a failing reverify.

### Case C — only the M2 smoke image is staged; no real post-READY fixture exists (the expected gated case)

Do **not** regen against the smoke image (that would "re-baseline" onto the
very fixture `i74w` complains about — fake progress). Instead:

1. Write the gate down: annotate `i74w` (bd note) with: feasibility checked on
   <date>/<host>, staged initramfs blake3 <hash> identified as the M2 smoke
   image, re-baseline blocked on execution of
   `.agents/plans/resolve-m9-post-ready-reference-workload/` (no EXECUTED
   banner) / delivery of the post-READY reference-workload fixture.
2. If the beads DB supports it, add the dependency:
   `bd dep add determinism-hypervisor-i74w <blocking-bead>` where the blocking
   bead is the M9 post-READY fixture bead if one exists (check `bd show` /
   the plan's `06-bead-handoff.md`); otherwise the bd note carries the link.
3. Leave `i74w` open. Record the same gate statement in this plan directory as
   an EXECUTED-note (one paragraph appended to this file is enough) so the
   next agent doesn't redo the feasibility check blind.

## Acceptance

- Case A: reverify test green (output captured), hash comparison recorded,
  bead closed.
- Case B: regenerated `expected.txt` committed after review; reverify test
  green **twice in a row** on the reference host; hygiene gates green
  (`cargo test --workspace --all-targets`, clippy `-D warnings`, CI-shaped
  `cargo fmt --check` — the manifest is a fixture, so these should be
  untouched); bead closed with old/new hashes.
- Case C: gate documented on the bead and in this plan dir; no repo changes
  besides the documentation; bead left open. This outcome **passes** the
  package — documenting a real gate is the deliverable.
- All cases: the disposition must also annotate bead `jyo7` (fixture-era
  Linux corpus staleness — it overlaps `i74w`): fold it into the outcome or
  record "open, out of scope because X" (see 00-overview's scope section).

## Failure guidance

- **Ambiguous fixture identity** (hashes match neither the manifest nor any
  known reference-workload release): stop and ask the user / annotate the
  bead — do not re-baseline onto an unidentified image.
- **Reverify diverges only sometimes**: determinism P0. File immediately with
  both logs; everything else in this package pauses.
- **Reference host unavailable**: the package cannot run anywhere else
  (README: "Refresh the manifest only on the reference host"). Document and
  stop.
