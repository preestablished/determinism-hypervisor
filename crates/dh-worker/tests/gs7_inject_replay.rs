//! GS-7 Linux inject_point acceptance gate.
//!
//! This is intentionally ignored: it needs the staged M9 Linux artifacts, KVM
//! dirty-ring support, and the sibling guest-sdk/reference-workload fixture to
//! emit real `inject_point` traffic with non-trivial fault decisions.

#![cfg(target_arch = "x86_64")]

mod common;

use std::collections::BTreeSet;

use common::TestResult;
use detguest_wire::ports::PORT_INJECT;
use detguest_wire::record::EventKind;
use detguest_wire::FaultDecision;
use dh_inputlog::reader::{LogReader, RecordBody};
use dh_proto::v1 as proto;
use dh_proto::v1::hypervisor_worker_server::HypervisorWorker;
use tokio_stream::StreamExt;
use tonic::Request;

const GS7_INJECT_FRAMES: u32 = 1;
const GS7_INJECT_HARD_CAP: u64 = 50_000_000;
const MIN_INJECT_QUERIES: usize = 2;
const MIN_NONTRIVIAL_DECISIONS: usize = 2;

#[derive(Clone, Copy, Debug)]
struct InjectQueryEvidence {
    icount: u64,
    iseq: u32,
    name_id: u32,
}

#[derive(Clone, Copy, Debug)]
struct RecordedInjectAnswer {
    seq: u32,
    icount: u64,
    value: u32,
}

fn decode_inject_query(event: &proto::GuestEvent) -> TestResult<Option<InjectQueryEvidence>> {
    if event.stream != EventKind::InjectQuery as u32 {
        return Ok(None);
    }
    if event.payload.len() != 8 {
        return Err(format!(
            "InjectQuery payload at icount {} must be 8 bytes, got {}",
            event.icount,
            event.payload.len()
        ));
    }
    let iseq = u32::from_le_bytes(event.payload[0..4].try_into().unwrap());
    let name_id = u32::from_le_bytes(event.payload[4..8].try_into().unwrap());
    Ok(Some(InjectQueryEvidence {
        icount: event.icount,
        iseq,
        name_id,
    }))
}

fn stream_inject_queries(events: &[proto::GuestEvent]) -> TestResult<Vec<InjectQueryEvidence>> {
    let mut queries = Vec::new();
    for event in events {
        if let Some(query) = decode_inject_query(event)? {
            queries.push(query);
        }
    }
    Ok(queries)
}

fn recorded_inject_answers(log: &[u8]) -> TestResult<Vec<RecordedInjectAnswer>> {
    let reader = LogReader::parse(log).map_err(|e| format!("GS-7 DHILOG parse failed: {e:?}"))?;
    let mut answers = Vec::new();
    for record in reader.canonical() {
        let RecordBody::DevEvent {
            device_id,
            event_type,
            data,
        } = record.body()
        else {
            continue;
        };
        if device_id != dh_inputlog::dhilog::DEVICE_ID_DETCHANNEL
            || event_type != dh_inputlog::dhilog::EVENT_PIO_ANSWER
        {
            continue;
        }
        if data.len() != 8 {
            return Err(format!(
                "PIO_ANSWER seq {} payload must be 8 bytes, got {}",
                record.seq(),
                data.len()
            ));
        }
        let port = u16::from_le_bytes(data[0..2].try_into().unwrap());
        if port != PORT_INJECT {
            continue;
        }
        let value = u32::from_le_bytes(data[4..8].try_into().unwrap());
        answers.push(RecordedInjectAnswer {
            seq: record.seq(),
            icount: record.icount(),
            value,
        });
    }
    Ok(answers)
}

fn assert_contiguous_iseq(queries: &[InjectQueryEvidence]) -> TestResult<()> {
    for (expected, query) in queries.iter().enumerate() {
        if query.iseq != expected as u32 {
            return Err(format!(
                "InjectQuery at icount {} had iseq {}, expected contiguous {}",
                query.icount, query.iseq, expected
            ));
        }
    }
    Ok(())
}

async fn stream_guest_events(
    svc: &dh_worker::service::WorkerService,
    lease: proto::Lease,
) -> TestResult<Vec<proto::GuestEvent>> {
    let mut stream = svc
        .stream_guest_events(Request::new(proto::StreamGuestEventsRequest {
            lease: Some(lease),
            streams: Vec::new(),
        }))
        .await
        .map_err(|e| format!("StreamGuestEvents: {e}"))?
        .into_inner();
    let mut events = Vec::new();
    while let Some(event) = stream.as_mut().next().await {
        events.push(event.map_err(|e| format!("StreamGuestEvents item: {e}"))?);
    }
    Ok(events)
}

