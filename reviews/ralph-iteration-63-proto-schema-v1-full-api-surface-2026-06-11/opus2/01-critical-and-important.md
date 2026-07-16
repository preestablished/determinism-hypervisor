# Critical & Important Findings

**None.**

I specifically probed the high-risk areas a first-pass review tends to skip, and each
checked out:

## Verified clean (no finding)

### protoc C++-scoping — no latent collisions beyond the handled one
The `PAUSED → PAUSED_S` rename exists because proto enum values are package-scoped
(C++ rules), so two enums cannot share a value name. I extracted every enum value
across the whole package (parsing `proto/hypervisor.proto`, including the nested
`MemPredicate.Op`):

```
28 distinct value names, 0 collisions.
```

Every "UNSPECIFIED" zero-value is correctly prefixed
(`SLOT_UNSPECIFIED`, `HASH_EPOCHS_UNSPECIFIED`, `PF_UNSPECIFIED`,
`QUIESCE_MODE_UNSPECIFIED`, `STOP_UNSPECIFIED`, `OP_UNSPECIFIED`), which is exactly
the convention that prevents this collision class. No future-firing collision exists
today. (There is a forward-guarding *guidance* gap — see `02-suggestions.md` #1 — but
it is not a defect in this diff.)

### prost oneof / same-named-message mangling — compiles unambiguously
`ScheduledEvent` has oneof `Event` with variants `PadSet`/`DevEvent`/`NetRx` that
reference messages `PadSet`/`DeviceEvent`/`NetRx`. Generated (verified in the `.rs`):

```rust
pub mod scheduled_event {
    pub enum Event {
        PadSet(super::PadSet),     // variant name == message name; module scope disambiguates
        DevEvent(super::DeviceEvent),
        NetRx(super::NetRx),       // ditto
    }
}
```

The variant lives in `scheduled_event::Event`, the message at crate-root `v1::NetRx` /
`v1::PadSet` — no clash. Same pattern in `frame_capture_event::Msg::Done(super::RunResponse)`
(a oneof variant embedding the very `RunResponse` message that is *also* a standalone
message and a streamed terminal). Compiles clean.

### prost type mappings — all correct
- `NextSdkEvent.stream` (proto3 `optional uint32`) → `Option<u32>`. ✓
- `RunResponse.sdk_event` (`GuestEvent`) → `Option<GuestEvent>`, and `GuestEvent` is
  *also* the `StreamGuestEvents` server-stream item (`type ResponseStream` Item =
  `GuestEvent`). Dual-use is fine. ✓
- `MemPredicate.Op` nested enum → `mem_predicate::Op` with values `Unspecified/Eq/Ne/Ge/Le`. ✓
- `FrameCaptureEvent.Msg` oneof embeds `RunResponse` as the terminal `Done` arm. ✓

### API.md ↔ proto field consistency — 14/14 messages match
Field-number-and-name diff of `MachineConfig`, `RunRequest`, `RunResponse`,
`TakeSnapshotResponse`, `ScheduledEvent`, `Divergence`, `QuiesceRequest`,
`MemPredicate`, `StopReason`, `SlotState`, `FrameCaptureEvent`, `CapturedFrame`,
`NextSdkEvent`, `VerifyReplayRequest` — all identical. `ErrorDetail` is the sole
deviation, and it is intentional + documented (API.md §2.9 prose names the three
fields in order `slot_id, icount, code` but assigns no numbers; the proto pins them
1/2/3 in that order, with an in-comment statement saying so). ✓

### Cross-arch + Cargo.lock
aarch64 generated output is present and identical in surface (2626 lines). `Cargo.lock`
is unchanged vs `main` (no new deps). ✓
