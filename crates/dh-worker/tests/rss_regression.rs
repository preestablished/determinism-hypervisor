#![cfg(target_arch = "x86_64")]

//! Bounded-RSS regression guard for long streaming Runs (bead
//! determinism-hypervisor-9f3x; plan
//! `.agents/plans/run-with-frame-capture-memory-leak-oom/03`).
//!
//! Guards against the 2026-07-07 production OOM: `dh-workerd` grew
//! ~300–500 MB/s inside one long `RunWithFrameCapture` (root cause: the
//! run agenda was materialized for the whole icount budget —
//! `dh_vmm::agenda`, fixed by `AgendaIter`) and was killed at ~26 GB.
//! The worker service runs in-process here, so test RSS == worker RSS.
//!
//! Two assertions:
//!
//! 1. CEILING — max RSS over the run must stay under a bound derived
//!    mechanically (see `rss_bound_kb` for the derivation and inputs),
//!    NOT scaling with run duration or budget. Catches the loud leak.
//! 2. PLATEAU — RSS must stop growing once warmed up: the windowed
//!    median over the final third of the run must not exceed the
//!    median at the end of the warm-up window by more than
//!    `PLATEAU_TOLERANCE`. Catches the slow (MB/s-scale) leak a
//!    generous ceiling would miss until a multi-hour soak.
//!
//! Duration: `DH_RSS_GUARD_SECS` (default 180). Below 120 s only the
//! ceiling runs — the plateau needs a real warm-up window. Evidence CSV
//! (1 Hz RSS + allocator counters) is written under `target/` and its
//! path printed either way; on failure it is the diagnostic.
//!
//! M9-gated lab-lane test like its siblings (requires KVM dirty-ring +
//! staged DH_M9_* artifacts; run --release; docs/ops/test-partitioning.md).

mod common;

use std::io::Write as _;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::TestResult;
use dh_proto::v1 as proto;
use dh_proto::v1::hypervisor_worker_server::HypervisorWorker;
use tokio_stream::StreamExt;
use tonic::Request;

/// Effectively unbounded: the incident shape. The run ends by stream
/// drop (cancel at a frame boundary), not by budget.
const HUGE_BUDGET: u64 = u64::MAX / 4;

/// Plateau tolerance: final-third median ≤ warm-up median × (1 + this).
/// 10% was chosen against the post-fix profile (2026-07-07: growth over
/// 180 s was 12 kB total ≈ 0.002% — the tolerance is ~5000× the
/// observed steady-state drift, yet a 1 MB/s slow leak overshoots it
/// in under a minute of tail).
const PLATEAU_TOLERANCE: f64 = 0.10;

/// Ceiling derivation (plan 03; all inputs printed at runtime):
///
///   bound = (baseline_rss                 measured in-process at READY,
///                                         idle — covers the test/image
///                                         cache/snapstore fixture
///            + slots × mem_bytes          headroom for one guest-size
///                                         of legitimate transient/
///                                         shadow growth per slot;
///                                         slots = 1 here, mem_bytes =
///                                         common::M9_LINUX_MEM_BYTES
///           ) × 1.25                      allocator slack + stream
///                                         buffers margin
///
/// The bound deliberately does NOT scale with duration or budget —
/// that is the property under test.
fn rss_bound_kb(baseline_kb: u64, slots: u64, mem_bytes: u64) -> u64 {
    (baseline_kb + slots * (mem_bytes / 1024)) * 5 / 4
}

fn read_proc_status() -> (u64, u64, u64) {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let field = |name: &str| -> u64 {
        status
            .lines()
            .find(|l| l.starts_with(name))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    };
    (field("VmRSS:"), field("VmHWM:"), field("RssAnon:"))
}

/// (arena_kb, uordblks_kb, fordblks_kb, mmapped_kb) from mallinfo2 —
/// on failure the CSV distinguishes live allocation growth (uordblks)
/// from allocator retention of freed memory (fordblks/hblkhd).
fn read_mallinfo() -> (u64, u64, u64, u64) {
    // SAFETY: mallinfo2 is a plain libc call returning a struct by value.
    let mi = unsafe { libc::mallinfo2() };
    (
        (mi.arena / 1024) as u64,
        (mi.uordblks / 1024) as u64,
        (mi.fordblks / 1024) as u64,
        (mi.hblkhd / 1024) as u64,
    )
}

