# Acceptance

Current evidence is not sufficient for acceptance.

The existing `target/m9/linux_boot_trace.json` artifact is favorable: `unclassified_denied_msr_indices`, `unclassified_mmio_addresses`, and `unclassified_irq_timer_exit_counts` are empty, and only lAPIC-required APIC MMIO/MSR plus Linux CPU compatibility MSRs appear.

However, `linux_entry_smoke` does not assert those conditions. It would pass after writing a trace with non-empty unclassified buckets as long as the guest did not immediately fail before a serviceable exit. The branch therefore reports the right data but does not make "no unclassified exits remain" an enforced acceptance property.

Minimum acceptance gate I would expect:

```text
DH_M9_TRACE_BOOT=1 cargo test -p determinism-tests --test linux_boot_trace -- --ignored --nocapture
```

with assertions that all unclassified trace fields are empty and the terminal reason is one of the expected bounded-run outcomes.
