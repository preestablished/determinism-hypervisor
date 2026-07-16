# Positive Notes

- The ignored gate behavior is appropriate for the current sibling fixture state. With `DH_M9_ALLOW_SKIP=0`, missing artifacts/KVM fail via the existing M9 helpers, and the current SDK stub should fail at the inject-query evidence check instead of producing a false pass.

- The `InjectQuery` parser matches the current detguest-wire canonical payload: `iseq` at bytes 0..4 and `name_id` at bytes 4..8.

- The `PIO_ANSWER` parser matches `dh_inputlog::dhilog::LogWriter::pio_answer`: 8-byte payload, port at bytes 0..2, packed answer at bytes 4..8, filtered to `PORT_INJECT`.

- `VerifyReplay` is correctly invoked from the READY snapshot with only the sealed input-log id, and the test checks both end state hash and total icount against the live segment.

- The docs label the gate as fixture-dependent/pending rather than claiming completed Linux GS-7 evidence.
