# Plan: Phase-4 OOM Fix And Capture-Engine Proving — Remaining Scope

> **EXECUTED 2026-07-08** — item 5 (capture-engine proof) DONE. Bead
> `ncn7`; proving test `crates/dh-worker/tests/capture_engine_real_image.rs`
> ran green against the real image (`test result: ok. 1 passed`, all
> checks a–d). Evidence:
> `.agents/requests/phase4-oom-fix-and-capture-engine-proving/evidence/`;
> resolution addendum `04a-item5-resolution.md`; refwork pointer
> `../reference-workload/.agents/requests/phase4-real-capture-corpus-fast-follow/04-engine-proof-available.md`;
> cost follow-up `uyhu`. Items 1–4 were already done (fix `c0337ab`);
> `9f3x` stays open pending the bridge's redeploy confirmation.

Plan for `.agents/requests/phase4-oom-fix-and-capture-engine-proving/`.
Written 2026-07-08 for a coding agent to execute.

## Read The Request State First — Most Of It Is Already Done

The request has five work items. **Items 1–4 (the OOM leak fix) were
executed on 2026-07-07** and handed back in the request dir's
`04-resolution.md`:

- Bead `determinism-hypervisor-9f3x` filed and IN_PROGRESS; fix commit
  `c0337ab` (agenda materialization was the root cause); evidence in
  `target/oom-evidence-2026-07-07/`; RSS ceiling+plateau guard in
  `crates/dh-worker/tests/rss_regression.rs`; hash-chain bit-identity
  proven against a pre-fix recording; bridge `9bx` answered
  ("unbounded, build `c0337ab`+"); `38b6` annotated (fix DISJOINT from
  the M4 epoch-hash design). Follow-up beads `iar8`, `i74w` filed.
- The corresponding execution plan lives at
  `.agents/plans/run-with-frame-capture-memory-leak-oom/` — do **not**
  redo any of it.

**Do not re-execute items 1–4.** What remains, and what this plan
covers:

1. **Item 5 — prove the capture engine on real data.** Its entry
   condition has since been MET: `refwork-gp9` is closed and the
   regenerated real workload image exists at
   `../reference-workload/dist/workload-image-0.1.0/` (verified
   2026-07-08: `bzImage`, `initramfs.cpio.zst`,
   `expected-regions.toml`, `boot.toml`, `harness.toml`,
   `determinism.last_green`). The `04-resolution.md` "WAITING on
   refwork-gp9" note is now stale — executing item 5 is the bulk of
   this plan.
2. **The items-1–4 closeout tail.** Bead `9f3x` stays open pending
   bridge confirmation (their `l1w` close or a handback note after
   redeploying `c0337ab`+ and re-running their eqb validation) or the
   phases track's `05-verification.md`. Check for either; close `9f3x`
   if one has arrived, otherwise leave it with an updated note. This is
   a check, not new engineering.
3. **Update the resolution.** Append the item-5 evidence to the request
   dir (see `04-closeout.md`) so the handback no longer says "waiting".

## What Item 5 Is (And Is Not)

Prove, against the real workload image, that the Phase-3 capture engine
(`CaptureSpec`/`ExtractRange` → packed `feature_bytes` + `fb_lz4`) works
end-to-end on **both** capture surfaces (`Run`-with-capture and
`TakeSnapshot`-with-capture), with four checks:

- (a) `feature_bytes` match independent `detguest-host` reads of the
  same (region, offset, len) ranges bit-for-bit;
- (b) `fb_lz4` decodes to the D7 229,376-byte frame
  (`xrgb8888-256x224-stride1024` per the image's
  `expected-regions.toml`);
- (c) the same capture spec over a restored/forked child returns
  identical bytes for unchanged state;
- (d) the negative case: a mismatched `layout_version` in the
  extraction list is rejected (`FAILED_PRECONDITION` per
  `proto/hypervisor.proto:104`) — record the proven version (the real
  image publishes `layout_version = 1` for all three regions).

Plus: record per-capture cost (spec-compile + extract + pack) — one
number, not an acceptance bar (scorer M4's 1.5 ms p50 budget sits
downstream); and record a small durable sample set (spec, byte hashes,
revs) that reference-workload's corpus request and state-scorer M1
consume.

**Not in scope** (the request is explicit):

- Corpus production/packaging/exporter tooling — that is
  reference-workload's round-2 request
  (`../reference-workload/.agents/requests/phase4-real-capture-corpus-fast-follow/`).
  You prove the engine; they produce the corpus.
- Capture under a *concurrent* `RunWithFrameCapture` stream — unproven,
  explicitly out of scope; say so in the resolution so nobody assumes
  otherwise.
- Round-1's items (already resolved separately, commit `8558443`), the
  M4 latency pipeline, and anything on the open beads `jyo7`/`i74w`
  (fixture-era gate re-baselines — adjacent, not this request).

## Plan Files

| File | Contents |
|---|---|
| `01-entry-and-staging.md` | Re-verify entry conditions, stage the real-image artifacts, build precondition |
| `02-capture-engine-proof.md` | The proving harness: both surfaces, checks (a)–(d), cost measurement |
| `03-evidence-and-samples.md` | The durable sample-evidence format downstream consumers read |
| `04-closeout.md` | Resolution updates, bead handling, cross-repo notifications, session close |

## Ground Rules For The Implementer

1. **Track it in beads.** File a bead for item 5 before writing code
   (P1, feature/task — it gates Phase 4's entry), linked in notes to
   `9f3x` and the request dir. The request dir is not a tracker. Do not
   reuse `9f3x` for capture work — it is the OOM incident bead and
   closes on bridge confirmation.
2. **Prove, don't patch — unless proving fails.** The expected shape of
   this work is a lab-lane integration test plus evidence. If a real
   engine defect surfaces, file a bead for it, fix it, and note it in
   the resolution — a defect found here is the point of proving.
3. **Determinism verification discipline** (standing repo lessons): if
   you end up touching anything hash-adjacent, never chain
   `cargo test ; git merge` — gate on exit codes; verify
   determinism-sensitive changes with 3+ consecutive full workspace
   runs. Pure test/evidence additions don't need the 3× rule.
4. **The old cached initramfs is a trap.** `~/.cache/dh-m9/reference-workload/initramfs.cpio`
   is the OLD contract fixture and is REJECTED by the worker tests.
   Stage from `dist/workload-image-0.1.0/` per `01-entry-and-staging.md`.
5. **Session close protocol is mandatory** (CLAUDE.md): quality gates,
   bead updates, `git pull --rebase && bd dolt push && git push`,
   verify up-to-date with origin.
