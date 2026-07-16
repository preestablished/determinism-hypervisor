# Suggestions

### S1. `ref_b2 == ref_b1` is near-tautological given both restores succeeded — keep it, but the comment over-sells it

**File:** `crates/dh-worker/tests/store_durability.rs:221-245`

The final assertion is billed as "the strongest form." Trace what actually makes
it hold:

- Both `outcome1` (instance 1) and `outcome2` (instance 2) restore from the
  **same** `delta.snapshot_ref`. The restore is content-addressed: `get_snapshot`
  returns the byte-identical manifest, `resolve_pages` flattens the identical
  page set. So once `outcome2`'s restore is asserted byte-identical to the live
  source (lines 200-218) and `outcome2.chain.value() == outcome1.chain.value()`
  (line 219), `outcome2` and `outcome1` carry the *same* boundary inputs into the
  re-snapshot.
- `take_snapshot` with `PageSource::Full` is a deterministic function of (RAM
  bytes, vCPU blob, boundary, config). Equal inputs ⇒ equal container ⇒ equal
  ref. So `ref_b2 == ref_b1` follows from the *already-asserted* byte-equality
  plus the determinism that `restore_engine`/`m4_transparency` already prove.

It is therefore not adding an independent durability signal beyond the byte-level
restore assertions above it — it is re-confirming take-snapshot determinism over
state the test has already pinned. That is *fine* to keep (a cheap end-to-end
backstop, and the diff at line 244 even names the right failure: "restart changed
the bytes a restore reproduces"). But the lead comment "The strongest form …
content-addressed identity carried entirely by the persisted bytes" implies it is
the load-bearing durability check, when the load-bearing check is the
`delta.snapshot_ref` restore at line 186. Consider trimming the comment to
"end-to-end backstop: re-snapshot through the restarted instance reproduces the
same ref" so the hierarchy of what-proves-what is clear.

### S2. The same-instance reference leg (`ref_b1`, lines 141-172) is only used by S1's assertion — and that assertion is near-tautological

**File:** `crates/dh-worker/tests/store_durability.rs:141-172`

`outcome1` / `ref_b1` are computed entirely against the *live* instance 1, then
the only later uses are `outcome1.chain.value()` (line 219) and `ref_b1` (line
243). Per S1, `ref_b2 == ref_b1` mostly restates the byte-level assertions. The
chain comparison `outcome2 == outcome1` is the one genuinely useful reuse — it
ties the restarted restore's chain to a value computed before the restart, which
is a clean cross-check. If S1's assertion is trimmed or dropped, consider whether
the whole `slot_b1` reference leg (lines 141-172, ~32 lines) still earns its
keep, or whether asserting `outcome2.chain.value()` against a precomputed
constant (the chain is deterministic from fixed inputs) would be equally strong
and shorter. Not a blocker — the reference leg is readable and the duplication
with `restore_engine` is intentional acceptance redundancy — just flagging that a
third of the test body serves one near-tautological assertion.

### S3. vCPU comparison `capture(&slot_b2) == capture(&slot_a)` requires `slot_a` to stay live — add a one-line note

**File:** `crates/dh-worker/tests/store_durability.rs:214-218`

The assertion compares the restored slot to the **still-live source slot**
`slot_a` (created line 71, never dropped). This is the exact pattern from
`restore_engine.rs:161-162` and `:387-388`, so it is consistent and correct.
Its incremental value here over `restore_engine` is narrow: it confirms the vCPU
blob survives the *restart*, not just a live restore. That is legitimate but
modest. The subtle maintainability risk: the assertion silently depends on
`slot_a` outliving instance 1's teardown (lines 174-181). It does, but a future
edit that drops `slot_a` early (e.g. to free the KVM fd before instance 2) would
turn a meaningful comparison into a compile error or, worse, a comparison against
a re-created slot. A one-line comment at line 71 ("kept live to the end: the
post-restart byte/vCPU comparisons read it") would protect the invariant.

### S4. UDS readiness probe has no failure diagnostics; `.expect("store ready")` after 50×10ms is opaque

**File:** `crates/dh-worker/tests/common/mod.rs:68-78`

The probe loops 50×10ms (~500ms budget) then `client.expect("store ready")`. If
instance 2 ever fails to bind its UDS (e.g. a future change reuses a socket name,
or `serve_for_tests`'s stale-socket removal interacts badly with a slow
teardown), the failure surfaces as a bare `expect` panic with no captured
connect error — the last `Err(_)` is discarded at line 75. Consider keeping the
last error and surfacing it in the panic message
(`client.unwrap_or_else(|| panic!("store ready: last connect err: {last_err:?}"))`),
which costs nothing and turns a future flake into a self-diagnosing failure.
Minor; the current 500ms budget is generous for in-process bind.

### S5. `MEM` comment is stale (copy-paste from a 4 MiB fixture)

**File:** `crates/dh-worker/tests/store_durability.rs:34`

`const MEM: u64 = 2 * 1024 * 1024; // 512 pages` — 2 MiB / 4096 = 512 pages, so
the page count is right, but the same `// 512 pages` comment rides on a
`2 * 1024 * 1024` literal here while the sibling `restore_engine.rs:30` uses the
identical line. Fine as-is; just confirming it is correct, not the "1 MiB / 256
pages" the comment phrasing might suggest at a glance. No change needed beyond
awareness — flagging because the dirtied GPAs (`0x2000`, `0x5000`, `0x9000`) and
the root page (`0x8000`) must all fall inside `MEM`; `0x9000` = 36 KiB < 2 MiB,
so they do. Good.
