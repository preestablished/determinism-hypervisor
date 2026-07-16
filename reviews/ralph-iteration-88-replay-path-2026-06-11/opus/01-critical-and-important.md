# Critical & Important Findings

## Critical

None. The core property is correctly implemented and the negatives are genuine.

---

## Important

### I1 — The EPOCH_HASH `Divergence` discards its structured detail; 1py will have nothing to report

**File:** `crates/dh-worker/src/replay_engine.rs:391-401`

When an epoch link mismatches, the sink returns `BoundaryError::Exit(format!(...))` carrying
the real `expected`/`got` hashes and the link `icount`. `run_to` then maps it back:

```rust
RunError::Boundary(BoundaryError::Exit(m)) if m.contains("EPOCH_HASH") => {
    ReplayError::Divergence {
        what: "EPOCH_HASH (see message)",
        at_icount: start,          // <-- quantum START, not the link icount
        expected: [0; 32],         // <-- real expected hash thrown away
        got: [0; 32],              // <-- real got hash thrown away
    }
}
```

Two problems compound here:

1. **String-matching the error channel is fragile.** `m.contains("EPOCH_HASH")` is the only
   thing distinguishing an epoch divergence from any other boundary error. The string lives in
   the sink (lines 371-383). If anyone rewords those messages — or another `BoundaryError::Exit`
   anywhere in the run loop happens to contain the substring "EPOCH_HASH" — the classification
   silently flips. A divergence could be misreported as `ReplayError::Run`, or a generic run
   failure misreported as a `Divergence`. For the product's core property, the divergence/no-
   divergence boundary should not hinge on a substring match.

2. **The structured detail is destroyed exactly where the consumer needs it.** Bead **1py**
   (the next consumer, VerifyReplay) is specified to report
   `Divergence{first_divergent_epoch, hashes}`. The information to populate that exists at the
   point of failure (in the sink) but is flattened to a string and then the structured fields
   are zeroed. `at_icount` is set to the quantum *start*, not the diverging link's icount, so
   even the location is degraded.

**Fix (the prompt's own suggestion, and the right one):** capture the divergence through a
side channel instead of the error string. A `Cell<Option<Divergence>>` (or
`Cell<Option<(u64, [u8;32], [u8;32])>>`) owned by `replay_segment` and written by the sink at
the mismatch point:

```rust
let epoch_div: Cell<Option<(u64 /*idx*/, u64 /*icount*/, [u8;32] /*exp*/, [u8;32] /*got*/)>>
    = Cell::new(None);
// in the sink, on mismatch:
epoch_div.set(Some((idx, icount, *e_value, value)));
return Err(BoundaryError::Exit("epoch divergence".into())); // just to abort the quantum
// in run_to's error map:
RunError::Boundary(_) if epoch_div.get().is_some() => {
    let (idx, ic, exp, got) = epoch_div.get().unwrap();
    ReplayError::Divergence { what: "EPOCH_HASH", at_icount: ic, expected: exp, got }
}
```

This removes the substring dependency AND hands 1py the real `(icount, expected, got)` it is
specified to surface. Worth doing now, before 1py is written against the degraded shape.

---

### I2 — `Divergence{what:"resealed log bytes"}` reports `expected = header.body_hash`, `got = [0;32]` — misleading and unactionable

**File:** `crates/dh-worker/src/replay_engine.rs:492-499`

When the reseal hammer fails (`resealed != log_bytes`), the engine returns:

```rust
ReplayError::Divergence {
    what: "resealed log bytes",
    at_icount: header.end_icount,
    expected: header.body_hash,    // the INPUT log's body hash
    got: [0; 32],                  // "byte-compare failed; the diff is in the logs"
}
```

The `expected`/`got` pair is meant to be a comparable hash pair for divergence tooling
(that is how every other `Divergence` uses it). Here `expected` is the input's `body_hash`
but `got` is a hardcoded zero — they are not the same quantity, so a consumer that diffs the
two fields learns nothing, and a log reader sees `got = 0x0000…` which looks like a missing
value rather than "recompute it yourself."

This matters because the reseal hammer is the **strongest** check in the path — when it fails,
divergence tooling most needs to localize the discrepancy. Cheap improvement: compute and
report the resealed bytes' body hash so the pair is actually comparable:

```rust
let got_hash = *blake3::hash(&resealed[HEADER_LEN..]).as_bytes();
ReplayError::Divergence { what: "resealed log bytes", at_icount: header.end_icount,
    expected: header.body_hash, got: got_hash }
```

(Or carry both byte buffers in a dedicated variant; the per-record checks already ran first,
so a reseal mismatch that the earlier checks missed is a genuinely interesting forensic case —
e.g. an AUX record like FRAME_MARK / NET_TX whose icount or ordering differs while the chain
hashes still match. That is precisely the divergence class the reseal hammer is there to catch,
and right now its diagnostic is the weakest of all the variants.)

This is **Important** rather than Critical because correctness is unaffected — the engine
*does* reject the divergence loudly. Only the diagnostic is degraded.
