//! RunWithFrameCapture acceptance (play-60fps plan, M2): the streaming
//! capture RPC is capture-neutral (identical terminal state hash to a
//! plain Run over the same budget — the chain is cumulative, so terminal
//! equality across an epoch-crossing budget proves the whole link
//! sequence matches), frame-complete (contiguous absolute FRAME_COUNTER
//! indices, count == frames_elapsed), determinism-neutral under consumer
//! backpressure, and lands the slot Paused at a frame boundary on stream
//! cancel and on the stalled-consumer watchdog.
//!
//! All tests are M9-gated: they need KVM with dirty-ring support and the
//! staged DH_M9_* artifacts (docs/ops/test-partitioning.md).

mod common;

use common::TestResult;
use dh_proto::v1 as proto;
use dh_proto::v1::hypervisor_worker_server::HypervisorWorker;
use tokio_stream::StreamExt;
use tonic::Request;

/// Budget for the capture-neutrality leg: crosses the 50M-instruction
/// epoch grid at least twice so streaming-path epoch links are exercised,
/// not just the final stop link.
const NEUTRALITY_BUDGET: u64 = 120_000_000;
/// Budget for the slow-consumer and cancel legs: long enough for tens of
/// frames, short enough that a paced consumer finishes quickly.
const SHORT_BUDGET: u64 = 20_000_000;

/// layout_version 1: XRGB8888 256x224, stride 1024 (D7 contract).
const FB_V1_BYTES: usize = 1024 * 224;

struct StreamRun {
    frames: Vec<proto::CapturedFrame>,
    done: proto::RunResponse,
}

async fn run_streaming(
    svc: &dh_worker::service::WorkerService,
    lease: &proto::Lease,
    icount_budget: u64,
    read_delay: Option<std::time::Duration>,
) -> Result<StreamRun, String> {
    let response = svc
        .run_with_frame_capture(Request::new(proto::RunWithFrameCaptureRequest {
            lease: Some(lease.clone()),
            until: Some(proto::run_with_frame_capture_request::Until::IcountBudget(
                icount_budget,
            )),
            hard_icount_cap: 0,
        }))
        .await
        .map_err(|e| format!("RunWithFrameCapture: {e}"))?;
    let mut stream = response.into_inner();
    let mut frames = Vec::new();
    let mut done = None;
    while let Some(event) = stream.next().await {
        let event = event.map_err(|e| format!("stream event: {e}"))?;
        match event.msg {
            Some(proto::frame_capture_event::Msg::Frame(frame)) => {
                frames.push(frame);
                if let Some(delay) = read_delay {
                    tokio::time::sleep(delay).await;
                }
            }
            Some(proto::frame_capture_event::Msg::Done(response)) => {
                done = Some(response);
            }
            None => return Err("FrameCaptureEvent without msg".into()),
        }
    }
    let done = done.ok_or_else(|| "stream ended without terminal RunResponse".to_string())?;
    Ok(StreamRun { frames, done })
}

fn assert_frame_completeness(
    run: &StreamRun,
    start_frame_counter: u32,
    context: &str,
) -> Result<(), String> {
    if run.frames.len() as u64 != run.done.frames_elapsed {
        return Err(format!(
            "{context}: streamed {} frames but frames_elapsed={}",
            run.frames.len(),
            run.done.frames_elapsed
        ));
    }
    let mut expected = start_frame_counter;
    for frame in &run.frames {
        expected = expected
            .checked_add(1)
            .ok_or_else(|| format!("{context}: frame counter overflow"))?;
        if frame.frame_index != expected {
            return Err(format!(
                "{context}: frame index gap: expected {expected}, got {}",
                frame.frame_index
            ));
        }
        let fb_info = frame
            .fb_info
            .as_ref()
            .ok_or_else(|| format!("{context}: CapturedFrame without fb_info"))?;
        if fb_info.frame_counter != frame.frame_index {
            return Err(format!(
                "{context}: fb_info.frame_counter {} != frame_index {}",
                fb_info.frame_counter, frame.frame_index
            ));
        }
        let pixels = lz4_flex::decompress_size_prepended(&frame.fb_lz4)
            .map_err(|e| format!("{context}: fb_lz4 decompress: {e}"))?;
        if pixels.len() != FB_V1_BYTES {
            return Err(format!(
                "{context}: framebuffer payload {} bytes, expected {FB_V1_BYTES}",
                pixels.len()
            ));
        }
    }
    Ok(())
}

