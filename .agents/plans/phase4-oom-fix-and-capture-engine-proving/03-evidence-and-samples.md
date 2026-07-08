# 03 — Durable Sample Evidence: What To Record And Where

The point of the sample set: reference-workload's corpus request
(`../reference-workload/.agents/requests/phase4-real-capture-corpus-fast-follow/`,
work item 1: "the engine-side proof is the hypervisor's round-2
request; consume it, don't rebuild it") and state-scorer M1 build on
this as the interface evidence. It must let a downstream reader answer:
*what spec was sent, what came back, at which revs, and how was it
independently checked* — without rerunning the lab.

## Two Evidence Locations

1. **Raw/local (untracked):** `target/capture-proof-<date>/` at the
   proof-commit checkout — full logs, raw captured bytes, timing CSVs,
   a README narrating the run (mirrors `target/oom-evidence-2026-07-07/`).
   Cite the path in the resolution; per repo convention it stays on the
   lab machine.
2. **Durable/committed:** a compact sample set inside the request dir,
   e.g. `.agents/requests/phase4-oom-fix-and-capture-engine-proving/evidence/`
   — hashes and specs only, **never raw guest bytes** (the game image
   is the operator's private ROM; reference-workload's redaction rules
   treat game-derived bytes as private — commit BLAKE3/SHA-256 hashes
   of captured bytes, not the bytes themselves; the D7 framebuffer
   frame is also game-derived, so hash it too).

## The Committed Sample Set — Contents

`evidence/README.md` — narrative + the rev table:

| Field | Value to record |
|---|---|
| worker commit | `git rev-parse HEAD` of this repo at proof time |
| worker build | release binary, exact `cargo` invocation |
| image bundle | `workload-image-0.1.0`, reference-workload commit, sha256 of `bzImage`/`initramfs.cpio.zst`/`expected-regions.toml` |
| region manifest | the three regions + sizes + `layout_version` actually served by the guest at proof time |
| feature map | `reference-workload/feature-maps/demo-game.yaml` @ refwork commit (placeholder offsets — noted as such) |
| snapshot ref(s) | the READY snapshot id(s) the runs started from |

`evidence/capture-samples.jsonl` — one line per proven capture, shape:

```json
{"sample": 1, "surface": "run", "spec": {"ranges": [{"region": "wram", "layout_version": 1, "offset": 1947, "len": 2}, ...]},
 "at_icount": 123456789, "feature_bytes_len": 14, "feature_bytes_blake3": "...",
 "crosscheck": "detguest-host reads bit-identical", "fb_lz4_len": 4321,
 "fb_decoded_len": 229376, "fb_decoded_blake3": "..."}
```

Cover at minimum: one `Run`-with-capture sample, one
`TakeSnapshot`-with-capture sample, one restored/forked-child sample
(same spec, recorded as identical to its parent's hashes), and the
negative `layout_version` rejection (record the request, the gRPC
status/code, and the message).

`evidence/cost.md` — per-capture cost numbers: spec-compile, extract,
pack (and lz4-encode) timings, p50/p95 over enough iterations to be
stable (≥100 captures is cheap once the VM is up). State the
measurement method (where the timers sit — service-side around the
capture call is fine) and the host. One honest table; this feeds scorer
M4's 1.5 ms p50 budget conversation, it is not a gate.

## Extraction-List Design For The Samples

Compile from `demo-game.yaml` over the real manifest: take each
feature's `(region, offset, type-width)` as an `ExtractRange` — e.g.
`room_id` → (`wram`, 0x079B, 2), `player_x` → (`wram`, 0x0AF6, 2),
`health` → (`wram`, 0x09C2, 2) — all with `layout_version: 1`. Add
edge-probing ranges the feature map won't give you: offset 0 len 1,
tail range ending exactly at `wram` size 131072, a multi-hundred-byte
range, and a `meta`-region range. (An out-of-bounds range's rejection
is worth one extra negative line if the engine validates bounds —
record actual behavior either way.)

Because the offsets are placeholders, the *values* are meaningless —
the proof is byte-identity between the engine's packed output and
independent reads of the same ranges, plus packing order (request
order, per `proto/hypervisor.proto:99`).

## What NOT To Put In The Committed Evidence

- Raw `feature_bytes`, decoded framebuffer pixels, or any game-derived
  byte content — hashes only.
- The operator ROM hash is already on record in refwork's flow; don't
  duplicate private metadata here.
- Anything implying capture-under-concurrent-stream was proven — state
  the opposite explicitly in the README.
