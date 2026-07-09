//! M0 play-path perf attribution + relative regression guard
//! (play-60fps plan, 01/04).
//!
//! Phase A drives the bridge's historical per-frame loop —
//! `Run{frame_budget=1}` + `GetFramebuffer` — and times each stage; every
//! per-frame Run stop pushes a full-memory `hash_final_stop` chain link,
//! which is the cost M2 removes. Phase B replays the same instruction
//! span through `RunWithFrameCapture` with a fast consumer.
//!
//! The assertion is deliberately RELATIVE (streamed fps must not fall
//! below the per-frame-Run fps measured in the same process on the same
//! box): the shared kvm-intel runner makes absolute wall-clock gates
//! flaky, and the regression this guards is the ratio collapsing —
//! e.g. someone reintroducing a per-frame chain link in the streaming
//! path. Absolute fps acceptance stays an operator-run step
//! (docs/ops/test-partitioning.md). Run with --release for numbers that
//! mean anything; the printed table feeds
//! .agents/plans/play-60fps-decouple-hash-from-frames/05-measurements.md.

mod common;

use std::time::{Duration, Instant};

use common::TestResult;
use dh_proto::v1 as proto;
use dh_proto::v1::hypervisor_worker_server::HypervisorWorker;
use tokio_stream::StreamExt;
use tonic::Request;

const PER_FRAME_SAMPLES: u64 = 240;
/// Per-frame Run hard cap: ~2 frames of headroom over the measured
/// workload. Each Run asserts BUDGET_REACHED, so if instructions/frame
/// drifts past the cap the smoke fails loudly instead of silently
/// skewing instr_per_frame and the fps ratio.
const FRAME_HARD_CAP: u64 = 2 * common::M9_INSTR_PER_FRAME_ESTIMATE;

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

#[test]
#[ignore = "M9 Linux perf smoke: requires KVM dirty-ring support and staged DH_M9_* artifacts; use --release"]
fn linux_play_path_perf_smoke() -> TestResult<()> {
    let Some(ready) =
        common::m9_linux_ready_snapshot("play_perf_smoke::linux_play_path_perf_smoke", 2)?
    else {
        return Ok(());
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("test runtime: {e}"))?;
    rt.block_on(async {
        // ---- Phase A: per-frame Run + GetFramebuffer (bridge play loop) ----
        let start_icount = ready.ready_snapshot.icount;
        let mut run_total = Duration::ZERO;
        let mut fb_total = Duration::ZERO;
        let mut run_max = Duration::ZERO;
        let mut last_icount = start_icount;
        for i in 0..PER_FRAME_SAMPLES {
            let t = Instant::now();
            let run = ready
                .svc
                .run(Request::new(proto::RunRequest {
                    lease: Some(ready.lease.clone()),
                    until: Some(proto::run_request::Until::FrameBudget(1)),
                    hard_icount_cap: FRAME_HARD_CAP,
                    capture: None,
                }))
                .await
                .map_err(|e| format!("per-frame Run {i}: {e}"))?
                .into_inner();
            if run.reason != i32::from(proto::StopReason::BudgetReached) {
                return Err(format!(
                    "per-frame Run {i} stopped with reason {} instead of BUDGET_REACHED — \
                     FRAME_HARD_CAP / M9_INSTR_PER_FRAME_ESTIMATE is stale",
                    run.reason
                ));
            }
            let run_elapsed = t.elapsed();
            run_total += run_elapsed;
            run_max = run_max.max(run_elapsed);
            last_icount = run.icount;

            let t = Instant::now();
            ready
                .svc
                .get_framebuffer(Request::new(proto::GetFramebufferRequest {
                    lease: Some(ready.lease.clone()),
                }))
                .await
                .map_err(|e| format!("GetFramebuffer {i}: {e}"))?;
            fb_total += t.elapsed();
        }
        let span_icount = last_icount - start_icount;
        let instr_per_frame = span_icount / PER_FRAME_SAMPLES;
        let per_frame_wall = run_total + fb_total;
        let per_frame_fps = PER_FRAME_SAMPLES as f64 / per_frame_wall.as_secs_f64();
        destroy(&ready.svc, &ready.lease).await;

        // ---- Phase B: one streaming run over the same instruction span ----
        let restored = ready
            .svc
            .restore_snapshot(Request::new(proto::RestoreSnapshotRequest {
                snapshot: Some(ready.ready_snapshot_ref.clone()),
                entropy_seed: Vec::new(),
                baseline: None,
            }))
            .await
            .map_err(|e| format!("RestoreSnapshot: {e}"))?
            .into_inner();
        let stream_lease = restored
            .lease
            .ok_or_else(|| "RestoreSnapshot returned no lease".to_string())?;
        let t = Instant::now();
        let mut stream = ready
            .svc
            .run_with_frame_capture(Request::new(proto::RunWithFrameCaptureRequest {
                lease: Some(stream_lease.clone()),
                until: Some(proto::run_with_frame_capture_request::Until::IcountBudget(
                    span_icount,
                )),
                hard_icount_cap: 0,
            }))
            .await
            .map_err(|e| format!("RunWithFrameCapture: {e}"))?
            .into_inner();
        let mut streamed_frames = 0u64;
        let mut first_frame_at = None;
        while let Some(event) = stream.next().await {
            match event.map_err(|e| format!("stream event: {e}"))?.msg {
                Some(proto::frame_capture_event::Msg::Frame(_)) => {
                    first_frame_at.get_or_insert_with(|| t.elapsed());
                    streamed_frames += 1;
                }
                Some(proto::frame_capture_event::Msg::Done(_)) => {}
                None => return Err("FrameCaptureEvent without msg".into()),
            }
        }
        let stream_wall = t.elapsed();
        destroy(&ready.svc, &stream_lease).await;
        let streamed_fps = streamed_frames as f64 / stream_wall.as_secs_f64();

        let profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        let epochs_crossed = span_icount / 50_000_000;
        println!("== play-path perf smoke (build_profile={profile}) ==");
        println!("frames sampled (per-frame path):  {PER_FRAME_SAMPLES}");
        println!("instructions/frame:               {instr_per_frame}");
        println!("epoch links in span (~50M grid):  {epochs_crossed}");
        println!(
            "per-frame Run avg/max:            {:.2} ms / {:.2} ms",
            ms(run_total) / PER_FRAME_SAMPLES as f64,
            ms(run_max)
        );
        println!(
            "per-frame GetFramebuffer avg:     {:.2} ms",
            ms(fb_total) / PER_FRAME_SAMPLES as f64
        );
        println!("per-frame path fps:               {per_frame_fps:.1}");
        println!(
            "streaming: {streamed_frames} frames in {:.1} ms (first frame at {:.1} ms)",
            ms(stream_wall),
            first_frame_at.map(ms).unwrap_or(f64::NAN)
        );
        println!("streaming fps:                    {streamed_fps:.1}");
        println!(
            "streamed/per-frame fps ratio:     {:.2}x",
            streamed_fps / per_frame_fps
        );

        if streamed_fps < per_frame_fps {
            return Err(format!(
                "streaming fps ({streamed_fps:.1}) fell below the per-frame-Run baseline \
                 ({per_frame_fps:.1}) measured in the same job — a per-frame cost has crept \
                 into the streaming path"
            ));
        }
        Ok::<(), String>(())
    })?;
    Ok(())
}

async fn destroy(svc: &dh_worker::service::WorkerService, lease: &proto::Lease) {
    let _ = svc
        .destroy_vm(Request::new(proto::DestroyVmRequest {
            lease: Some(lease.clone()),
        }))
        .await;
}