fn median(mut vals: Vec<u64>) -> u64 {
    if vals.is_empty() {
        return 0;
    }
    vals.sort_unstable();
    vals[vals.len() / 2]
}

#[test]
#[ignore = "9f3x RSS lab-lane guard: requires KVM dirty-ring support and staged DH_M9_* artifacts; use --release"]
fn linux_streaming_run_rss_stays_bounded_and_plateaus() -> TestResult<()> {
    let guard_secs: u64 = std::env::var("DH_RSS_GUARD_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(180);

    let Some(ready) = common::m9_linux_ready_snapshot(
        "rss_regression::linux_streaming_run_rss_stays_bounded_and_plateaus",
        1,
    )?
    else {
        return Ok(());
    };

    let evidence_dir = std::path::PathBuf::from(format!(
        "{}/rss-guard-{}",
        option_env!("CARGO_TARGET_DIR").unwrap_or("target"),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&evidence_dir).map_err(|e| format!("evidence dir: {e}"))?;
    let csv_path = evidence_dir.join("rss.csv");
    let mut csv = std::fs::File::create(&csv_path).map_err(|e| format!("csv: {e}"))?;
    writeln!(
        csv,
        "elapsed_s,vmrss_kb,vmhwm_kb,rssanon_kb,arena_kb,uordblks_kb,fordblks_kb,hblkhd_kb,frames"
    )
    .map_err(|e| format!("csv: {e}"))?;

    let (baseline_kb, _, baseline_anon) = read_proc_status();
    let bound_kb = rss_bound_kb(baseline_kb, 1, common::M9_LINUX_MEM_BYTES);
    eprintln!(
        "rss-guard: baseline VmRSS={baseline_kb} kB RssAnon={baseline_anon} kB; \
         bound=(baseline + 1x{} MiB guest) x1.25 = {bound_kb} kB; duration {guard_secs}s",
        common::M9_LINUX_MEM_BYTES / (1024 * 1024)
    );

    let stop = Arc::new(AtomicBool::new(false));
    let frames_seen = Arc::new(AtomicU64::new(0));

    // Sampler thread: 1 Hz, streamed to disk so evidence survives even a
    // kill; trips `stop` at the bound so a real leak aborts the run
    // instead of OOM-killing a shared host (the incident took two
    // unrelated k8s pods down before the worker died).
    let sampler = {
        let stop = stop.clone();
        let frames_seen = frames_seen.clone();
        let started = Instant::now();
        let mut csv = csv;
        std::thread::spawn(move || -> Vec<(f64, u64)> {
            let mut series = Vec::new();
            loop {
                let (rss, hwm, anon) = read_proc_status();
                let (arena, uord, ford, hblk) = read_mallinfo();
                let t = started.elapsed().as_secs_f64();
                let _ = writeln!(
                    csv,
                    "{t:.1},{rss},{hwm},{anon},{arena},{uord},{ford},{hblk},{}",
                    frames_seen.load(Ordering::Relaxed)
                );
                let _ = csv.flush();
                series.push((t, rss));
                if rss >= bound_kb {
                    eprintln!(
                        "rss-guard: CEILING crossed at {t:.1}s: VmRSS={rss} kB >= {bound_kb} kB"
                    );
                    stop.store(true, Ordering::Relaxed);
                    break;
                }
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(1000));
            }
            series
        })
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| format!("test runtime: {e}"))?;

    let stream_result: Result<u64, String> = rt.block_on(async {
        let response = ready
            .svc
            .run_with_frame_capture(Request::new(proto::RunWithFrameCaptureRequest {
                lease: Some(ready.lease.clone()),
                until: Some(proto::run_with_frame_capture_request::Until::IcountBudget(
                    HUGE_BUDGET,
                )),
                hard_icount_cap: 0,
            }))
            .await
            .map_err(|e| format!("RunWithFrameCapture: {e}"))?;
        let mut stream = response.into_inner();
        let started = Instant::now();
        // Paced ~60 Hz consumer, frames dropped after counting (retaining
        // them would be a harness-side leak).
        loop {
            if started.elapsed() >= Duration::from_secs(guard_secs) || stop.load(Ordering::Relaxed)
            {
                break; // drop the stream: cancel lands Paused at a frame boundary
            }
            match tokio::time::timeout(Duration::from_secs(1), stream.next()).await {
                Err(_) => continue, // no frame this tick; keep watching stop/deadline
                Ok(None) => break,
                Ok(Some(event)) => {
                    let event = event.map_err(|e| format!("stream event: {e}"))?;
                    match event.msg {
                        Some(proto::frame_capture_event::Msg::Frame(_)) => {
                            frames_seen.fetch_add(1, Ordering::Relaxed);
                            tokio::time::sleep(Duration::from_micros(16_600)).await;
                        }
                        Some(proto::frame_capture_event::Msg::Done(_)) | None => break,
                    }
                }
            }
        }
        let frames = frames_seen.load(Ordering::Relaxed);
        drop(stream);
        // Let the worker-side cancel path land Paused before sampling ends.
        tokio::time::sleep(Duration::from_secs(3)).await;
        Ok(frames)
    });

    stop.store(true, Ordering::Relaxed);
    let series = sampler.join().map_err(|_| "sampler thread panicked")?;
    let frames = stream_result?;
    let max_rss = series.iter().map(|&(_, r)| r).max().unwrap_or(0);
    eprintln!(
        "rss-guard: frames={frames} samples={} max VmRSS={max_rss} kB (bound {bound_kb} kB); evidence {}",
        series.len(),
        csv_path.display()
    );

    // 1. CEILING.
    if max_rss >= bound_kb {
        return Err(format!(
            "RSS ceiling violated: max VmRSS {max_rss} kB >= bound {bound_kb} kB \
             (baseline {baseline_kb} kB); RSS must not scale with run length — \
             see {} for the full series",
            csv_path.display()
        ));
    }

    // 2. PLATEAU (windowed medians; skipped below 120 s — no real warm-up).
    if guard_secs >= 120 {
        let warmup_end = (guard_secs.max(180) / 3).max(60) as f64;
        let warm: Vec<u64> = series
            .iter()
            .filter(|&&(t, _)| t > warmup_end - 10.0 && t <= warmup_end)
            .map(|&(_, r)| r)
            .collect();
        let tail_start = guard_secs as f64 * 2.0 / 3.0;
        let tail: Vec<u64> = series
            .iter()
            .filter(|&&(t, _)| t >= tail_start)
            .map(|&(_, r)| r)
            .collect();
        let warm_med = median(warm);
        let tail_med = median(tail);
        eprintln!(
            "rss-guard: plateau warm_median={warm_med} kB (@{warmup_end:.0}s) \
             tail_median={tail_med} kB (>= {tail_start:.0}s) tolerance {PLATEAU_TOLERANCE}"
        );
        if warm_med == 0 || tail_med == 0 {
            return Err("plateau check got empty sample windows — run too short?".into());
        }
        if (tail_med as f64) > (warm_med as f64) * (1.0 + PLATEAU_TOLERANCE) {
            return Err(format!(
                "RSS failed to plateau: final-third median {tail_med} kB > warm-up \
                 median {warm_med} kB x{:.2} — a slow leak; see {}",
                1.0 + PLATEAU_TOLERANCE,
                csv_path.display()
            ));
        }
    } else {
        eprintln!("rss-guard: plateau check skipped (duration < 120 s)");
    }

    // The stream must have actually streamed play — a zero-frame "pass"
    // would mean the run never reached the workload (the pre-fix bug's
    // other symptom: agenda compilation starved the guest entirely).
    if frames == 0 {
        return Err("no frames streamed — the guard did not exercise the play path".into());
    }
    Ok(())
}
