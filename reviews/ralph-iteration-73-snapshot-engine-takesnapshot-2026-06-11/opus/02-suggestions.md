# Suggestions (non-blocking)

## S1 — Trusting a `bool` attestation for agenda-empty: acceptable for v1, but consider a harder token later

**File:** `crates/dh-worker/src/snapshot_engine.rs:130, 183-185`

The §8.1 invariant "pending agenda MUST be empty — TakeSnapshot fails otherwise" is enforced
by `boundary.agenda_empty: bool`, set by the caller. A bool is trivially fakeable: a caller
bug that hard-codes `true` defeats the gate silently and produces a *plausible but wrong*
snapshot (the worst failure mode for a determinism platform — it round-trips and replays
divergently later).

**Position:** for v1 this is **acceptable** and I would not block on it. The engine sits
behind run-control, which owns the agenda; the engine cannot reach the agenda handle without
a layering inversion, and the `SlotState::Paused` re-check already catches the most common
"snapshot while running" mistake. But the bool is the weakest link, and it is worth a tracked
follow-up to replace it with something the caller cannot trivially forge — e.g. passing a
`&FinalStop` / agenda handle and having the engine call `.is_empty()` itself, or a
zero-sized witness type minted only by the agenda's drain path
(`struct AgendaDrained(())` that only `agenda::drain()` can construct). That moves the
guarantee from "caller asserts" to "caller proves". The `BoundaryState` doc already calls it
an attestation, which is honest; the suggestion is to harden the proof, not the wording.

## S2 — No byte-determinism test (two identical-state snapshots → same ref)

**File:** `crates/dh-worker/tests/snapshot_engine.rs`

The module header and `build_dhsnap` make a strong, load-bearing claim: "the container is
part of the snapshot-ref preimage, so section order is fixed HERE" and "two engines with
different bus layouts produce identical bytes for identical state." Nothing tests it. The
FULL test verifies *sections* and tag *order*, but never asserts that a second
`take_snapshot` of the same state yields the **same `snapshot_ref`** (the whole point of the
canonical ordering). A `assert_eq!(ref_a, ref_b)` for two snapshots of identical state — and
a companion `assert_eq!` across two buses registered in different *insertion* orders but
covering the same device set — would lock down the determinism guarantee that justifies the
`sort_by_key` machinery. This is the single highest-value test to add. (Tracked as an action
item.)

## S3 — No multi-device-ordering test (bus with BLKO/EVTC)

**File:** `crates/dh-worker/tests/snapshot_engine.rs:437-447`

`test_bus()` registers PAD, CLOCK, ENTROPY, SERIAL → tags PADD, CLKD, ENTR, SERL. The
canonical sort only exercises CLKD/PADD/SERL on the device tail. The interesting reorderings
— EVTC (KNOWN_TAGS idx 7, device_id 0x0001) sorting *after* PADD but *before* BLKO, and BLKO
(idx 8) before NETL/SERL — are untested. A bus including a detchannel (EVTC) and a blk
overlay (BLKO) would prove the `KNOWN_TAGS`-position sort actually reorders relative to
base-address order (in the current test the base order already happens to match canonical
order, so the sort is a no-op and the assertion can't distinguish a correct sort from no sort
at all). Worth adding to make the determinism claim non-vacuous.

## S4 — `total_pages` uses integer `/` while `DirtyPageSet::new` uses `div_ceil`

**File:** `crates/dh-worker/src/snapshot_engine.rs:191`

`let total_pages = slot.mem_bytes / PAGE_SIZE;` (truncating) vs
`DirtyPageSet::new` → `mem_bytes.div_ceil(PAGE_SIZE)`. These agree only when `mem_bytes` is a
page multiple — which it always is (the manifest layer rejects non-multiples). So this is
benign today. But the two computations of "page count" for the same slot diverging by
convention is a latent trap if a non-aligned `mem_bytes` ever slips through. Either assert
`debug_assert!(slot.mem_bytes % PAGE_SIZE == 0)` at entry, or use the same `div_ceil` for
symmetry. One line, defensive.

## S5 — `EngineError::Kvm`/`Codec`/`Store` carry `String`, not the source error

**File:** `crates/dh-worker/src/snapshot_engine.rs:156-168`

The error variants stringify their causes (`format!("{e:?}")`). That's fine for a top-level
orchestrator boundary, but it erases the structured error type, so callers can't match on
"was this a transient store-unavailable (retry) vs a corrupt-snapshot (don't retry)?" The
retry-vs-fail decision is exactly the kind of thing run-control will want to make. Consider
`#[source]` boxed errors (or at least a `retryable: bool`) on the `Store` variant so the
caller's retry loop can distinguish `Unavailable` from `BatchBlake3Mismatch`. Non-blocking;
revisit when the retry loop is wired.
