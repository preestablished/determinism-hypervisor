# Quality-Gate Closeout Tails

> **EXECUTED — 2026-07-16** on gate host `infra-control` (tier 2), HEAD
> `b4358a7`. Toolchain: rustc 1.97.1 / clippy 0.1.97 / rustfmt 1.9.0-stable
> (2026-07-14). Tier-1 corroboration: CI push run 29472974677 green at HEAD.
>
> - **Pkg 01 `mmra` CLOSED** — no longer reproduced: `cargo check` +
>   `cargo test --workspace --all-targets` green (67 suites, 771 passed,
>   0 failed, 32 ignored KVM lab-lane `--ignored`); fixed by `dd49ebf`.
> - **Pkg 02 `lynb` CLOSED** — clippy `--all-targets -D warnings` exit 0;
>   fixed by `2bca5d8`.
> - **Pkg 03 `jyp4` CLOSED** — CI-shaped `cargo fmt --check` exit 0; fixed
>   by `dd49ebf`. Packages 01–03: no code change, determinism obligation
>   vacuous.
> - **Pkg 04 `uyhu` CLOSED** — three-variant cost isolation landed
>   (test-side only, 8-angle review pass applied); feature-only delta
>   +50 µs p50 (quiet primary run; ≤ ~242 µs loaded) vs the 1.5 ms scorer-M4
>   budget. Evidence:
>   `.agents/docs/determinism-hypervisor/capture-cost-isolation.md`. Gates
>   re-run green on the changed tree (771/0/32, clippy 0, fmt 0).
> - **Pkg 05 `i74w` OPEN, gate documented** — real dist fixture staged but
>   reverify fails at HEAD on both artifact sets (old fixture: pre-boot
>   autostart contract; dist: epoch_len OVERSHOOT `642206698 > 642190000`,
>   identical to `jyo7`); blocked on `jyo7` (dependency already recorded).
>   EXECUTED note in `05-*.md`; both beads annotated.
> - Scope checks: `9f3x` untouched (waiting on bridge `l1w`); `jyo7` open,
>   annotated as the blocker; `38b6` open, deferred M4 pipeline, out of
>   scope. Runner-reservation caveat: kvm-intel runner service not pausable
>   (no passwordless sudo); no queued/in-progress runs during KVM windows.

Plan name: `quality-gate-closeout-tails`
Drafted: 2026-07-15 against HEAD `776a80f` ("Capture complete M8 acceptance
identity", clean, pushed).

## Goal

Close out the remaining hygiene/tail beads left open after M0–M9 + M8
verification-mode hardening were accepted. This is **verify-first closeout, not
feature work**: for each item, empirically reproduce the reported failure at
HEAD before changing anything. Several items appear to have been fixed already
by later commits (see grounding below) — in that case the deliverable is a
green gate run recorded as evidence and a bead closure, **not** a code change.

## Authority And Trackers

- The local beads DB is **stale**. Treat `.agents/plans/*` and
  `.agents/requests/*` records plus empirical reproduction at HEAD as
  authoritative for what is actually open. Use `bd show <id>` only to read
  notes and `bd close <id> -r "..."` to record disposition; run all `bd`
  commands serially, never in parallel batches.
- **Beads sync before any disposition.** Sync first: `git pull --rebase`, then
  the repo's beads sync per its `CLAUDE.md` (`bd dolt push` is the push side;
  pull/refresh the DB before reading it). Then reconcile `bd show` output for
  every in-scope bead against the `.agents` request records above before any
  close. Failure modes: if a bead is **missing locally** or **already
  closed**, do not force anything — record the disposition in the relevant
  `.agents` note instead and say so in the closure summary. If a serial `bd`
  command hits an embedded-Dolt exclusive-lock error, retry serially a bounded
  number of times per beads conventions; do not improvise raw Dolt commands.
- Beads in scope: `determinism-hypervisor-mmra`, `-lynb`, `-jyp4`, `-uyhu`,
  `-i74w`. Filed in:
  - `mmra`/`lynb`/`jyp4`: `.agents/requests/lease-semantics-doc-and-orphan-slot-warn/04-resolution.md`
    (2026-07-10, "Validation" section).
  - `uyhu`: `.agents/requests/phase4-oom-fix-and-capture-engine-proving/04a-item5-resolution.md`
    (2026-07-08, "Cost" section).
  - `i74w`: `.agents/requests/phase4-oom-fix-and-capture-engine-proving/04-resolution.md`
    (2026-07-07).

