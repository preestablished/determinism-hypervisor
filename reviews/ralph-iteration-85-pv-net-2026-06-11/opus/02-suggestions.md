# Suggestions

### S-1. `FrameTooBig` is returned for zero-length and over-cap frames — the name is wrong for the empty case

`apply_net_rx` (`net.rs:120`) collapses three distinct rejection reasons into
`NetRxError::FrameTooBig`:

```rust
if len == 0 || len > MAX_FRAME || len > self.rx_cap {
    return Err(NetRxError::FrameTooBig);
}
```

For `len == 0` the error name is actively misleading (the frame is too *small*). The
variant doc string also only mentions "exceeds MAX_FRAME or the guest-published RX
capacity," so the zero case is undocumented. The test even has to write a comment to
explain the surprising result (`net.rs:341`: *"cap is 0 — too big before buffer check"*),
which is a code smell: the test is apologizing for the API.

Consider either a dedicated `EmptyFrame` variant, or — if I-2 is resolved by accepting
zero-length frames — removing the `len == 0` check from this clause entirely. At minimum,
update the `FrameTooBig` doc comment to mention the empty case so the variant's contract
matches its uses.

### S-2. Error precedence: cap-vs-buffer ordering returns `FrameTooBig` before `NoRxBuffer` — defensible, but worth a one-line rationale

When no RX buffer is published, `rx_cap` defaults to 0, so any nonzero frame trips
`len > self.rx_cap` and returns `FrameTooBig` *before* the `rx_buf_gpa == 0` →
`NoRxBuffer` check is reached. The test documents this (`net.rs:339-342`). The ordering is
**sensible** as written: a guest that published neither a buffer nor a capacity is
equally broken either way, and run control "faults the slot" identically for all three
`NetRxError` variants, so the exact variant is diagnostic-only, never control-flow.

However, the more *intuitive* precedence for a human debugging a slot fault would be
"buffer missing" before "frame too big for the (zero) capacity," since a missing buffer is
the root cause. If `NetRxError` is ever surfaced to an operator (logs, metrics), consider
moving the `rx_buf_gpa == 0` check first so the reported reason points at the actual
guest-side omission. Not worth churning the tests for if the variant stays purely internal —
just flag the intent with a comment on the precedence.

### S-3. `STATUS_IDLE`/`STATUS_OK`/`STATUS_FAULT` numbering differs from `PvBlk`'s status codes — confirm this is intentional

`PvBlk` uses `STATUS_OK = 0` (`blk.rs:50`); `PvNet` uses `STATUS_IDLE = 0, STATUS_OK = 1,
STATUS_FAULT = 2` (`net.rs:39-41`). The pv-net choice (0 = never-rung) is arguably *better*
than pv-blk's (a freshly-reset device reads `STATUS_OK` before any doorbell, which is
slightly misleading). This divergence is fine and probably an improvement — just confirm
no shared run-control code or test assumes a single cross-device status convention, and
consider a one-line module-doc note that pv-net intentionally adds an IDLE state that
pv-blk lacks, so a future reader does not "fix" it to match pv-blk.

### S-4. `TX_STATUS` is not reset to `IDLE` on new `TX_BUF_GPA`/`TX_LEN` writes — fine, but make the contract explicit

`TX_STATUS` retains its last value (`OK`/`FAULT`) across subsequent register writes until
the next doorbell overwrites it. This is the same "STATUS is the completion latch, valid
at the doorbell exit, sticky until the next doorbell" contract pv-blk uses, and it is
correct and deterministic (no host input, snapshot captures it verbatim). The only risk is
a guest that reads `TX_STATUS` *after writing new TX regs but before ringing the doorbell*
and mistakes the stale value for this transaction's result. That is a guest-driver contract
question, not a determinism bug. A single sentence in the module doc — "TX_STATUS reflects
the most recent doorbell only; it is not cleared by register writes" — would close the
ambiguity for SDK authors (fbr).