async fn restore_lease(
    svc: &dh_worker::service::WorkerService,
    snapshot: &proto::SnapshotRef,
) -> Result<proto::Lease, String> {
    let restored = svc
        .restore_snapshot(Request::new(proto::RestoreSnapshotRequest {
            snapshot: Some(snapshot.clone()),
            entropy_seed: Vec::new(),
        }))
        .await
        .map_err(|e| format!("RestoreSnapshot: {e}"))?
        .into_inner();
    restored
        .lease
        .ok_or_else(|| "RestoreSnapshot returned no lease".to_string())
}

async fn destroy(svc: &dh_worker::service::WorkerService, lease: &proto::Lease) {
    let _ = svc
        .destroy_vm(Request::new(proto::DestroyVmRequest {
            lease: Some(lease.clone()),
        }))
        .await;
}

#[test]
#[ignore = "M9 Linux acceptance: requires KVM dirty-ring support and staged DH_M9_* artifacts"]
fn linux_streaming_capture_is_neutral_complete_and_backpressure_safe() -> TestResult<()> {
    let Some(ready) = common::m9_linux_ready_snapshot(
        "frame_capture_stream::linux_streaming_capture_is_neutral_complete_and_backpressure_safe",
        2,
    )?
    else {
        return Ok(());
    };
    let start_frame = ready.ready_snapshot.frame_counter;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("test runtime: {e}"))?;
    rt.block_on(async {
        // Leg 1 — plain Run over the epoch-crossing budget on the READY
        // lease: the capture-neutrality reference.
        let plain = ready
            .svc
            .run(Request::new(proto::RunRequest {
                lease: Some(ready.lease.clone()),
                until: Some(proto::run_request::Until::IcountBudget(NEUTRALITY_BUDGET)),
                hard_icount_cap: 0,
                capture: None,
            }))
            .await
            .map_err(|e| format!("plain Run: {e}"))?
            .into_inner();
        destroy(&ready.svc, &ready.lease).await;

        // Leg 2 — the same budget from the same READY snapshot through
        // the streaming RPC with a fast consumer.
        let fast_lease = restore_lease(&ready.svc, &ready.ready_snapshot_ref).await?;
        let fast = run_streaming(&ready.svc, &fast_lease, NEUTRALITY_BUDGET, None).await?;
        destroy(&ready.svc, &fast_lease).await;

        if plain.state_hash != fast.done.state_hash
            || plain.icount != fast.done.icount
            || plain.frames_elapsed != fast.done.frames_elapsed
        {
            return Err(format!(
                "capture-neutrality violated: plain (icount={}, frames={}) vs streamed (icount={}, frames={})",
                plain.icount, plain.frames_elapsed, fast.done.icount, fast.done.frames_elapsed
            ));
        }
        if fast.done.frames_elapsed == 0 {
            return Err("streaming run produced no frames; budget too small for workload".into());
        }
        assert_frame_completeness(&fast, start_frame, "fast consumer")?;

        // Leg 3 — backpressure: a deliberately slow consumer over a
        // shorter budget must land bit-identically to a fast consumer of
        // the same budget (consumer timing must not perturb execution).
        let fast_short_lease = restore_lease(&ready.svc, &ready.ready_snapshot_ref).await?;
        let fast_short =
            run_streaming(&ready.svc, &fast_short_lease, SHORT_BUDGET, None).await?;
        destroy(&ready.svc, &fast_short_lease).await;

        let slow_lease = restore_lease(&ready.svc, &ready.ready_snapshot_ref).await?;
        let slow = run_streaming(
            &ready.svc,
            &slow_lease,
            SHORT_BUDGET,
            Some(std::time::Duration::from_millis(25)),
        )
        .await?;
        destroy(&ready.svc, &slow_lease).await;

        if fast_short.done.state_hash != slow.done.state_hash
            || fast_short.done.icount != slow.done.icount
            || fast_short.frames.len() != slow.frames.len()
        {
            return Err(format!(
                "slow consumer diverged: fast (icount={}, frames={}) vs slow (icount={}, frames={})",
                fast_short.done.icount,
                fast_short.frames.len(),
                slow.done.icount,
                slow.frames.len()
            ));
        }
        assert_frame_completeness(&slow, start_frame, "slow consumer")?;
        Ok::<(), String>(())
    })?;
    Ok(())
}

