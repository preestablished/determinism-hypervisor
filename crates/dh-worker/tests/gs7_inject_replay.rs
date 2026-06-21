//! GS-7 Linux inject_point acceptance gate.
//!
//! This is intentionally ignored: it needs the staged M9 Linux artifacts, KVM
//! dirty-ring support, and the sibling guest-sdk/reference-workload fixture to
//! emit real `inject_point` traffic with non-trivial fault decisions.

#![cfg(target_arch = "x86_64")]

mod common;

use std::collections::BTreeSet;

use common::TestResult;
use detguest_wire::events::log_stream;
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
const OBSERVED_DECISIONS_PREFIX: &str = "gs7.inject_decisions=";

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
    packed_value: u32,
    decision: FaultDecision,
}

fn canonical_decision(value: u32, context: &str) -> TestResult<FaultDecision> {
    let decision = FaultDecision::unpack(value);
    if decision.pack() != value {
        return Err(format!(
            "{context} has non-canonical packed FaultDecision {value:#010x}"
        ));
    }
    Ok(decision)
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
        let decision = canonical_decision(value, &format!("PIO_ANSWER seq {}", record.seq()))?;
        answers.push(RecordedInjectAnswer {
            seq: record.seq(),
            icount: record.icount(),
            packed_value: value,
            decision,
        });
    }
    Ok(answers)
}

fn assert_adjacent_iseq_increments(queries: &[InjectQueryEvidence]) -> TestResult<()> {
    for pair in queries.windows(2) {
        let [previous, next] = pair else {
            unreachable!("windows(2) yields two elements")
        };
        if next.iseq != previous.iseq.wrapping_add(1) {
            return Err(format!(
                "InjectQuery iseq jumped from {} at icount {} to {} at icount {}; expected adjacent increments",
                previous.iseq, previous.icount, next.iseq, next.icount
            ));
        }
    }
    Ok(())
}

fn decode_log_line_msg(event: &proto::GuestEvent) -> TestResult<Option<&str>> {
    if event.stream != EventKind::LogLine as u32 {
        return Ok(None);
    }
    if event.payload.len() < 8 {
        return Err(format!(
            "LogLine payload at icount {} must be at least 8 bytes, got {}",
            event.icount,
            event.payload.len()
        ));
    }
    let msg_len = u16::from_le_bytes(event.payload[2..4].try_into().unwrap()) as usize;
    if 8 + msg_len > event.payload.len() {
        return Err(format!(
            "LogLine payload at icount {} declares msg_len {}, but payload has {} bytes",
            event.icount,
            msg_len,
            event.payload.len()
        ));
    }
    let msg = std::str::from_utf8(&event.payload[8..8 + msg_len]).map_err(|e| {
        format!(
            "LogLine payload at icount {} is not UTF-8: {e}",
            event.icount
        )
    })?;
    Ok(Some(msg))
}

fn parse_observed_decision_list(raw: &str) -> TestResult<Vec<FaultDecision>> {
    if raw.trim().is_empty() {
        return Err(format!(
            "{OBSERVED_DECISIONS_PREFIX} fixture line must include at least one decision"
        ));
    }
    raw.split(',')
        .enumerate()
        .map(|(index, token)| {
            let token = token.trim();
            let hex = token.strip_prefix("0x").unwrap_or(token);
            let value = u32::from_str_radix(hex, 16).map_err(|e| {
                format!(
                    "{OBSERVED_DECISIONS_PREFIX} token {index} ({token:?}) is not packed u32 hex: {e}"
                )
            })?;
            canonical_decision(value, &format!("{OBSERVED_DECISIONS_PREFIX} token {index}"))
        })
        .collect()
}