## Explicitly OUT of scope — do not touch

- **Bead `9f3x`** (RunWithFrameCapture OOM incident): the fix (`c0337ab`) is
  merged; the bead stays open **waiting on the rom-operator-bridge team's
  redeploy confirmation** (their bead `l1w` / a phases-track
  `05-verification.md`). That is another party's action. Do not close it, do
  not re-verify it, do not fold it into any package here. Tracked in
  `.agents/plans/phase4-oom-fix-and-capture-engine-proving/04-closeout.md`.
- Any optimization of capture cost (package 04 is measurement + documentation
  only; if the isolated cost exceeds budget, file a follow-up request).
- Any weakening of M9/Linux gates or `boot.toml`/READY contract preflights
  (package 05 is a feasibility check; if gated, document the gate).
- **Bead `jyo7`** (P1, fixture-era Linux corpus staleness — filed in
  `.agents/requests/phase3-snapshot-restore-no-frame-under-no-tick-take-two/09-followup-resolution.md`,
  still referenced open in
  `phase4-oom-fix-and-capture-engine-proving/00-overview.md:89` and
  `04-closeout.md:61`): verify its state via `bd show` after the beads sync.
  It **overlaps with `i74w`**, so package 05's disposition must annotate it —
  either fold it into the `i74w` outcome or record "open, out of scope
  because X" explicitly. Do not leave it silently unaccounted.
- **Bead `38b6`** (deferred M4 epoch-hash pipeline): verify state via
  `bd show` after sync; expected open but **out of scope** here — deferred M4
  pipeline work, tracked in its own filing record. Note that in the closure
  summary, nothing more.

## Environment Requirements

- The CI bar is Linux (x86_64 + aarch64 GitHub runners, plus the self-hosted
  `kvm-intel` box). This workspace has **path deps on sibling checkouts**
  `../control-plane`, `../guest-sdk`, `../snapshot-store` — they must sit next
  to the repo (they do in this tree: `/Users/punk1290/git/preestablished/`).
- Packages 01–03 (build/test/clippy/fmt gates) should be run on a Linux host
  matching CI; KVM-requiring tests self-skip without `/dev/kvm`. `nasm` is
  required (tests/nanokernel build.rs). A macOS aarch64 run is advisory only
  and must not be used to close a bead.
- Package 04 requires the **Linux/KVM reference host** (`infra-control`) with
  `DH_M9_*` pointing at the **dist bundle**
  `reference-workload/dist/workload-image-0.1.0/` — decompress
  `initramfs.cpio.zst`; it must carry `usr/bin/refwork-harness`. The capture
  test it extends (`crates/dh-worker/tests/capture_engine_real_image.rs`,
  module doc ~lines 54–59) explicitly **REJECTS** the old
  `~/.cache/dh-m9/reference-workload/initramfs.cpio` contract fixture — stage
  per that module doc, not the `~/.cache` layout.
- Package 05 requires the same reference host, but its object of study is the
  **`~/.cache` staging**: the M9 artifacts under
  `/home/infra-admin/.cache/dh-m9/reference-workload/` (see
  `.agents/plans/resolve-4s9-27-linux-m5-corpus/00-summary.md`).

## Where To Execute (packages 01–03)

This plan was drafted from a macOS checkout that cannot run the Linux gate
bar itself. Resolve the execution seam in this order:

1. **First-line evidence — CI at HEAD.** Check the green CI run for HEAD
   `776a80f` via `gh run list` / `gh run view`: the push run executed the
   fmt/clippy/build/test bar on both hosted lanes plus `kvm-intel`. Note the
   asymmetry: CI's `cargo test` lacks `--all-targets`, but CI clippy runs
   `--all-targets`, which covers the `mmra` compile surface (bench/test
   targets compile under clippy).