#[test]
#[ignore = "GS-7 Linux acceptance: requires M9 artifacts, KVM, and guest-sdk inject_point fixture traffic"]
fn linux_gs7_inject_points_verify_replay_from_dhilog() -> TestResult<()> {
    let Some(ready) = common::m9_linux_ready_snapshot(
        "gs7_inject_replay::linux_gs7_inject_points_verify_replay_from_dhilog",
        2,
    )?
    else {
        return Ok(());
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("test runtime: {e}"))?;
    let (run, events, post_snapshot, verify_done) = rt.block_on(async {
        let run = ready
            .svc
            .run(Request::new(proto::RunRequest {
                lease: Some(ready.lease.clone()),
                until: Some(proto::run_request::Until::FrameBudget(GS7_INJECT_FRAMES)),
                hard_icount_cap: GS7_INJECT_HARD_CAP,
                capture: None,
            }))
            .await
            .map_err(|e| format!("Run GS-7 inject segment: {e}"))?
            .into_inner();
        if run.reason != i32::from(proto::StopReason::BudgetReached) {
            return Err(format!(
                "GS-7 inject segment stopped with {}, expected BudgetReached",
                run.reason
            ));
        }
        if run.frames_elapsed != u64::from(GS7_INJECT_FRAMES) {
            return Err(format!(
                "GS-7 inject segment frames_elapsed {}, expected {GS7_INJECT_FRAMES}",
                run.frames_elapsed
            ));
        }

        let events = stream_guest_events(&ready.svc, ready.lease.clone()).await?;
        let post_snapshot = ready
            .svc
            .take_snapshot(Request::new(proto::TakeSnapshotRequest {
                lease: Some(ready.lease.clone()),
                seal_input_log: Some(true),
                capture: None,
            }))
            .await
            .map_err(|e| format!("TakeSnapshot GS-7 inject segment: {e}"))?
            .into_inner();
        let verify_done = common::verify_replay_done(
            &ready.svc,
            ready.ready_snapshot_ref.clone(),
            post_snapshot.input_log_id.clone(),
        )
        .await?;

        let _ = ready
            .svc
            .destroy_vm(Request::new(proto::DestroyVmRequest {
                lease: Some(ready.lease.clone()),
            }))
            .await;

        Ok::<_, String>((run, events, post_snapshot, verify_done))
    })?;

    let queries = stream_inject_queries(&events)?;
    if queries.len() < MIN_INJECT_QUERIES {
        return Err(format!(
            "GS-7 fixture emitted {} InjectQuery events; expected at least {MIN_INJECT_QUERIES}. \
             This usually means the sibling guest-sdk/reference-workload fixture has not enabled real inject_point traffic yet.",
            queries.len()
        ));
    }
    assert_contiguous_iseq(&queries)?;

    let log = common::input_log_payload(&ready.store, &post_snapshot.input_log_id)?;
    let answers = recorded_inject_answers(&log)?;
    if answers.len() != queries.len() {
        return Err(format!(
            "GS-7 DHILOG recorded {} PORT_INJECT PIO_ANSWER values, but StreamGuestEvents saw {} InjectQuery events",
            answers.len(),
            queries.len()
        ));
    }

    let nontrivial: Vec<_> = answers
        .iter()
        .copied()
        .filter(|answer| answer.value != FaultDecision::Proceed.pack())
        .collect();
    if nontrivial.len() < MIN_NONTRIVIAL_DECISIONS {
        return Err(format!(
            "GS-7 DHILOG recorded {} non-Proceed inject decisions; expected at least {MIN_NONTRIVIAL_DECISIONS}",
            nontrivial.len()
        ));
    }
    let distinct_nontrivial: BTreeSet<u32> = nontrivial.iter().map(|answer| answer.value).collect();
    if distinct_nontrivial.len() < MIN_NONTRIVIAL_DECISIONS {
        return Err(format!(
            "GS-7 DHILOG recorded {} distinct non-Proceed inject decisions; expected at least {MIN_NONTRIVIAL_DECISIONS}",
            distinct_nontrivial.len()
        ));
    }

    let live_end_hash = post_snapshot
        .state_hash
        .as_ref()
        .ok_or_else(|| "GS-7 snapshot returned no state hash".to_string())?;
    assert_eq!(
        verify_done
            .end_state_hash
            .ok_or_else(|| "VerifyReplay returned no end state hash".to_string())?
            .hash,
        live_end_hash.hash,
        "VerifyReplay must reproduce the inject-bearing Linux segment from only the DHILOG"
    );
    assert_eq!(
        verify_done.total_icount, run.icount,
        "VerifyReplay total_icount must match the recorded inject segment"
    );

    eprintln!(
        "gs7-inject-replay icount={} queries={} answers={} nontrivial={} distinct_nontrivial={} first_answer_seq={} first_answer_icount={} first_query_name_id={}",
        run.icount,
        queries.len(),
        answers.len(),
        nontrivial.len(),
        distinct_nontrivial.len(),
        answers.first().map(|answer| answer.seq).unwrap_or_default(),
        answers.first().map(|answer| answer.icount).unwrap_or_default(),
        queries.first().map(|query| query.name_id).unwrap_or_default(),
    );

    Ok(())
}
