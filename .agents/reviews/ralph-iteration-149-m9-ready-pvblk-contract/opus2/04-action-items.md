Critical:
- Load the full protected-mode bzImage image, not only `payload_offset/payload_length`, and add a test that fails with the current layout.

Important:
- Reject or zero unsupported setup-header fields before writing `boot_params`.

Suggestions:
- Replace the BzImage proto conversion panic with an error-returning path or documented invariant.
- Extend the ignored Linux smoke to execute guest code, not just inspect entry state.