#[test]
#[ignore = "M9 Linux acceptance: requires KVM dirty-ring support and staged DH_M9_* artifacts"]
fn linux_stream_cancel_lands_paused_at_a_frame_boundary() -> TestResult<()> {
    let Some(ready) = common::m9_linux_ready_snapshot(
        "frame_capture_stream::linux_stream_cancel_lands_paused_at_a_frame_boundary",
        2,
    )?
    else {
        return Ok(());
    };
    let start_frame = ready.ready_snapshot.frame_counter;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("test runtime: {e}"))?;
    rt.block_on(async {
        // Start a long streaming run, read a handful of frames, then
        // drop the stream — the interactive-Stop path.
        let response = ready
            .svc
            .run_with_frame_capture(Request::new(proto::RunWithFrameCaptureRequest {
                lease: Some(ready.lease.clone()),
                until: Some(proto::run_with_frame_capture_request::Until::IcountBudget(
                    10_000_000_000,
                )),
                hard_icount_cap: 0,
            }))
            .await
            .map_err(|e| format!("RunWithFrameCapture: {e}"))?;
        let mut stream = response.into_inner();
        let mut seen = 0u32;
        while seen < 10 {
            match stream.next().await {
                Some(Ok(proto::FrameCaptureEvent {
                    msg: Some(proto::frame_capture_event::Msg::Frame(_)),
                })) => seen += 1,
                other => return Err(format!("expected a frame, got {other:?}")),
            }
        }
        drop(stream);

        // The cancel lands at the next frame boundary; wait for the slot
        // to report Paused (GetFramebuffer requires the Paused state, so
        // its first success IS the landing signal).
        let mut cancelled_fb = None;
        for _ in 0..500 {
            match ready
                .svc
                .get_framebuffer(Request::new(proto::GetFramebufferRequest {
                    lease: Some(ready.lease.clone()),
                }))
                .await
            {
                Ok(fb) => {
                    cancelled_fb = Some(fb.into_inner());
                    break;
                }
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
            }
        }
        let cancelled_fb =
            cancelled_fb.ok_or_else(|| "slot never landed Paused after cancel".to_string())?;
        let landed_frames = cancelled_fb
            .frame_counter
            .checked_sub(start_frame)
            .ok_or_else(|| "frame counter went backwards".to_string())?;
        if landed_frames < seen {
            return Err(format!(
                "cancel landed at frame delta {landed_frames}, but {seen} frames were streamed"
            ));
        }
        let cancelled_snapshot = ready
            .svc
            .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                lease: Some(ready.lease.clone()),
                seal_input_log: Some(false),
                capture: None,
            }))
            .await
            .map_err(|e| format!("TakeSnapshot cancelled slot: {e}"))?
            .into_inner();
        destroy(&ready.svc, &ready.lease).await;

        // Reference: a fresh restore run to the SAME frame boundary via
        // FrameBudget — one segment, one final link, exactly like the
        // cancel landing. Content-addressed snapshot refs must agree.
        let reference_lease = restore_lease(&ready.svc, &ready.ready_snapshot_ref).await?;
        let reference = ready
            .svc
            .run(Request::new(proto::RunRequest {
                lease: Some(reference_lease.clone()),
                until: Some(proto::run_request::Until::FrameBudget(landed_frames)),
                hard_icount_cap: 10_000_000_000,
                capture: None,
            }))
            .await
            .map_err(|e| format!("reference FrameBudget Run: {e}"))?
            .into_inner();
        if reference.icount != cancelled_snapshot.icount {
            return Err(format!(
                "cancel landed at icount {} but the frame-budget reference stopped at {}",
                cancelled_snapshot.icount, reference.icount
            ));
        }
        let reference_snapshot = ready
            .svc
            .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                lease: Some(reference_lease.clone()),
                seal_input_log: Some(false),
                capture: None,
            }))
            .await
            .map_err(|e| format!("TakeSnapshot reference slot: {e}"))?
            .into_inner();
        destroy(&ready.svc, &reference_lease).await;

        // Compare state hashes, not snapshot refs: the refs embed
        // lineage (the cancelled session descends from the initial
        // snapshot, the reference from the READY restore), which differs
        // by construction. The chain value at the landing boundary is
        // the determinism claim.
        if cancelled_snapshot.state_hash != reference_snapshot.state_hash {
            return Err(
                "cancelled-run state hash differs from the frame-budget reference: \
                 cancel landing is not the deterministic frame boundary"
                    .to_string(),
            );
        }
        Ok::<(), String>(())
    })?;
    Ok(())
}