2. **Gate host for the superset.** `infra-control` is the acceptable gate host
   for the full `cargo test --workspace --all-targets` acceptance command
   (host identity: `.agents/plans/resolve-4s9-27-linux-m5-corpus/00-summary.md`;
   precedent: `resolve-4s9-35-final-m9-acceptance/00-summary.md` "assumes the
   implementation agent is on `infra-control`").
3. **Neither reachable.** Record advisory macOS results in this plan directory
   and **STOP — do not close beads.** A macOS run never closes a Linux-bar
   bead.

Precedence note: CI green at HEAD (tier 1) suffices to disposition packages
01–03; prefer the `infra-control` superset run (tier 2) whenever that host is
reachable. Packages' "on the Linux gate host" acceptance language means
tier 1 or tier 2 — never a macOS run.

## The CI Lint Bar (grounded from `.github/workflows/ci.yaml`)

Confirmed by reading `ci.yaml` at HEAD — use these exact shapes:

- **fmt**: NOT `cargo fmt --all` (that would format the sibling path deps).
  CI scopes to workspace members, fail-closed:

  ```bash
  set -euo pipefail
  members=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[].name')
  test -n "$members"
  cargo fmt --check $(printf -- '--package %s ' $members)
  ```

- **clippy**: `cargo clippy --workspace --all-targets -- -D warnings`
  (so `-D warnings` IS the repo's bar).
- **build**: `cargo build --workspace`.
- **test**: `cargo test --workspace` (CI does **not** pass `--all-targets` to
  `cargo test`; the bead reproductions used
  `cargo test --workspace --all-targets`, which is a strict superset — use the
  superset as the acceptance command so bench/test targets also compile).
- The nightly-drift workflow additionally runs the determinism regression
  suite: `cargo test -p determinism-tests --test regression --test
  counting_semantics --test counting_smoke` and the pad_echo corpus reverify.

## Determinism Safety

Repo convention: determinism bugs are P0; any touched execution-path code
requires re-running the determinism tests. **Packages 01–03 are expected to
touch no execution-path code at all** — they are verify-and-close, or at most
mechanical lint/format fixes. State this explicitly in each closure. If (and
only if) a real fix turns out to be needed in an execution-path file
(`crates/dh-vmm/src/runctl.rs` is execution path), re-run
`cargo test -p determinism-tests` on the KVM host before closing. Package 04
adds measurement only (a bench/instrumented test — no production code change);
package 05 changes at most a checked-in manifest fixture, never engine code.

## Grounding Notes (verified 2026-07-15 at HEAD 776a80f)

Findings that reshape the work — the three hygiene beads were filed 2026-07-10,
and commits on 2026-07-11 appear to have already fixed all three:

1. **`mmra`** — `tools/dh-cli/src/ops.rs:178` builds
   `proto::RestoreSnapshotRequest` and at HEAD it **already contains
   `baseline: None` (line 181)**. Commit `dd49ebf` ("Restore hypervisor CI
   compatibility", 2026-07-11) added exactly +1 line to `ops.rs`. The reported
   compile failure most likely no longer reproduces.
2. **`lynb`** — the three `clippy::unnecessary_lazy_evaluations` findings at
   `crates/dh-worker/src/m9_handoff.rs:1392–1406` were fixed by commit
   `2bca5d8` ("Fix hypervisor strict CI checks", 2026-07-11): the diff shows
   the exact `unwrap_or_else(|| …)` → `unwrap_or(…)` conversions at those
   lines. `grep unwrap_or_else crates/dh-worker/src/m9_handoff.rs` finds no
   remaining hits.
3. **`jyp4`** — `rustfmt --check --edition 2021` on
   `crates/dh-vmm/src/runctl.rs` and `crates/dh-worker/tests/rss_regression.rs`
   exits 0 at HEAD (local stable rustfmt, aarch64-apple-darwin). `dd49ebf`
   touched both files. Likely already clean.
4. **`uyhu`** — measurement seams confirmed: the 1.9 ms figure came from the
   with/without-capture TakeSnapshot delta in
   `crates/dh-worker/tests/capture_engine_real_image.rs` (~lines 546–660,
   `COST_ITERS = 100`). The engine is `capture_at_boundary` at
   `crates/dh-worker/src/service.rs:3363`: feature ranges via
   `channel.read_region` (591 packed bytes in the proof), framebuffer via
   `lz4_flex::compress_prepend_size` over a 229,376-byte region. A criterion
   bench harness already exists at `crates/dh-worker/benches/perf_gates.rs`
   (harness=false, x86-gated, KVM-skipping). Docs home:
   `.agents/docs/determinism-hypervisor/`.
5. **`i74w`** — corpus manifest is
   `crates/dh-worker/tests/fixtures/record_replay_corpus/m9_linux_post_ready/expected.txt`,
   baselined once at `a6b940d` (2026-06-20) and never updated. Regen command is
   in the fixture README (reference host only). The feasibility gate is real:
   `.agents/plans/resolve-m9-post-ready-reference-workload/` has **no EXECUTED
   banner** (grep confirmed), and `i74w` says the manifest was baselined on the
   pre-real-emulator initramfs. Note a tension in the records:
   `.agents/plans/resolve-4s9-27-linux-m5-corpus/` describes a deterministic
   post-READY workload and `expected.txt` shows post-READY content
   (`frame_counter=5`, 18 DHILOG records) — package 05 resolves this
   empirically by hashing the staged artifacts, not by trusting either record.

**Not confirmed (gaps):** (a) no full `cargo test/clippy` run was possible on
this macOS drafting host, so "already fixed" for 01–03 is inferred from source
+ git history, not from a green gate — that is exactly why each package leads
with verification; (b) current live bead states were not read (`bd` DB stale by
instruction); (c) whether the reference host's staged `DH_M9_*` artifacts today
match `expected.txt`'s hashes or the real-emulator image is unknown from this
checkout (`../reference-workload/dist/` does not exist here) — package 05's
first step checks it.

## Packages (execute in order)

| # | File | Bead | Shape |
|---|------|------|-------|
| 1 | `01-mmra-workspace-test-gate.md` | `mmra` | verify `cargo test --workspace --all-targets`; fix only if red |
| 2 | `02-lynb-clippy-m9-handoff.md` | `lynb` | verify clippy `-D warnings`; fix only the cited lints if red |
| 3 | `03-jyp4-rustfmt-drift.md` | `jyp4` | verify CI-shaped `cargo fmt --check`; format only if red |
| 4 | `04-uyhu-feature-capture-cost-isolation.md` | `uyhu` | measurement + `.agents/docs/` note (P2) |
| 5 | `05-i74w-m9-corpus-rebaseline-feasibility.md` | `i74w` | feasibility check; re-baseline only if unblocked, else document the gate |

1–3 are independent of each other but share the same gate runs — run the three
gates once, then disposition all three beads from that single run. 4 and 5 are
independent of 1–3 and of each other but both need the reference host.

## Done Criteria For The Whole Plan

- Beads `mmra`, `lynb`, `jyp4` closed with gate-run evidence (or, if a gate is
  genuinely still red, fixed minimally and then closed with the green run).
- Bead `uyhu` closed with a committed measurement + note, or explicitly left
  open with a filed follow-up request if the isolated feature-only cost
  exceeds the 1.5 ms p50 scorer budget.
- Bead `i74w` either closed with a reviewed re-baseline, or annotated with the
  documented gate and left open — no fake progress.
- `9f3x` untouched.
- Determinism regression suite green if any execution-path file changed
  (expected: none).
- Mark this plan's `00-overview.md` with an **EXECUTED banner** (date, gate
  host or CI run URL, rustfmt/clippy versions, pass/skip counts per package)
  and commit it — closure evidence must land in the authoritative `.agents`
  layer, not only in `bd close` reasons (house convention; see e.g.
  `run-with-frame-capture-memory-leak-oom/00-overview.md` line 3).
  Optionally append a short addendum to the two filing request dirs
  (`lease-semantics-doc-and-orphan-slot-warn/`,
  `phase4-oom-fix-and-capture-engine-proving/`) as the richer record.

## Session Close (mandatory, per repo CLAUDE.md)

Mirrors `.agents/plans/phase4-oom-fix-and-capture-engine-proving/04-closeout.md`
§5. This plan directory is currently **untracked** — committing it (plus any
executed-notes appended to the package files) is part of session close, not
optional.

```bash
git status && git add .agents/plans/quality-gate-closeout-tails/ <other files> && git commit
git pull --rebase
bd dolt push
git push
git status   # MUST show clean / up to date with origin
```

Run each as a separate, individually-checked command; a push is not done
until `git status` confirms it landed.
