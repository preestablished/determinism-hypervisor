# 01 — Entry Conditions And Artifact Staging

Do this before any capture work. Everything here is verification and
environment setup; no code changes.

## 1. Re-Verify The Entry Condition (Cheap, Do It Anyway)

The request gates item 5 on reference-workload's regenerated image
(`refwork-gp9`). As of 2026-07-08 this HOLDS, but re-verify at
execution time:

```bash
cd ../reference-workload && bd show refwork-gp9   # expect: closed
ls dist/workload-image-0.1.0/
# expect: bzImage  initramfs.cpio.zst  expected-regions.toml  boot.toml
#         harness.toml  workload-image.yaml  determinism.last_green  README.md
```

If `gp9` is somehow reopened or the dist bundle is gone, stop and
resolve item 5 as still-waiting (the request's own fallback) — do not
substitute the old cached fixtures.

Record the revs you will cite in evidence:

```bash
git -C ../reference-workload rev-parse HEAD          # image-producing repo rev
git rev-parse HEAD                                    # worker rev under test
sha256sum ../reference-workload/dist/workload-image-0.1.0/{bzImage,initramfs.cpio.zst,expected-regions.toml}
```

## 2. Confirm The Region Manifest Is What The Proof Assumes

`../reference-workload/dist/workload-image-0.1.0/expected-regions.toml`
(schema_version 1, schema_owner guest-sdk) publishes exactly three
regions, all `layout_version = 1`, all required, none writable:

| region | size | format |
|---|---|---|
| `wram` | 131072 | — |
| `framebuffer` | 229376 | `xrgb8888-256x224-stride1024` |
| `meta` | 4096 | — |

Check (b)'s "D7 229,376-byte frame" is this framebuffer region.
Check (d)'s mismatched version means anything ≠ 1 (use e.g. 2). If the
file at execution time differs from this table, the proof still runs —
but update the extraction list and evidence to match reality and note
the drift in the resolution.

## 3. Stage The M9 Artifacts (Known-Good Recipe)

The dh-worker lab-lane tests locate the guest image via `DH_M9_*`
environment variables (see `crates/dh-worker/tests/common/` for the
authoritative names and lookup logic). The staging recipe that is known
to work — and the trap to avoid:

- `DH_M9_BZIMAGE` → `../reference-workload/dist/workload-image-0.1.0/bzImage`
- `DH_M9_INITRAMFS` → the **decompressed** dist initramfs:
  `zstd -d -k initramfs.cpio.zst` somewhere stable (e.g.
  `~/.cache/dh-m9/dist-0.1.0/initramfs.cpio`). It must contain
  `usr/bin/refwork-harness` — verify with
  `cpio -it < initramfs.cpio | grep refwork-harness`.
- `DH_M9_BASE_IMAGE` / `DH_M9_GAME_IMAGE` → the cached
  `~/.cache/dh-m9/reference-workload/base.img` / `game.img` (these are
  fine to reuse; the game image is the operator ROM block device).
- `DH_M9_IMAGE_CACHE` → `~/.cache/dh-m9/image-cache`.

**TRAP:** `~/.cache/dh-m9/reference-workload/initramfs.cpio` is the OLD
M9 contract fixture (autostarts `/opt/m9-refwork-contract`); the worker
tests' `m9_linux_ready_snapshot` helper REJECTS it. Only the dist
initramfs (autostarts `/usr/bin/refwork-harness`) is valid.

Smoke the staging before writing anything new — an existing `--ignored`
lab-lane test that boots the image proves the environment, e.g.:

```bash
cargo test -p dh-worker --release --test rss_regression -- --ignored --nocapture
# or a faster M9 test if one boots the same snapshot; see crates/dh-worker/tests/
```

(You don't need the full multi-minute RSS run to pass staging — any
test that reaches the READY snapshot proves the artifacts are right.
Pick the cheapest one in `crates/dh-worker/tests/` that uses the
`m9_linux_ready_snapshot` common helper.)

## 4. Build Precondition: The Leak Fix

All capture runs must be on `c0337ab` or later (current `main` HEAD
`d8abd74` includes it — fine). This is why item 5 was sequenced after
the fix: a capture session is a long Run. No segment-bounding is needed
on this build; if for any reason you must run on a pre-`c0337ab` build
(don't), segment-bounded Runs per the bridge's `fbd38d1` pattern are
mandatory.

## 5. Working Notes

- Run lab-lane tests `--release`; debug-mode guest boots are painfully
  slow and some perf-adjacent gates assume release.
- Don't run capture tests while other heavy suites are running on the
  host (standing repo lesson: hash/determinism-sensitive results flake
  under parallel-suite load).
- Evidence dir convention: `target/capture-proof-2026-MM-DD/` at the
  proof-commit checkout, mirroring `target/oom-evidence-2026-07-07/`
  (a README with the narrative plus raw logs/CSVs). `target/` is
  untracked — the durable copy of the *sample set* goes in the request
  dir per `03-evidence-and-samples.md`.