#[test]
#[ignore = "M9 Linux acceptance: requires KVM dirty-ring support and staged DH_M9_* artifacts"]
fn linux_live_injected_inputs_at_frame_holds_replay_identically() -> TestResult<()> {
    let Some(ready) = common::m9_linux_ready_snapshot(
        "frame_capture_stream::linux_live_injected_inputs_at_frame_holds_replay_identically",
        2,
    )?
    else {
        return Ok(());
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("test runtime: {e}"))?;
    rt.block_on(async {
        // A slow consumer holds the vCPU at each FRAME_MARK, giving the
        // injection a stable frame-hold window (M3's operating mode).
        let response = ready
            .svc
            .run_with_frame_capture(Request::new(proto::RunWithFrameCaptureRequest {
                lease: Some(ready.lease.clone()),
                until: Some(proto::run_with_frame_capture_request::Until::IcountBudget(
                    12 * 28_000_000,
                )),
                hard_icount_cap: 0,
            }))
            .await
            .map_err(|e| format!("RunWithFrameCapture: {e}"))?;
        let mut stream = response.into_inner();
        let mut frames_seen = 0u32;
        let mut injected_at_frame = None;
        let mut done = None;
        while let Some(event) = stream.next().await {
            match event.map_err(|e| format!("stream event: {e}"))?.msg {
                Some(proto::frame_capture_event::Msg::Frame(frame)) => {
                    frames_seen += 1;
                    if frames_seen == 3 && injected_at_frame.is_none() {
                        let target = frame.frame_index + 4;
                        // Mid-run injection MUST return promptly (the
                        // out-of-band queue, not the busy actor channel)
                        // and be accepted for a future frame.
                        let scheduled = ready
                            .svc
                            .inject_inputs(Request::new(proto::InjectInputsRequest {
                                lease: Some(ready.lease.clone()),
                                events: vec![proto::ScheduledEvent {
                                    at: Some(proto::scheduled_event::At::AtFrame(target)),
                                    event: Some(proto::scheduled_event::Event::PadSet(
                                        proto::PadSet {
                                            port: 0,
                                            buttons: 0x0000_0010,
                                        },
                                    )),
                                }],
                            }))
                            .await
                            .map_err(|e| format!("live InjectInputs: {e}"))?
                            .into_inner()
                            .scheduled;
                        if scheduled != 1 {
                            return Err(format!("expected 1 scheduled, got {scheduled}"));
                        }
                        // Rejections: a held/past frame target and an
                        // icount-scheduled event while streaming.
                        let past = ready
                            .svc
                            .inject_inputs(Request::new(proto::InjectInputsRequest {
                                lease: Some(ready.lease.clone()),
                                events: vec![proto::ScheduledEvent {
                                    at: Some(proto::scheduled_event::At::AtFrame(
                                        frame.frame_index,
                                    )),
                                    event: Some(proto::scheduled_event::Event::PadSet(
                                        proto::PadSet { port: 0, buttons: 1 },
                                    )),
                                }],
                            }))
                            .await;
                        match past {
                            Err(status) if status.code() == tonic::Code::InvalidArgument => {}
                            other => {
                                return Err(format!(
                                    "past-frame live inject must fail INVALID_ARGUMENT, got {other:?}"
                                ))
                            }
                        }
                        let at_icount = ready
                            .svc
                            .inject_inputs(Request::new(proto::InjectInputsRequest {
                                lease: Some(ready.lease.clone()),
                                events: vec![proto::ScheduledEvent {
                                    at: Some(proto::scheduled_event::At::AtIcount(u64::MAX)),
                                    event: Some(proto::scheduled_event::Event::PadSet(
                                        proto::PadSet { port: 0, buttons: 1 },
                                    )),
                                }],
                            }))
                            .await;
                        match at_icount {
                            Err(status) if status.code() == tonic::Code::FailedPrecondition => {}
                            other => {
                                return Err(format!(
                                    "icount live inject must fail FAILED_PRECONDITION, got {other:?}"
                                ))
                            }
                        }
                        injected_at_frame = Some(target);
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
                Some(proto::frame_capture_event::Msg::Done(response)) => {
                    done = Some(response);
                }
                None => return Err("FrameCaptureEvent without msg".into()),
            }
        }
        let done =
            done.ok_or_else(|| "stream ended without terminal RunResponse".to_string())?;
        let injected_at_frame =
            injected_at_frame.ok_or_else(|| "run too short to inject".to_string())?;
        if u64::from(frames_seen) != done.frames_elapsed {
            return Err("frame count mismatch".into());
        }
        if u64::from(injected_at_frame - ready.ready_snapshot.frame_counter)
            > done.frames_elapsed
        {
            return Err(format!(
                "budget ended before the injected frame {injected_at_frame} was reached; \
                 raise the test budget"
            ));
        }

        // Seal the segment and replay it from the READY base: the
        // DHILOG must reproduce the live-injected run bit-for-bit.
        let sealed = ready
            .svc
            .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                lease: Some(ready.lease.clone()),
                seal_input_log: Some(true),
                capture: None,
            }))
            .await
            .map_err(|e| format!("TakeSnapshot seal: {e}"))?
            .into_inner();
        if sealed.state_hash != done.state_hash {
            return Err("sealed snapshot state hash != streamed terminal state hash".into());
        }
        let verify = common::verify_replay_done(
            &ready.svc,
            ready.ready_snapshot_ref.clone(),
            sealed.input_log_id.clone(),
        )
        .await?;
        if verify.end_state_hash != done.state_hash {
            return Err(
                "DHILOG replay of the live-injected run diverged from the recorded terminal \
                 state hash"
                    .to_string(),
            );
        }
        destroy(&ready.svc, &ready.lease).await;
        Ok::<(), String>(())
    })?;
    Ok(())
}

