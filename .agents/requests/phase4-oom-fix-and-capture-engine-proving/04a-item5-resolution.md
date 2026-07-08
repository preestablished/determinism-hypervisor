# Resolution Addendum — Item 5 (Capture Engine On Real Data) — 2026-07-08

Supersedes the "WAITING on `refwork-gp9`" note in `04-resolution.md`:
the entry condition is now MET (`refwork-gp9` closed;
`../reference-workload/dist/workload-image-0.1.0/` exists) and item 5
is **DONE**. Items 1–4 remain as handed back in `04-resolution.md`
(fix `c0337ab`); this file covers only item 5.

- **Bead id:** `determinism-hypervisor-ncn7` (P1, filed before code,
  linked to `9f3x` and this request dir). Follow-up filed: `uyhu`
  (P2 — cleaner per-capture cost isolation vs scorer M4's 1.5 ms
  budget; see cost note below).
- **Proof commit / test:** `crates/dh-worker/tests/capture_engine_real_image.rs`
  — an `--ignored` M9 lab-lane test (staging + invocation in its module
  doc). Ran green against the real image on 2026-07-08:
  `test result: ok. 1 passed`, 16.93 s, worker `b1eba73`.
- **No engine change was needed** — the Phase-3 capture engine works
  end-to-end against the real image as built. This item is a proof, and
  it passed.

## Surfaces And Checks (AC4)

Both capture surfaces proven — `Run`-with-capture and
`TakeSnapshot`-with-capture (`dh-worker/src/service.rs`, shared
`capture_at_boundary`):

- **(a)** a 12-range compiled extraction list (9 `demo-game.yaml`
  features + 3 edge probes; 591 packed bytes) returned `feature_bytes`
  bit-identical to independent `ReadGuestMemory`/`read_region` reads of
  the same `(region, offset, len)` at the same paused boundary; packing
  is request order (proven by a reversed-order re-capture on the
  TakeSnapshot surface whose per-range bytes still matched).
- **(b)** `fb_lz4` decoded to exactly 229,376 bytes (D7 geometry
  confirmed via `fb_info`), non-black, and equalled an independent
  full-region read of `framebuffer`.
- **(c)** a restored child (fresh slot, zero instructions) returned
  bit-identical `feature_bytes` (blake3 `032757f9…` == parent) and
  framebuffer.
- **(d)** a mismatched `layout_version = 2` was rejected
  `FAILED_PRECONDITION` on **both** surfaces; **proven good version = 1**
  (the guard protecting scorer M1 from a stale layout).

## Cost (advisory, not a bar)

Capture-attributable delta **p50 ≈ 1.9 ms** (method: TakeSnapshot
with-vs-without capture delta, `seal_input_log=false`, 100 iters,
2-core lab host). This is a noisy *upper bound* — it lz4-compresses the
full 229 KB framebuffer every iteration and the TakeSnapshot machinery
dominates. It lands just above scorer M4's 1.5 ms p50 budget, so `uyhu`
is filed to measure feature-only capture cost cleanly before scorer M4
depends on it (the packed feature payload is only 591 bytes; feature-
only will be far cheaper).

## Evidence

- Durable/committed (hashes + specs only, no game-derived bytes):
  `evidence/README.md` + `evidence/capture-samples.jsonl` in this dir
  (4 samples: run / take_snapshot / restored_child / negative, with
  blake3 hashes, the compiled spec, and the revs table).
- Raw/local (lab machine): `target/capture-proof-1783477731/`.

## Scope Honored

Capture under a **concurrent `RunWithFrameCapture` stream is OUT OF
SCOPE and unproven** — stated so nobody assumes otherwise. Corpus
production/packaging/exporter remains reference-workload's round-2
request (a pointer note was left in their request dir); this item
proved only the engine.

## `38b6` / Items-1–4 Tail (unchanged, restated)

- `38b6` annotation stands from the 2026-07-07 execution: the OOM fix
  is DISJOINT from the M4 epoch-hash pipeline; M4 stays deferred.
- `9f3x` (OOM incident bead) remains OPEN pending the bridge's redeploy
  confirmation (their `l1w` was still open at 2026-07-08 check; no
  `05-verification.md` from the phases track yet). Noted on the bead.

Awaiting the phases track's `05-verification.md`; the capture proof
re-runs cleanly from a fresh checkout with the staged `DH_M9_*` dist
artifacts (per `01-entry-and-staging.md`).
