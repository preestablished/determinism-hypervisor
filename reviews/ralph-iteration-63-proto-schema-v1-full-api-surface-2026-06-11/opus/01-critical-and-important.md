# Critical & Important Findings

## Critical

**None.**

The headline field-number fidelity audit found zero wire-contract breaks. I compared `proto/hypervisor.proto` against API.md §2 message-by-message, field-by-field, number-by-number. Result of the audit on the explicitly-flagged high-risk cases:

| Message | Spec (API.md §2) | Proto | Match |
|---|---|---|---|
| `RunRequest.until` oneof | `icount_budget=2, vns_budget=3, frame_budget=8, next_sdk_event=4, goal=5` | identical (incl. the non-contiguous 8) | ✅ |
| `RunRequest` scalars | `hard_icount_cap=6, capture=7` (lease=1) | identical | ✅ |
| `ScheduledEvent` `at` oneof | `at_icount=1, at_vns=2, at_frame=3` | identical | ✅ |
| `ScheduledEvent` `event` oneof | `pad_set=4, dev_event=5, net_rx=6` | identical | ✅ |
| `TakeSnapshotResponse` (12 fields) | 1..12, types as spec | identical, all 12 | ✅ |
| `Divergence` (8 fields) | 1..8, `diff_page_idx` repeated=7 | identical | ✅ |
| `MachineConfig` (10 fields) | 1..10 | identical | ✅ |
| `RunResponse` (9 fields) | reason=1..fb_info=9 | identical | ✅ |
| Streaming rpcs | `StreamGuestEvents`→`stream GuestEvent`, `VerifyReplay`→`stream VerifyReplayProgress`, `RunWithFrameCapture`→`stream FrameCaptureEvent`, `WatchSlots`→`stream SlotEvent` | identical | ✅ |
| All other messages/enums (§2.1–§2.10) | per spec | identical | ✅ |

The only value that differs from the *original* spec text is `SlotState.PAUSED_S` (tag 2) — see below; it is intentional, wire-preserving, and the local API.md §2.8 has been updated to match, so proto and the in-repo normative copy now agree.

## Important

**None.**

### PAUSED_S decision — assessed, judged correct

- **Correct call?** Yes. proto3 enum values use C++ scoping rules (the constants are siblings in the enclosing scope, not nested under the enum type), so `SlotState.PAUSED` would collide with `StopReason.PAUSED` and protoc fails the build. The spec already worked around the *identical* rule for `FAULTED` → `FAULTED_S`; applying the same `_S` suffix to `PAUSED` is the consistent, minimal fix.
- **Wire-compatible?** Yes. The rename keeps tag `= 2`. Enum wire format is the integer tag only; the symbol name is never serialized. A `PAUSED_S` value 2 is byte-identical on the wire to a `PAUSED` value 2. The round-trip test pins `v1::SlotState::PausedS as i32 == 2` (lib.rs:149), and `FaultedS as i32 == 5` (lib.rs:150).
- **Comment quality?** Good in both files. `proto/hypervisor.proto:406-409` explains the C++-scoping cause, names the colliding `StopReason.PAUSED/FAULTED`, and notes API.md's original oversight. `API.md:443-445` (the local edit) carries an equivalent note. The two comments are paraphrases of each other (not verbatim-identical), which is fine — both are accurate.
- **Upstream sync** is tracked by bead veu, as stated in the prompt. The proto comment at line 408 ("API.md §2.8 wrote PAUSED here — an oversight…") is now slightly stale relative to the *local* API.md, which has already been edited to PAUSED_S; this is a cosmetic nit, captured in 02-suggestions.md, not a correctness issue.

### dh-proto test additions — assessed, sound

- **17-rpc call-shape pin** (`_all_seventeen_rpcs`, lib.rs:48–90): correct pattern. The uncalled `async fn` takes a `&mut HypervisorWorkerClient` and issues every method with its `*Request::default()`, so a proto-level rename of any rpc method or its request type breaks *compilation here* rather than silently downstream. The `all_seventeen_rpcs_are_generated` test references `_all_seventeen_rpcs` as a fn item (lib.rs:89) to defeat dead-code elimination. The doc-comment correctly explains why a plain fn-item pin won't work (tonic's `impl IntoRequest<T>` arg means the method only resolves through a real call expression). I counted the calls: exactly 17, matching the service. No rpc is missing.
- **Message round-trips** (`full_surface_message_shapes`, lib.rs:94–185): exercises both `ScheduledEvent` oneofs, `RunRequest`'s `frame_budget` arm + `capture`, `GoalCondition` with nested `MemPredicate::Op`, `VerifyReplayProgress::Divergence`, `NextSdkEvent` optional, and `ErrorDetail` — a good load-bearing subset. All use prost encode→decode→`assert_eq`, the right equivalence for "the generated type holds the shape I think it does."
- **proto3 `optional uint32 stream`** (NextSdkEvent): generates `Option<u32>`. Used correctly — `stream: None` vs `stream: Some(9)` (lib.rs:171-172), asserting the two encode to *different* bytes (lib.rs:173). This is exactly the right assertion: it proves field presence is tracked (a non-optional `uint32` would encode `0` and `None`-equivalent identically for the default). Good.
