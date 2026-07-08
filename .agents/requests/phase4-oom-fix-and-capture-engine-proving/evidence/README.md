# Capture-engine real-image proof — bead `ncn7` (Phase-4 request item 5)

The Phase-3 capture engine (`CaptureSpec`/`ExtractRange` → packed
`feature_bytes` + `fb_lz4`) proved end-to-end against the **real**
reference-workload image on BOTH capture surfaces. Committed hashes +
specs only — no raw guest/game-derived bytes (the game image is the
operator's private ROM). The full local run (logs, raw evidence dir)
lives on the lab machine under `target/capture-proof-1783477731/`.

Test: `crates/dh-worker/tests/capture_engine_real_image.rs`
(`--ignored` lab lane; invocation in its module doc).
Run: 2026-07-08, `test result: ok. 1 passed`, 16.93 s.

## Revs

| What | Value |
|---|---|
| worker (determinism-hypervisor) | `b1eba73` (proof run) |
| image bundle | reference-workload `1ea7292`, `dist/workload-image-0.1.0` |
| bzImage sha256 | `a7096dbd14289a910320a20bccc9fb21cdfea5b64de54582300f7632942ee057` |
| initramfs.cpio.zst sha256 | `79802eb1f3385326e934358d42841ac479e6ba87c2b45578c8aacff23b23d8f3` |
| expected-regions.toml sha256 | `9eba43dc54a847ec34c8c81892fb5d568cc73dca2590c261134ff5e33cf31df7` |
| feature map | reference-workload `feature-maps/demo-game.yaml` (PLACEHOLDER offsets — values meaningless; byte-plumbing is the proof) |
| game image blake3 | `e02849845005d9d34fa3245d98fa59116a0245ed0136b496dbd2defebdc203ac` |
| READY snapshot state_hash | `8cfdcc15ae320047c9608a9edc18d8ec82922be5cd60899cf3ef60644ad85c6f` |

## Manifest served by the real guest at proof time (proven)

`wram` 131072 / `framebuffer` 229376 (`xrgb8888-256x224-stride1024`) /
`meta` 4096 — all `layout_version = 1`. Matches
`expected-regions.toml`.

## Checks — all green

- **(a)** 12-range compiled extraction list (9 demo-map features + 3
  edge probes, 591 bytes total): capture `feature_bytes` bit-match
  independent `ReadGuestMemory` reads (which delegate to
  `detguest-host` `Channel::read_region`) of the same
  `(region, offset, len)` at the same paused boundary; packing is
  request order (proven by a reversed-order re-capture whose per-range
  bytes still match).
- **(b)** `fb_lz4` decodes to exactly 229,376 bytes (D7 geometry
  256×224 stride 1024, `fb_info` confirmed), non-black
  (`fb_all_zero=false`), and equals an independent full-region read of
  `framebuffer`.
- **(c)** a restored child (fresh slot, zero instructions executed)
  returns bit-identical `feature_bytes` (blake3 `032757f9…` == parent)
  and framebuffer for unchanged state.
- **(d)** a mismatched `layout_version = 2` is rejected
  `FAILED_PRECONDITION` on **both** surfaces (Run and TakeSnapshot);
  proven good version = **1**.

## Cost (advisory, not a gate)

Method: TakeSnapshot **with**-vs-**without** capture delta,
`seal_input_log=false`, 100 iterations, client-side RPC time, 2-core
lab host. with-capture p50 45.7 ms / p95 74.9 ms; no-capture p50
43.8 ms; **capture-attributable delta p50 ≈ 1.9 ms**.

Caveat, read before citing: this delta is a noisy *upper bound*, not a
clean capture cost. Each iteration lz4-compresses the full 229 KB
framebuffer, and the surrounding TakeSnapshot machinery dominates
(~44 ms) with heavy p95 contention on a 2-core box. It sits just above
scorer M4's 1.5 ms p50 budget — flagged for a cleaner isolation in a
follow-up bead (feature-only capture without the framebuffer will be
far cheaper; the packed feature payload here is only 591 bytes).

## Scope

Capture under a **concurrent `RunWithFrameCapture` stream is OUT OF
SCOPE and unproven** — do not infer it from this proof.

See `capture-samples.jsonl` for the per-capture hash table.
