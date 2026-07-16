# Positive notes

### P-1. The "every host mutation flows through one logged sink" architecture is exactly right

`CtxSink` (detchannel.rs:347–386) is the single chokepoint: `ring_push`,
`cons_bump`, and `pio_answer` are the *only* ways channel memory mutations reach
the log, and they all stamp `DevCtx`'s icount/rip. Combined with the fact that
`channel()` is read-only and `push_command`/`push_workload_ctrl`/drains are the
only mutators, this makes "no host behaviour can differ between record and
replay" structurally enforceable rather than a property you have to audit at
every call site. The module's design matches the determinism goal cleanly.

### P-2. The digest-over-payload-only choice is the correct one, even though it looks fragile

Taking the digest over `buf[RECORD_HEADER_LEN..n]` (detchannel.rs:501) rather
than the whole record is what makes the dropped `FLAG_TRUNCATED` / `seq` /
`vnanos` reconstruction *not matter* (see I-1). Excluding the header from the
digest sidesteps an entire class of re-encode-divergence bugs. Whether
deliberate or lucky, it is the right slice.

### P-3. The INJECT double-logging trap is correctly avoided, and tested

`pio_in`'s INJECT arm returns early (detchannel.rs:239–244) precisely because
`InjectResponder::answer` already logs the `PIO_ANSWER` through the sink
(inject.rs:62). The `inject_flow_answers_via_plan_and_logs_once` test asserts the
exact record count (`before + 4`: CONS_BUMP + 2× SDK_EVENT + 1× PIO_ANSWER),
which would catch a regression that double-logged. Good instinct to count records
rather than just check the answer value.

### P-4. Status-code mapping correctly delegates class decisions to the library

`channel_init` (detchannel.rs:263–291) does the two checks it owns (committed
size, 2 MiB alignment) and then hands everything else to `Channel::attach` and
`AttachError::init_status` (channel.rs:59–66). It does not re-implement the
magic/version/ring-descriptor → status mapping. The `init_status_codes` test
drives all four status outcomes (sentinel, BadMagicVersion, BadGpa ×2, Ok,
AlreadyAttached). Thorough.

### P-5. `INIT_STATUS_NEVER_COMMITTED = u32::MAX` sentinel is a genuinely good call

Choosing a value deliberately outside the ABI's `0..=3` (detchannel.rs:46–49,
tested at init_status_codes) means a guest that reads `IN 0xD37C` before
committing cannot mistake an uninitialized read for `Ok`. This is the kind of
defensive sentinel that prevents a subtle guest-side attach bug, and the
reasoning is documented inline.

### P-6. Clean deny-list / clippy conformance; the new file names no host APIs

The new file uses only `Vec`/`alloc` and the two library crates. It passes the
crate's `no_host_ambient_authority` source-grep gate (lib.rs:62) and
`#![deny(clippy::disallowed_types, clippy::disallowed_methods)]` with a clean
`cargo clippy -p dh-devices --all-targets`. No host time, randomness, network,
or filesystem on the execution path — the module respects the §6 contract it
operates under.
