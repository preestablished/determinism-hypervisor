# Positive Notes

### Capture-neutrality is enforced by construction, not just intent

`FrameSink` returns a `FrameSinkFlow` and is documented and implemented as
read-only w.r.t. guest state, DHILOG, and chain
(`crates/dh-vmm/src/runctl.rs:53-64, 411-459, 686-702`). Chain links stay on the
epoch grid plus the final stop regardless of whether a sink is attached; the
plain-Run path even shares the same `run_segment_with_frame_captures` entry with
no-op sinks, so there is one code path and no drift between "captured" and
"plain." The vmm test asserts this directly:
`frame_sink_observes_every_frame_and_is_capture_neutral_live`
(`runctl.rs:2118-2185`) uses a RUN-TWICE pattern and asserts
`(boundary, state_hash, frames_elapsed)` equality between a plain run and a
sink-observed run.

### The live-input side channel mirrors the one existing cross-actor pattern

`SlotLiveInputs` (`crates/dh-worker/src/runtime.rs:426-520`) is explicitly
modeled on the async-pause `Arc<AtomicBool>` — the only other thing that crosses
the actor boundary without an actor command — so `InjectInputs` on a slot with
an active streaming run does not queue behind the whole play session on the
actor's mpsc. The lock discipline is clean: `observe_frame` (actor thread) and
`live_inject_from_proto` (blocking pool) both serialize on the same mutex, and
`observe_frame`'s `target <= frame` drain plus the `deactivate` leftover
re-queue (`service.rs:3644-3663`) guarantee an accepted input is either
landed-and-logged or carried forward — never dropped. The rejection floor
(`last_streamed_frame`, strictly-greater) is what the operator has actually
seen, which is the right contract.

### The streaming design avoids the bounded-channel deadlock trap

Frames are produced on a dedicated OS thread, the `Receiver` is handed straight
to tonic and concurrently polled, and the producer uses `try_send` in a manual
loop rather than `.send().await` (`service.rs:5159-5194`). This is exactly the
shape `tokio-channel-streaming-deadlocks.md` prescribes: "spawn the producer as
a separate task ... design for backpressure from the start," and never pre-fill
then hand over. The full-channel state IS the backpressure hold, and the actor
thread (not a runtime worker) is what blocks — respecting the "never a shared
async runtime" constraint.

### Cancel/watchdog landing is validated against a deterministic reference

`linux_stream_cancel_lands_paused_at_a_frame_boundary`
(`crates/dh-worker/tests/frame_capture_stream.rs:235-365`) compares the
cancelled run's **state hash** against a fresh `FrameBudget` run to the same
frame — and correctly compares hashes, not content-addressed snapshot refs,
because the refs embed lineage that differs by construction (comment at
`349-354`). That is the right determinism claim and the right way to assert it.

### Metric accounting is balanced across all three exit paths

`frame_holds_in_progress` is incremented once via `get_or_insert_with`
(`service.rs:5170-5173`) and decremented on every terminating branch — success
(`5162-5164`), watchdog (`5175`), and cancel-with-prior-hold (`5185-5187`) — and
the no-hold cancel path never incremented, so the gauge stays balanced. The
termination-reason mapping keeps cancel/watchdog distinct from a plain
`Paused` via `stop_cause` (`5221-5247`), which is the only way to disambiguate
them since all three land `StopReason::Paused`.

### Backward-compatible proto evolution

`build_profile` is added as field 6 on `GetWorkerInfoResponse` with no renumber,
and the `RunWithFrameCapture` message/RPC pre-existed on `main` (only its
doc-comment expanded), so no wire-format break (`proto/hypervisor.proto`).
`build_profile` is plumbed consistently through a single `build_profile()`
helper, the `GetWorkerInfo` handler, the startup log, and the perf-smoke print
(`service.rs:1127-1133, 5286`; `bin/dh-workerd.rs:70`;
`tests/play_perf_smoke.rs:135-141`).

### The perf guard is a *relative* regression check

`play_perf_smoke.rs` asserts streamed fps does not fall below the per-frame-Run
baseline measured in the same process (`166-172`), explicitly avoiding flaky
absolute wall-clock gates on a shared runner. The comment names the exact
regression it guards ("someone reintroducing a per-frame chain link in the
streaming path"), which is precisely the cost M2 removes.
</content>
