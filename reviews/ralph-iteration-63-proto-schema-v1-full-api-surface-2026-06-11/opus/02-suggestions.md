# Suggestions (non-blocking)

## S1 — Proto PAUSED_S comment is now slightly stale w.r.t. the local API.md

`proto/hypervisor.proto:408` reads:

> `// API.md §2.8 wrote PAUSED here — an oversight of the same rule its own FAULTED_S works around (codegen rejects it).`

The local API.md §2.8 has *already* been edited to `PAUSED_S` in this same branch (API.md:441), so the in-repo spec no longer "writes PAUSED here." The past-tense framing now describes the *upstream* (unsynced) copy, not the file a reader will open. Minor wording drift only. Consider:

```proto
// PAUSED_S follows FAULTED_S: proto enum values use C++ scoping (siblings of
// the package, not the enum), so SlotState values must not collide with
// StopReason's PAUSED/FAULTED. The original API.md §2.8 wrote a bare PAUSED
// (the same oversight FAULTED_S already corrects); the local spec copy is
// patched to PAUSED_S, upstream sync tracked by bead veu.
```

This also surfaces the bead-veu reference inline, which currently lives only in the prompt/PR context.

## S2 — The two PAUSED_S comments are paraphrases, not verbatim — and the header says comments "are ported … transcribed exactly"

The file header (`proto/hypervisor.proto:8-11`) claims the inline comments are "ported from API.md §2" and "transcribed exactly," and asks to "keep both in sync." The PAUSED_S note is the one place where the proto comment (lines 406-409) and the API.md comment (lines 443-445) deliberately diverge in wording. That's defensible — they're explaining the same rule from each file's vantage point — but it's worth a one-line acknowledgement so a future "verbatim" audit doesn't flag it as drift. Either align the two texts, or soften the header to "ported (PAUSED_S note intentionally differs per file)."

## S3 — `icount_hi: 1124` in the Divergence round-trip sits exactly on the spec bound

`crates/dh-proto/src/lib.rs:156-157` uses `icount_lo: 100, icount_hi: 1124` → `hi - lo == 1024`, which is exactly the API.md §2.7 bound (`hi - lo ≤ 1024`). Harmless for a shape/round-trip test (the test asserts nothing about the bound), but if a reader treats the fixture as exemplary it's pinned at the boundary. Optional: pick a comfortably-inside value (e.g. `icount_hi: 1100`) so the fixture reads as "a normal divergence window," or add a one-word comment that the test does not validate the bound.

## S4 — Consider one round-trip that crosses the `frame_budget = 8` discontinuity explicitly

The `RunRequest` test already exercises `FrameBudget(60)` (the field-8 arm) — good, that's the highest-risk oneof tag because it's non-contiguous. As a belt-and-suspenders pin against a future accidental renumber to a contiguous `6`, a tiny additional assertion that the encoded oneof tag for the `frame_budget` arm corresponds to field 8 (e.g. inspect that `encode_to_vec()` for `FrameBudget(1)` starts with the field-8 varint key `0x40`) would catch a silent tag change that a pure round-trip (which is internally consistent regardless of the number) cannot. Strictly optional — the proto text + protoc is the real guard here.
