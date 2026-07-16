# Positive Notes

## The compile-time rpc-surface pin is the right shape

`crates/dh-proto/src/lib.rs:48` — `_all_seventeen_rpcs` takes a real
`HypervisorWorkerClient<Channel>` and calls every generated method with its request
type, then `all_seventeen_rpcs_are_generated` references the fn-item
(`let _ = _all_seventeen_rpcs;`) to defeat dead-code elimination. This makes a proto
rpc rename a **compile error in dh-proto**, not a runtime surprise in dh-worker. The
comment correctly notes *why* it can't be a plain fn-item pin (tonic's
`impl IntoRequest<T>` argument rules). This is a genuinely better test than asserting
method existence by name string.

## The `PAUSED_S` handling is documented in all three places it needs to be

The collision fix is explained in (1) the proto inline comment at the enum, (2) the
`full_surface_message_shapes` test comment, and (3) the API.md §2.8 local edit. The
API.md note (lines 443-445) is placed *immediately under* the enum and *inside* the
code fence, so an independent implementer transcribing §2.8 sees it in-band and won't
re-introduce a bare `PAUSED`. The note even calls out that the original API.md text was
"an oversight of the same rule its own FAULTED_S works around" — precise root-cause
framing.

## Field-for-field transcription fidelity

I diffed 14 messages/enums against the normative API.md §2 text and every field number,
name, and type matches. The inline semantic comments are ported verbatim from API.md
(frame-counter semantics, capture-neutrality invariants, the `at_frame` absolute-vs-
relative warning), and the proto header explicitly states the keep-in-sync obligation
("keep both in sync when either changes"). For a 421-line additive transcription this
is impressively low-drift.

## ErrorDetail field-number provenance is explicit

`proto/hypervisor.proto` §2.9 comment states plainly that API.md writes `dh.ErrorDetail`
with no field numbers and that the numbers are "pinned here, not in API.md". This is
exactly the kind of decision that otherwise becomes an unanswerable "why is this 1/2/3?"
six months later. The field order (slot_id/icount/code) also matches the prose order in
API.md §2.9, so the pin is non-arbitrary.

## build.rs fails loudly on package drift

`crates/dh-proto/build.rs` checks that `determinism.hypervisor.v1.rs` actually got
generated and returns a descriptive error if not, instead of letting a `package` vs
`include_proto!` mismatch surface as an opaque "No such file" at the include site.
This carried over cleanly and still guards the larger surface.

## Cross-arch and dependency hygiene

The aarch64 codegen output is byte-identical in surface to x86_64 (2626 lines), the
vendored protoc covers both arches, and the change adds **zero** new dependencies
(`Cargo.lock` unchanged). Incremental rebuild after a proto touch is ~0.6s — the arm
CI cold-build cost is in the tonic/prost dep tree, not this schema, so the surface
growth (skeleton → full) is not a CI-time concern.