#[test]
#[ignore = "M9 Linux acceptance (slow: ~40s watchdog wait): requires KVM dirty-ring support and staged DH_M9_* artifacts"]
fn linux_stalled_consumer_watchdog_ends_the_run_paused() -> TestResult<()> {
    let Some(ready) = common::m9_linux_ready_snapshot(
        "frame_capture_stream::linux_stalled_consumer_watchdog_ends_the_run_paused",
        1,
    )?
    else {
        return Ok(());
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("test runtime: {e}"))?;
    rt.block_on(async {
        let response = ready
            .svc
            .run_with_frame_capture(Request::new(proto::RunWithFrameCaptureRequest {
                lease: Some(ready.lease.clone()),
                until: Some(proto::run_with_frame_capture_request::Until::IcountBudget(
                    10_000_000_000,
                )),
                hard_icount_cap: 0,
            }))
            .await
            .map_err(|e| format!("RunWithFrameCapture: {e}"))?;
        let mut stream = response.into_inner();
        // Read a few frames to prove the stream is live, then keep the
        // connection open WITHOUT reading — distinct from cancel.
        for _ in 0..3 {
            match stream.next().await {
                Some(Ok(proto::FrameCaptureEvent {
                    msg: Some(proto::frame_capture_event::Msg::Frame(_)),
                })) => {}
                other => return Err(format!("expected a frame, got {other:?}")),
            }
        }
        let stall_started = std::time::Instant::now();
        tokio::time::sleep(std::time::Duration::from_secs(35)).await;

        // Resume reading: the buffered frames drain, then the terminal
        // RunResponse must arrive with the Pause-equivalent stop.
        let mut done = None;
        while let Some(event) = stream.next().await {
            match event.map_err(|e| format!("stream event: {e}"))?.msg {
                Some(proto::frame_capture_event::Msg::Frame(_)) => {}
                Some(proto::frame_capture_event::Msg::Done(response)) => {
                    done = Some(response);
                }
                None => return Err("FrameCaptureEvent without msg".into()),
            }
        }
        let done =
            done.ok_or_else(|| "stream ended without terminal RunResponse".to_string())?;
        if done.reason != i32::from(proto::StopReason::Paused) {
            return Err(format!(
                "watchdog stop must report PAUSED, got reason {}",
                done.reason
            ));
        }
        if stall_started.elapsed() < std::time::Duration::from_secs(30) {
            return Err("terminal event arrived before the 30s watchdog deadline".into());
        }

        // The slot is Paused and reusable: a follow-up Run must succeed.
        let follow_up = ready
            .svc
            .run(Request::new(proto::RunRequest {
                lease: Some(ready.lease.clone()),
                until: Some(proto::run_request::Until::IcountBudget(1_000_000)),
                hard_icount_cap: 0,
                capture: None,
            }))
            .await
            .map_err(|e| format!("follow-up Run after watchdog: {e}"))?
            .into_inner();
        if follow_up.icount <= done.icount {
            return Err("follow-up Run did not advance from the watchdog boundary".into());
        }
        destroy(&ready.svc, &ready.lease).await;
        Ok::<(), String>(())
    })?;
    Ok(())
}
