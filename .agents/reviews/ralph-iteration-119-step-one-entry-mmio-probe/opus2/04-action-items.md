# Action Items

Re-review result: the previous C1 assertion-strength concern is resolved.

What changed in the updated `boundary.rs` test logic:
- `StepProfile` now carries full before/after `Boundary` values plus the observed MMIO exits.
- `discover_mmio_profiles` now requires the exact MMIO cluster sequence: write `MMIO_BASE + 0x14`, write `MMIO_BASE + 0x08`, read `MMIO_BASE + 0x18`.
- Fresh runs now assert full boundary identity where it matters, including RIP/RCX.
- The previous loose `after_first.icount > discovered_after` assertion is replaced by an exact baseline-plus-ISR-delta assertion.
- `measure_isr_delta` independently measures interrupt retirement cost on a pure step and validates the ISR table before the MMIO-crossing assertion uses that delta.
- The second chained injection is also checked against the post-MMIO pure-step profile and exact expected icount.

Validation run:

```text
cargo test -p dh-vmm step_one_entry_chained_injection_crosses_mmio_exactly_live -- --nocapture
```

Result: passed.

Remaining action items: none from this re-review. No production code was edited.
