# Suggestions (non-blocking)

### S1. `device_exercise.asm` comment is now half-stale on the "tracked in beads" tail
`tests/nanokernel/asm/device_exercise.asm:171-175`: the diff correctly changed "prints 0x1E0000" → "PRINTED 0x1E0000" (the vendored table is now fixed in this very diff), but the trailing "(doc contradiction tracked in beads)" at line 175 is now partly resolved — the **vendored** contradiction is gone; only the **upstream** guest-sdk repo still has it. `lib.rs:149-151` was updated more precisely ("fixed in the vendored copy, upstream tracked"). Align the asm comment to the same phrasing for consistency, e.g. "(vendored table now fixed; upstream tracked)."

### S2. §3.1: "PIO" measured, but spell out which PIO
The window's PIO OUT markers (`S`/`E`) are measured to contribute 0, but only the single-byte `out dx, al` form — `counting_smoke.rs:88-92` explicitly warns a batched `rep outsb` marker would alias both markers onto one counter read and silently measure a zero window. The §3.1 "PIO retires zero" generalization is fine, but a one-clause caveat that the measurement assumed single-byte OUT (not REP-string PIO, which would retire by the REP rule) would harden the spec against the exact aliasing footgun the test guards.

### S3. §3.2 line 260 is consistent — leave it, but consider a forward-reference
`ARCHITECTURE.md:260` ("handle exit (MMIO etc. are serviced; they don't disturb counting)") and `:265` ("service any interleaved MMIO exits (count unchanged until retirement)") are both **consistent** with the new §3.1 zero-retirement rule — verified, no change needed. Minor: ":265" still says "until retirement," which under the new rule never happens for the exiting instruction itself (the count is unchanged before and after). Reword to "(count unchanged across the exit; the exiting instruction retires zero, §3.1)" to remove the implied "eventually retires."

### S4. Reconcile the two timer-base docstrings regardless of I2's outcome
Independent of how §6.2 is fixed: `crates/dh-devices/src/clock.rs:90-92` and `crates/dh-vmm/src/runctl.rs:106-113` give contradictory accounts of who subtracts `vns_base`. Pick one and make both match the code (which today does the subtraction nowhere because base is 0, and assigns the future responsibility to the caller per runctl). This is a latent M4 footgun unrelated to this iteration's diff but worth a cleanup bead.

### S5. §3.1 "never retiring" vs boundary-engine wording
The new §3.1 reframes "not yet retired" → "never retiring" for a mid-emulation `KVM_EXIT_MMIO`. This is accurate and matches `runctl.rs`/§3.2. Good. One nicety: "never retiring" reads as a permanent property of the instruction, but you mean the *exiting* instruction's retirement is attributed to neither the pre-exit nor post-resume count. Consider "contributes zero to the retirement count on both sides of the exit" to avoid a reader inferring the instruction's *effects* don't happen.