fn observed_workload_decisions(events: &[proto::GuestEvent]) -> TestResult<Vec<FaultDecision>> {
    let mut observed = Vec::new();
    for event in events {
        let Some(msg) = decode_log_line_msg(event)? else {
            continue;
        };
        let Some(raw) = msg.strip_prefix(OBSERVED_DECISIONS_PREFIX) else {
            continue;
        };
        observed.push(parse_observed_decision_list(raw)?);
    }
    match observed.len() {
        0 => Err(format!(
            "GS-7 fixture emitted no workload-observed decision proof; expected one LogLine message with prefix {OBSERVED_DECISIONS_PREFIX:?}"
        )),
        1 => Ok(observed.remove(0)),
        n => Err(format!(
            "GS-7 fixture emitted {n} workload-observed decision proof LogLines; expected exactly one"
        )),
    }
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
    let (run, events, post_snapshot, verify_done, drained_before_segment) = rt.block_on(async {
        let drained_before_segment = stream_guest_events(&ready.svc, ready.lease.clone()).await?;
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

        Ok::<_, String>((
            run,
            events,
            post_snapshot,
            verify_done,
            drained_before_segment,
        ))
    })?;

    let queries = stream_inject_queries(&events)?;
    if let Some(query) = queries
        .iter()
        .find(|query| query.icount <= ready.ready_snapshot.icount)
    {
        return Err(format!(
            "post-READY GS-7 stream included InjectQuery at icount {} before or at READY snapshot icount {}; READY backlog was not isolated",
            query.icount, ready.ready_snapshot.icount
        ));
    }
    if queries.len() < MIN_INJECT_QUERIES {
        return Err(format!(
            "GS-7 fixture emitted {} InjectQuery events; expected at least {MIN_INJECT_QUERIES}. \
             This usually means the sibling guest-sdk/reference-workload fixture has not enabled real inject_point traffic yet.",
            queries.len()
        ));
    }
    assert_adjacent_iseq_increments(&queries)?;

    let log = common::input_log_payload(&ready.store, &post_snapshot.input_log_id)?;
    let answers = recorded_inject_answers(&log)?;
    if answers.len() != queries.len() {
        return Err(format!(
            "GS-7 DHILOG recorded {} PORT_INJECT PIO_ANSWER values, but StreamGuestEvents saw {} InjectQuery events",
            answers.len(),
            queries.len()
        ));
    }

    let observed_decisions = observed_workload_decisions(&events)?;
    let recorded_decisions: Vec<_> = answers.iter().map(|answer| answer.decision).collect();
    if observed_decisions != recorded_decisions {
        return Err(format!(
            "GS-7 workload-observed decisions did not match recorded PIO_ANSWER values: observed={observed_decisions:?} recorded={recorded_decisions:?}"
        ));
    }

    let nontrivial: Vec<_> = answers
        .iter()
        .copied()
        .filter(|answer| !matches!(answer.decision, FaultDecision::Proceed))
        .collect();
    if nontrivial.len() < MIN_NONTRIVIAL_DECISIONS {
        return Err(format!(
            "GS-7 DHILOG recorded {} non-Proceed inject decisions; expected at least {MIN_NONTRIVIAL_DECISIONS}",
            nontrivial.len()
        ));
    }
    let distinct_nontrivial: BTreeSet<u32> = nontrivial
        .iter()
        .map(|answer| answer.packed_value)
        .collect();
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
        "gs7-inject-replay icount={} drained_before_segment={} queries={} answers={} observed_decisions={} nontrivial={} distinct_nontrivial={} first_answer_seq={} first_answer_icount={} first_query_name_id={}",
        run.icount,
        drained_before_segment.len(),
        queries.len(),
        answers.len(),
        observed_decisions.len(),
        nontrivial.len(),
        distinct_nontrivial.len(),
        answers.first().map(|answer| answer.seq).unwrap_or_default(),
        answers.first().map(|answer| answer.icount).unwrap_or_default(),
        queries.first().map(|query| query.name_id).unwrap_or_default(),
    );

    Ok(())
}

#[test]
fn gs7_observed_decision_logline_parser_accepts_canonical_hex() {
    let payload = log_line_payload(
        log_stream::SDK_USER,
        &format!("{OBSERVED_DECISIONS_PREFIX}0x00020002,0xffffffc8"),
    );
    let events = vec![proto::GuestEvent {
        stream: EventKind::LogLine as u32,
        icount: 42,
        vns: 42,
        payload,
    }];
    assert_eq!(
        observed_workload_decisions(&events).unwrap(),
        vec![
            FaultDecision::Platform { kind: 2, arg: 512 },
            FaultDecision::Workload {
                kind: 200,
                arg: 0x00ff_ffff
            }
        ]
    );
}

#[test]
fn gs7_canonical_decision_rejects_kind_zero_with_arg_bits() {
    assert!(canonical_decision(0x0000_0100, "test").is_err());
}

#[test]
fn gs7_iseq_check_allows_nonzero_segment_start() {
    let queries = [
        InjectQueryEvidence {
            icount: 100,
            iseq: 7,
            name_id: 1,
        },
        InjectQueryEvidence {
            icount: 110,
            iseq: 8,
            name_id: 1,
        },
    ];
    assert!(assert_adjacent_iseq_increments(&queries).is_ok());
}

fn log_line_payload(stream: u8, msg: &str) -> Vec<u8> {
    let mut payload = vec![0u8; 8 + msg.len()];
    payload[0] = stream;
    payload[1] = 2;
    payload[2..4].copy_from_slice(&(msg.len() as u16).to_le_bytes());
    payload[8..].copy_from_slice(msg.as_bytes());
    payload
}
