# Suggestions (non-blocking)

### S1 — Record the microcode revision alongside "kernel 6.8" in the R2 class empirics

The trap-eating behavior is a per-determinism-class empiric (R2). The comments in
`boundary.rs` and `counting_semantics.rs` cite "measured, kernel 6.8" but the class is
really (kernel, CPU model, microcode). This box is kernel `6.8.0-124-generic`, microcode
`0xfa`. Recording the microcode (e.g. in the `COUNTING_DELTA_AT_OUT_EXITS` doc-comment in
`tests/nanokernel/src/lib.rs`, next to "kvm-intel class") makes the class definition precise
and gives the future R2 alarm a concrete baseline to diff against. Suggestion level — the
prompt itself flagged this as "THE per-class empirics concern."

### S2 — Make the MMIO-write sentinel a structured error variant

`tests/determinism/tests/counting_semantics.rs` signals "end the step at the MMIO write" by
returning `BoundaryError::Exit(MMIO_WRITE_SENTINEL.to_string())` and matching on the string
(`msg == MMIO_WRITE_SENTINEL`). It is correct and unambiguous *here* (the only `Exit` error
the harness's own `on_exit` produces is this sentinel, and the only other `Exit` source —
"unexpected exit" — would carry a different string and fall to the `Err(e) => return`
arm). But a structured variant (e.g. `BoundaryError::MmioWriteStep`) would be self-documenting
and immune to a future copy-paste of the string. Test-only, low priority; the `String` payload
on `BoundaryError::Exit` is the general-purpose escape hatch and is fine to reuse.

### S3 — Reconcile the "~700 instructions" free-run figure with the measured value

The doc-comment on `landing_across_an_mmio_write_does_not_free_run`
(`counting_semantics.rs`) says the vCPU "free-runs ~700 instructions to the park HLT."
The independent probe measured the un-re-armed free-run as **991** instructions from the
MMIO write (count 12) to the next exit, and the fix-reverted regression run reported
`counted: 1003`. The "~700" is a soft approximation and the exact figure depends on the
landing window's start, so this is cosmetic — but tightening it (or dropping the number)
would avoid a future reader being misled when they reproduce it.
