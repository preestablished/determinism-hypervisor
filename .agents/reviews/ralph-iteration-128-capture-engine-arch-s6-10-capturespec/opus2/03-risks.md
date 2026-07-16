# Risks

- Determinism/replay risk: DetChannel PIO handling was added only on the recording/runtime side. Until replay uses the same exit semantics, any DetChannel guest can produce logs that the verifier cannot replay.

- Resource-exhaustion risk: framebuffer and feature capture sizes are effectively guest/client controlled. A malformed manifest or oversized request can allocate far more memory than the worker should tolerate.

- API consistency risk: `Run` returning a capture error after committing execution makes client recovery ambiguous. This can create accidental double-advance behavior in orchestrators.

- DetChannel ABI edge risk: `service_exit_with_detchannel` zero-extends/truncates non-4-byte detcall PIO data. If the ABI requires 32-bit accesses, invalid widths should probably be rejected or faulted explicitly rather than normalized silently.

- Residual fork risk is low: the child-bus refactor preserves the old `fork_slot` shape and adds a child-memory-aware variant. Existing rollback tests cover builder failure and parent thawing.
