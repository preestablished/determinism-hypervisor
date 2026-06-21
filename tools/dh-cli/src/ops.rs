//! Operator RPC verbs for `dh-workerd`.
//!
//! These are client-side debugging commands over the generated
//! `determinism.hypervisor.v1.HypervisorWorker` API. They deliberately do
//! not import `dh-worker`; the daemon owns KVM/runtime state, and the CLI
//! surfaces the daemon's current runtime status honestly.

use dh_proto::v1 as proto;
use proto::hypervisor_worker_client::HypervisorWorkerClient;
use tonic::transport::Channel;
use tonic::Status;

const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:7400";

#[derive(Clone, Debug, PartialEq, Eq)]
struct OpConfig {
    endpoint: String,
    json: bool,
}

impl Default for OpConfig {
    fn default() -> Self {
        Self {
            endpoint: DEFAULT_ENDPOINT.into(),
            json: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ParsedOperator {
    config: OpConfig,
    command: OperatorCommand,
}

#[derive(Clone, Debug, PartialEq)]
enum OperatorCommand {
    Snapshot {
        lease: proto::Lease,
        seal_input_log: bool,
    },
    Restore {
        snapshot: Vec<u8>,
        entropy_seed: Vec<u8>,
    },
    Fork {
        parent: proto::Lease,
        count: u32,
        entropy_seeds: Vec<Vec<u8>>,
    },
    Replay {
        base: Vec<u8>,
        log: LogArg,
    },
    Verify {
        base: Vec<u8>,
        log: LogArg,
        bisect_on_divergence: bool,
    },
}

impl OperatorCommand {
    fn op(&self) -> &'static str {
        match self {
            Self::Snapshot { .. } => "snapshot",
            Self::Restore { .. } => "restore",
            Self::Fork { .. } => "fork",
            Self::Replay { .. } => "replay",
            Self::Verify { .. } => "verify",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LogArg {
    InlinePath(String),
    StoreId(Vec<u8>),
}

struct OpOutput {
    json: String,
    human: String,
}

#[derive(Debug)]
enum OpError {
    Io(String),
    Transport(String),
    Rpc(Status),
}

impl std::fmt::Display for OpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpError::Io(msg) | OpError::Transport(msg) => f.write_str(msg),
            OpError::Rpc(status) => {
                write!(f, "gRPC {}: {}", code_name(status), status.message())
            }
        }
    }
}

pub fn dispatch(command: &'static str, args: &[String]) {
    let parse_json = args.iter().any(|arg| arg == "--json");
    let parsed = match parse_operator(command, args) {
        Ok(parsed) => parsed,
        Err(msg) => {
            if parse_json {
                println!(
                    "{{\"op\":\"{}\",\"status\":\"error\",\"error\":{{\"kind\":\"usage\",\
                     \"message\":\"{}\"}}}}",
                    command,
                    json_escape(msg.as_bytes())
                );
            } else {
                eprintln!("dh-cli {command}: {msg}\n{}", operator_usage(command));
            }
            std::process::exit(2);
        }
    };
    let json = parsed.config.json;
    let op = parsed.command.op();

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            render_error_and_exit(op, json, OpError::Transport(format!("runtime: {e}")));
        }
    };

    let mut stdout = std::io::stdout();
    match rt.block_on(execute_to_writer(parsed, &mut stdout)) {
        Ok(()) => {}
        Err(e) => render_error_and_exit(op, json, e),
    }
}

fn render_error_and_exit(op: &str, json: bool, err: OpError) -> ! {
    if json {
        println!(
            "{{\"op\":\"{}\",\"status\":\"error\",\"error\":{}}}",
            op,
            error_json(&err)
        );
    } else {
        eprintln!("dh-cli {op}: {err}");
    }
    std::process::exit(1);
}

async fn execute_to_writer<W: std::io::Write + ?Sized>(
    parsed: ParsedOperator,
    out: &mut W,
) -> Result<(), OpError> {
    let json = parsed.config.json;
    let mut client = connect_worker(parsed.config.endpoint).await?;
    match parsed.command {
        OperatorCommand::Snapshot {
            lease,
            seal_input_log,
        } => {
            let response = client
                .take_snapshot(proto::TakeSnapshotRequest {
                    lease: Some(lease),
                    seal_input_log: Some(seal_input_log),
                    capture: None,
                })
                .await
                .map_err(OpError::Rpc)?
                .into_inner();
            write_output(out, json, snapshot_output(response))
        }
        OperatorCommand::Restore {
            snapshot,
            entropy_seed,
        } => {
            let response = client
                .restore_snapshot(proto::RestoreSnapshotRequest {
                    snapshot: Some(proto::SnapshotRef { hash: snapshot }),
                    entropy_seed,
                })
                .await
                .map_err(OpError::Rpc)?
                .into_inner();
            write_output(out, json, restore_output(response))
        }
        OperatorCommand::Fork {
            parent,
            count,
            entropy_seeds,
        } => {
            let response = client
                .fork(proto::ForkRequest {
                    parent: Some(parent),
                    count,
                    entropy_seeds,
                })
                .await
                .map_err(OpError::Rpc)?
                .into_inner();
            write_output(out, json, fork_output(response))
        }
        OperatorCommand::Replay { base, log } => {
            stream_verify_like_output(out, &mut client, "replay", base, log, false, json).await
        }
        OperatorCommand::Verify {
            base,
            log,
            bisect_on_divergence,
        } => {
            stream_verify_like_output(
                out,
                &mut client,
                "verify",
                base,
                log,
                bisect_on_divergence,
                json,
            )
            .await
        }
    }
}

fn write_output<W: std::io::Write + ?Sized>(
    out: &mut W,
    json: bool,
    output: OpOutput,
) -> Result<(), OpError> {
    let text = if json { output.json } else { output.human };
    writeln!(out, "{text}").map_err(|e| OpError::Io(format!("write stdout: {e}")))?;
    out.flush()
        .map_err(|e| OpError::Io(format!("flush stdout: {e}")))
}

async fn connect_worker(endpoint: String) -> Result<HypervisorWorkerClient<Channel>, OpError> {
    HypervisorWorkerClient::connect(endpoint)
        .await
        .map_err(|e| OpError::Transport(format!("connect worker: {e}")))
}

async fn stream_verify_like_output<W: std::io::Write + ?Sized>(
    out: &mut W,
    client: &mut HypervisorWorkerClient<Channel>,
    op: &'static str,
    base: Vec<u8>,
    log: LogArg,
    bisect_on_divergence: bool,
    json: bool,
) -> Result<(), OpError> {
    let log = match log {
        LogArg::InlinePath(path) => proto::verify_replay_request::Log::InputLog(
            std::fs::read(&path).map_err(|e| OpError::Io(format!("read {path}: {e}")))?,
        ),
        LogArg::StoreId(id) => proto::verify_replay_request::Log::InputLogId(id),
    };
    let mut stream = client
        .verify_replay(proto::VerifyReplayRequest {
            base: Some(proto::SnapshotRef { hash: base }),
            log: Some(log),
            bisect_on_divergence: Some(bisect_on_divergence),
        })
        .await
        .map_err(OpError::Rpc)?
        .into_inner();

    let mut saw_progress = false;
    let mut saw_divergence = false;
    while let Some(progress) = stream.message().await.map_err(OpError::Rpc)? {
        saw_progress = true;
        if matches!(
            progress.msg,
            Some(proto::verify_replay_progress::Msg::Divergence(_))
        ) {
            saw_divergence = true;
        }
        if json {
            writeln!(
                out,
                "{{\"op\":\"{}\",\"status\":\"progress\",\"progress\":{}}}",
                op,
                progress_json(&progress)
            )
        } else {
            writeln!(out, "{}", progress_human(&progress))
        }
        .map_err(|e| OpError::Io(format!("write stdout: {e}")))?;
        out.flush()
            .map_err(|e| OpError::Io(format!("flush stdout: {e}")))?;
    }
    if json {
        let status = if saw_divergence { "divergence" } else { "ok" };
        writeln!(out, "{{\"op\":\"{}\",\"status\":\"{}\"}}", op, status)
            .map_err(|e| OpError::Io(format!("write stdout: {e}")))?;
    } else if !saw_progress {
        writeln!(out, "{op}: no progress")
            .map_err(|e| OpError::Io(format!("write stdout: {e}")))?;
    } else if saw_divergence {
        writeln!(out, "{op}: divergence").map_err(|e| OpError::Io(format!("write stdout: {e}")))?;
    } else {
        writeln!(out, "{op}: ok").map_err(|e| OpError::Io(format!("write stdout: {e}")))?;
    }
    out.flush()
        .map_err(|e| OpError::Io(format!("flush stdout: {e}")))
}
fn snapshot_output(response: proto::TakeSnapshotResponse) -> OpOutput {
    let snapshot = snapshot_ref_json(response.snapshot.as_ref());
    let state_hash = state_hash_json(response.state_hash.as_ref());
    let input_log_id = hex(&response.input_log_id);
    let machine_config_hash = hex(&response.machine_config_hash);
    OpOutput {
        json: format!(
            "{{\"op\":\"snapshot\",\"status\":\"ok\",\"snapshot\":{},\"input_log_id\":\"{}\",\
             \"icount\":{},\"vns\":{},\"state_hash\":{},\"dirty_pages\":{},\
             \"machine_config_hash\":\"{}\",\"frame_counter\":{}}}",
            snapshot,
            input_log_id,
            response.icount,
            response.vns,
            state_hash,
            response.dirty_pages,
            machine_config_hash,
            response.frame_counter
        ),
        human: format!(
            "snapshot={} input_log_id={} icount={} vns={} dirty_pages={} frame_counter={}",
            snapshot_ref_human(response.snapshot.as_ref()),
            input_log_id,
            response.icount,
            response.vns,
            response.dirty_pages,
            response.frame_counter
        ),
    }
}

fn restore_output(response: proto::RestoreSnapshotResponse) -> OpOutput {
    OpOutput {
        json: format!(
            "{{\"op\":\"restore\",\"status\":\"ok\",\"lease\":{},\"config\":{},\
             \"state_hash\":{},\"frame_counter\":{}}}",
            lease_json(response.lease.as_ref()),
            machine_config_json(response.config.as_ref()),
            state_hash_json(response.state_hash.as_ref()),
            response.frame_counter
        ),
        human: format!(
            "lease={} state_hash={} frame_counter={}",
            lease_human(response.lease.as_ref()),
            state_hash_human(response.state_hash.as_ref()),
            response.frame_counter
        ),
    }
}

fn fork_output(response: proto::ForkResponse) -> OpOutput {
    let children_json: Vec<_> = response.children.iter().map(lease_value_json).collect();
    let children_human: Vec<_> = response.children.iter().map(lease_value_human).collect();
    OpOutput {
        json: format!(
            "{{\"op\":\"fork\",\"status\":\"ok\",\"children\":[{}]}}",
            children_json.join(",")
        ),
        human: format!("children={}", children_human.join(",")),
    }
}

fn parse_operator(command: &'static str, args: &[String]) -> Result<ParsedOperator, String> {
    match command {
        "snapshot" => parse_snapshot(args),
        "restore" => parse_restore(args),
        "fork" => parse_fork(args),
        "replay" => parse_replay_verify("replay", false, args),
        "verify" => parse_replay_verify("verify", true, args),
        _ => Err(format!("unknown operator command {command}")),
    }
}

fn parse_snapshot(args: &[String]) -> Result<ParsedOperator, String> {
    let mut config = OpConfig::default();
    let mut lease = None;
    let mut seal_input_log = true;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--endpoint" => {
                config.endpoint = normalize_endpoint(value(args, &mut i, "--endpoint")?)
            }
            "--json" => config.json = true,
            "--lease" => lease = Some(parse_lease(&value(args, &mut i, "--lease")?)?),
            "--no-seal-input-log" => seal_input_log = false,
            other => return Err(format!("unexpected argument {other}")),
        }
        i += 1;
    }
    Ok(ParsedOperator {
        config,
        command: OperatorCommand::Snapshot {
            lease: lease.ok_or("--lease is required")?,
            seal_input_log,
        },
    })
}

fn parse_restore(args: &[String]) -> Result<ParsedOperator, String> {
    let mut config = OpConfig::default();
    let mut snapshot = None;
    let mut entropy_seed = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--endpoint" => {
                config.endpoint = normalize_endpoint(value(args, &mut i, "--endpoint")?)
            }
            "--json" => config.json = true,
            "--snapshot" => {
                snapshot = Some(parse_hex_exact(
                    &value(args, &mut i, "--snapshot")?,
                    32,
                    "snapshot",
                )?)
            }
            "--entropy-seed" => {
                entropy_seed =
                    parse_hex_exact(&value(args, &mut i, "--entropy-seed")?, 32, "entropy_seed")?
            }
            other => return Err(format!("unexpected argument {other}")),
        }
        i += 1;
    }
    Ok(ParsedOperator {
        config,
        command: OperatorCommand::Restore {
            snapshot: snapshot.ok_or("--snapshot is required")?,
            entropy_seed,
        },
    })
}

fn parse_fork(args: &[String]) -> Result<ParsedOperator, String> {
    let mut config = OpConfig::default();
    let mut parent = None;
    let mut count = None;
    let mut entropy_seeds = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--endpoint" => {
                config.endpoint = normalize_endpoint(value(args, &mut i, "--endpoint")?)
            }
            "--json" => config.json = true,
            "--parent" => parent = Some(parse_lease(&value(args, &mut i, "--parent")?)?),
            "--count" => {
                count = Some(
                    value(args, &mut i, "--count")?
                        .parse::<u32>()
                        .map_err(|e| format!("--count must be u32: {e}"))?,
                )
            }
            "--entropy-seed" => entropy_seeds.push(parse_hex_exact(
                &value(args, &mut i, "--entropy-seed")?,
                32,
                "entropy_seed",
            )?),
            other => return Err(format!("unexpected argument {other}")),
        }
        i += 1;
    }
    let count = count.ok_or("--count is required")?;
    if count == 0 {
        return Err("--count must be nonzero".into());
    }
    if !entropy_seeds.is_empty() && entropy_seeds.len() != count as usize {
        return Err(format!(
            "--entropy-seed must be absent or repeated exactly --count times (got {})",
            entropy_seeds.len()
        ));
    }
    Ok(ParsedOperator {
        config,
        command: OperatorCommand::Fork {
            parent: parent.ok_or("--parent is required")?,
            count,
            entropy_seeds,
        },
    })
}

fn parse_replay_verify(
    command: &'static str,
    default_bisect: bool,
    args: &[String],
) -> Result<ParsedOperator, String> {
    let mut config = OpConfig::default();
    let mut base = None;
    let mut log = None;
    let mut bisect_on_divergence = default_bisect;
    let mut bisect_flag = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--endpoint" => {
                config.endpoint = normalize_endpoint(value(args, &mut i, "--endpoint")?)
            }
            "--json" => config.json = true,
            "--snapshot" => {
                base = Some(parse_hex_exact(
                    &value(args, &mut i, "--snapshot")?,
                    32,
                    "snapshot",
                )?)
            }
            "--input-log" => {
                if log.is_some() {
                    return Err("use only one of --input-log or --input-log-id".into());
                }
                log = Some(LogArg::InlinePath(value(args, &mut i, "--input-log")?));
            }
            "--input-log-id" => {
                if log.is_some() {
                    return Err("use only one of --input-log or --input-log-id".into());
                }
                log = Some(LogArg::StoreId(parse_hex_exact(
                    &value(args, &mut i, "--input-log-id")?,
                    32,
                    "input_log_id",
                )?));
            }
            "--bisect" if command == "verify" => {
                if bisect_flag.replace("--bisect").is_some() {
                    return Err("use only one of --bisect or --no-bisect".into());
                }
                bisect_on_divergence = true;
            }
            "--no-bisect" if command == "verify" => {
                if bisect_flag.replace("--no-bisect").is_some() {
                    return Err("use only one of --bisect or --no-bisect".into());
                }
                bisect_on_divergence = false;
            }
            other => return Err(format!("unexpected argument {other}")),
        }
        i += 1;
    }
    let base = base.ok_or("--snapshot is required")?;
    let log = log.ok_or("one of --input-log or --input-log-id is required")?;
    let command = match command {
        "replay" => OperatorCommand::Replay { base, log },
        "verify" => OperatorCommand::Verify {
            base,
            log,
            bisect_on_divergence,
        },
        _ => unreachable!("validated command"),
    };
    Ok(ParsedOperator { config, command })
}

fn value(args: &[String], i: &mut usize, flag: &str) -> Result<String, String> {
    *i += 1;
    let value = args
        .get(*i)
        .ok_or_else(|| format!("{flag} requires a value"))?;
    if value.starts_with("--") {
        return Err(format!("{flag} requires a value, got flag {value}"));
    }
    Ok(value.clone())
}

fn normalize_endpoint(value: String) -> String {
    if value.contains("://") {
        value
    } else {
        format!("http://{value}")
    }
}

fn parse_lease(value: &str) -> Result<proto::Lease, String> {
    let (slot, token) = value
        .split_once(':')
        .ok_or("lease must be SLOT:TOKEN_HEX")?;
    let slot_id = slot
        .parse::<u64>()
        .map_err(|e| format!("lease slot id must be u64: {e}"))?;
    Ok(proto::Lease {
        slot_id,
        token: parse_hex_exact(token, 16, "lease token")?,
    })
}

fn parse_hex_exact(value: &str, bytes: usize, field: &str) -> Result<Vec<u8>, String> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    if value.len() != bytes * 2 {
        return Err(format!("{field} must be {bytes} bytes as hex"));
    }
    let mut out = Vec::with_capacity(bytes);
    let raw = value.as_bytes();
    for i in (0..raw.len()).step_by(2) {
        let hi = hex_val(raw[i]).ok_or_else(|| format!("{field} contains non-hex digits"))?;
        let lo = hex_val(raw[i + 1]).ok_or_else(|| format!("{field} contains non-hex digits"))?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(10 + b - b'a'),
        b'A'..=b'F' => Some(10 + b - b'A'),
        _ => None,
    }
}

fn operator_usage(command: &str) -> String {
    match command {
        "snapshot" => {
            "usage: dh-cli snapshot --lease SLOT:TOKEN_HEX [--endpoint URL] [--no-seal-input-log] [--json]".into()
        }
        "restore" => {
            "usage: dh-cli restore --snapshot SNAPSHOT_HEX [--endpoint URL] [--entropy-seed HEX] [--json]".into()
        }
        "fork" => {
            "usage: dh-cli fork --parent SLOT:TOKEN_HEX --count N [--endpoint URL] [--entropy-seed HEX]... [--json]".into()
        }
        "replay" => {
            "usage: dh-cli replay --snapshot SNAPSHOT_HEX (--input-log PATH | --input-log-id HEX) [--endpoint URL] [--json]".into()
        }
        "verify" => {
            "usage: dh-cli verify --snapshot SNAPSHOT_HEX (--input-log PATH | --input-log-id HEX) [--endpoint URL] [--bisect|--no-bisect] [--json]".into()
        }
        _ => "usage: dh-cli <operator-command> ...".into(),
    }
}

fn progress_json(progress: &proto::VerifyReplayProgress) -> String {
    use proto::verify_replay_progress::Msg;
    match progress.msg.as_ref() {
        Some(Msg::EpochOk(epoch)) => format!(
            "{{\"type\":\"epoch_ok\",\"epoch_index\":{},\"icount\":{}}}",
            epoch.epoch_index, epoch.icount
        ),
        Some(Msg::Done(done)) => format!(
            "{{\"type\":\"done\",\"total_icount\":{},\"end_state_hash\":{}}}",
            done.total_icount,
            state_hash_json(done.end_state_hash.as_ref())
        ),
        Some(Msg::Divergence(div)) => format!(
            "{{\"type\":\"divergence\",\"first_bad_epoch\":{},\"icount_lo\":{},\
             \"icount_hi\":{},\"rip_expected\":{},\"rip_actual\":{},\"reg_diff\":\"{}\",\
             \"diff_page_idx\":[{}],\"suspected_cause\":\"{}\"}}",
            div.first_bad_epoch,
            div.icount_lo,
            div.icount_hi,
            div.rip_expected,
            div.rip_actual,
            hex(&div.reg_diff),
            div.diff_page_idx
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(","),
            json_escape(div.suspected_cause.as_bytes())
        ),
        None => "{\"type\":\"missing\"}".into(),
    }
}

fn progress_human(progress: &proto::VerifyReplayProgress) -> String {
    use proto::verify_replay_progress::Msg;
    match progress.msg.as_ref() {
        Some(Msg::EpochOk(epoch)) => {
            format!(
                "epoch_ok epoch={} icount={}",
                epoch.epoch_index, epoch.icount
            )
        }
        Some(Msg::Done(done)) => format!(
            "done total_icount={} end_state_hash={}",
            done.total_icount,
            state_hash_human(done.end_state_hash.as_ref())
        ),
        Some(Msg::Divergence(div)) => {
            let diff_page_idx = div
                .diff_page_idx
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "divergence first_bad_epoch={} icount_range={}..{} rip_expected={} \
                 rip_actual={} reg_diff={} diff_page_idx=[{}] suspected_cause={}",
                div.first_bad_epoch,
                div.icount_lo,
                div.icount_hi,
                div.rip_expected,
                div.rip_actual,
                hex(&div.reg_diff),
                diff_page_idx,
                div.suspected_cause
            )
        }
        None => "missing_progress".into(),
    }
}

fn error_json(err: &OpError) -> String {
    match err {
        OpError::Io(message) => format!(
            "{{\"kind\":\"io\",\"message\":\"{}\"}}",
            json_escape(message.as_bytes())
        ),
        OpError::Transport(message) => format!(
            "{{\"kind\":\"transport\",\"message\":\"{}\"}}",
            json_escape(message.as_bytes())
        ),
        OpError::Rpc(status) => {
            let details = status.details();
            let details_field = if details.is_empty() {
                String::new()
            } else {
                format!(",\"details\":\"{}\"", hex(details))
            };
            format!(
                "{{\"kind\":\"grpc\",\"code\":\"{}\",\"message\":\"{}\"{}}}",
                code_name(status),
                json_escape(status.message().as_bytes()),
                details_field
            )
        }
    }
}

fn code_name(status: &Status) -> &'static str {
    match status.code() {
        tonic::Code::Ok => "OK",
        tonic::Code::Cancelled => "CANCELLED",
        tonic::Code::Unknown => "UNKNOWN",
        tonic::Code::InvalidArgument => "INVALID_ARGUMENT",
        tonic::Code::DeadlineExceeded => "DEADLINE_EXCEEDED",
        tonic::Code::NotFound => "NOT_FOUND",
        tonic::Code::AlreadyExists => "ALREADY_EXISTS",
        tonic::Code::PermissionDenied => "PERMISSION_DENIED",
        tonic::Code::ResourceExhausted => "RESOURCE_EXHAUSTED",
        tonic::Code::FailedPrecondition => "FAILED_PRECONDITION",
        tonic::Code::Aborted => "ABORTED",
        tonic::Code::OutOfRange => "OUT_OF_RANGE",
        tonic::Code::Unimplemented => "UNIMPLEMENTED",
        tonic::Code::Internal => "INTERNAL",
        tonic::Code::Unavailable => "UNAVAILABLE",
        tonic::Code::DataLoss => "DATA_LOSS",
        tonic::Code::Unauthenticated => "UNAUTHENTICATED",
    }
}

fn snapshot_ref_json(snapshot: Option<&proto::SnapshotRef>) -> String {
    snapshot
        .map(|snapshot| format!("\"{}\"", hex(&snapshot.hash)))
        .unwrap_or_else(|| "null".into())
}

fn snapshot_ref_human(snapshot: Option<&proto::SnapshotRef>) -> String {
    snapshot
        .map(|snapshot| hex(&snapshot.hash))
        .unwrap_or_else(|| "null".into())
}

fn state_hash_json(hash: Option<&proto::StateHash>) -> String {
    hash.map(|hash| format!("\"{}\"", hex(&hash.hash)))
        .unwrap_or_else(|| "null".into())
}

fn state_hash_human(hash: Option<&proto::StateHash>) -> String {
    hash.map(|hash| hex(&hash.hash))
        .unwrap_or_else(|| "null".into())
}

fn lease_json(lease: Option<&proto::Lease>) -> String {
    lease.map(lease_value_json).unwrap_or_else(|| "null".into())
}

fn lease_value_json(lease: &proto::Lease) -> String {
    format!(
        "{{\"slot_id\":{},\"token\":\"{}\"}}",
        lease.slot_id,
        hex(&lease.token)
    )
}

fn lease_human(lease: Option<&proto::Lease>) -> String {
    lease
        .map(lease_value_human)
        .unwrap_or_else(|| "null".into())
}

fn lease_value_human(lease: &proto::Lease) -> String {
    format!("{}:{}", lease.slot_id, hex(&lease.token))
}

fn machine_config_json(config: Option<&proto::MachineConfig>) -> String {
    let Some(config) = config else {
        return "null".into();
    };
    format!(
        "{{\"version\":{},\"mem_bytes\":{},\"vcpus\":{},\"clock_num\":{},\
         \"clock_den\":{},\"base_image_hash\":\"{}\",\"epoch_len\":{},\
         \"hash_epochs\":{},\"skid_margin\":{},\"device_set\":[{}]}}",
        config.version,
        config.mem_bytes,
        config.vcpus,
        config.clock_num,
        config.clock_den,
        hex(&config.base_image_hash),
        config.epoch_len,
        config.hash_epochs,
        config.skid_margin,
        config
            .device_set
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Valid-JSON string escaping (RFC 8259): printable ASCII passes through,
/// everything else becomes \u00XX.
fn json_escape(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len());
    for &b in bytes {
        match b {
            b'"' => s.push_str("\\\""),
            b'\\' => s.push_str("\\\\"),
            0x20..=0x7E => s.push(char::from(b)),
            _ => s.push_str(&format!("\\u{b:04x}")),
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use proto::hypervisor_worker_server::{HypervisorWorker, HypervisorWorkerServer};
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};
    use tokio_stream::wrappers::TcpListenerStream;
    use tonic::transport::Server;
    use tonic::{Request, Response};

    type TestStream<T> = Pin<Box<dyn tokio_stream::Stream<Item = Result<T, Status>> + Send>>;

    #[derive(Clone, Debug)]
    enum SeenCall {
        TakeSnapshot(proto::TakeSnapshotRequest),
        RestoreSnapshot(proto::RestoreSnapshotRequest),
        Fork(proto::ForkRequest),
        VerifyReplay(proto::VerifyReplayRequest),
    }

    #[derive(Clone, Copy, Debug, Default)]
    enum VerifyMode {
        #[default]
        Ok,
        ErrorAfterFirst,
        BisectionDivergence,
    }

    #[derive(Clone, Default)]
    struct FakeWorker {
        calls: Arc<Mutex<Vec<SeenCall>>>,
        verify_mode: Arc<Mutex<VerifyMode>>,
    }

    impl FakeWorker {
        fn set_verify_mode(&self, mode: VerifyMode) {
            *self.verify_mode.lock().unwrap() = mode;
        }

        fn calls(&self) -> Vec<SeenCall> {
            self.calls.lock().unwrap().clone()
        }

        fn record(&self, call: SeenCall) {
            self.calls.lock().unwrap().push(call);
        }
    }

    #[tonic::async_trait]
    impl HypervisorWorker for FakeWorker {
        type StreamGuestEventsStream = TestStream<proto::GuestEvent>;
        type VerifyReplayStream = TestStream<proto::VerifyReplayProgress>;
        type RunWithFrameCaptureStream = TestStream<proto::FrameCaptureEvent>;
        type WatchSlotsStream = TestStream<proto::SlotEvent>;

        async fn create_vm(
            &self,
            _request: Request<proto::CreateVmRequest>,
        ) -> Result<Response<proto::CreateVmResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn restore_snapshot(
            &self,
            request: Request<proto::RestoreSnapshotRequest>,
        ) -> Result<Response<proto::RestoreSnapshotResponse>, Status> {
            self.record(SeenCall::RestoreSnapshot(request.into_inner()));
            Ok(Response::new(proto::RestoreSnapshotResponse {
                lease: Some(proto::Lease {
                    slot_id: 9,
                    token: vec![0x90; 16],
                }),
                config: None,
                state_hash: Some(proto::StateHash {
                    hash: vec![0x44; 32],
                }),
                frame_counter: 7,
            }))
        }

        async fn fork(
            &self,
            request: Request<proto::ForkRequest>,
        ) -> Result<Response<proto::ForkResponse>, Status> {
            self.record(SeenCall::Fork(request.into_inner()));
            Ok(Response::new(proto::ForkResponse {
                children: vec![proto::Lease {
                    slot_id: 2,
                    token: vec![0x22; 16],
                }],
            }))
        }

        async fn destroy_vm(
            &self,
            _request: Request<proto::DestroyVmRequest>,
        ) -> Result<Response<proto::DestroyVmResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn inject_inputs(
            &self,
            _request: Request<proto::InjectInputsRequest>,
        ) -> Result<Response<proto::InjectInputsResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn run(
            &self,
            _request: Request<proto::RunRequest>,
        ) -> Result<Response<proto::RunResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn pause(
            &self,
            _request: Request<proto::PauseRequest>,
        ) -> Result<Response<proto::PauseResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn take_snapshot(
            &self,
            request: Request<proto::TakeSnapshotRequest>,
        ) -> Result<Response<proto::TakeSnapshotResponse>, Status> {
            self.record(SeenCall::TakeSnapshot(request.into_inner()));
            Ok(Response::new(proto::TakeSnapshotResponse {
                snapshot: Some(proto::SnapshotRef {
                    hash: vec![0x11; 32],
                }),
                input_log_id: vec![0x12; 32],
                icount: 10,
                vns: 20,
                state_hash: Some(proto::StateHash {
                    hash: vec![0x13; 32],
                }),
                dirty_pages: 3,
                machine_config_hash: vec![0x14; 32],
                determinism_class: None,
                feature_bytes: Vec::new(),
                fb_lz4: Vec::new(),
                fb_info: None,
                frame_counter: 4,
            }))
        }

        async fn quiesce(
            &self,
            _request: Request<proto::QuiesceRequest>,
        ) -> Result<Response<proto::QuiesceResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn read_guest_memory(
            &self,
            _request: Request<proto::ReadGuestMemoryRequest>,
        ) -> Result<Response<proto::ReadGuestMemoryResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn get_framebuffer(
            &self,
            _request: Request<proto::GetFramebufferRequest>,
        ) -> Result<Response<proto::GetFramebufferResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn stream_guest_events(
            &self,
            _request: Request<proto::StreamGuestEventsRequest>,
        ) -> Result<Response<Self::StreamGuestEventsStream>, Status> {
            Ok(Response::new(Box::pin(tokio_stream::empty())))
        }

        async fn verify_replay(
            &self,
            request: Request<proto::VerifyReplayRequest>,
        ) -> Result<Response<Self::VerifyReplayStream>, Status> {
            self.record(SeenCall::VerifyReplay(request.into_inner()));
            let epoch = proto::VerifyReplayProgress {
                msg: Some(proto::verify_replay_progress::Msg::EpochOk(
                    proto::EpochOk {
                        epoch_index: 1,
                        icount: 50,
                    },
                )),
            };
            let done = proto::VerifyReplayProgress {
                msg: Some(proto::verify_replay_progress::Msg::Done(
                    proto::VerifyDone {
                        total_icount: 100,
                        end_state_hash: Some(proto::StateHash {
                            hash: vec![0x55; 32],
                        }),
                    },
                )),
            };
            let divergence = proto::VerifyReplayProgress {
                msg: Some(proto::verify_replay_progress::Msg::Divergence(
                    proto::Divergence {
                        first_bad_epoch: 7,
                        icount_lo: 60_000,
                        icount_hi: 80_000,
                        rip_expected: 0xffff_8000_0000_1000,
                        rip_actual: 0xffff_8000_0000_1004,
                        reg_diff: vec![0xa1, 0xb2],
                        diff_page_idx: vec![1536, 1537],
                        suspected_cause:
                            "replay-vs-recorded:EPOCH_HASH chain value; evidence_mode=replay-vs-recorded; expected_checkpoint_ref=eeee; actual_probe_ref=aaaa"
                                .into(),
                    },
                )),
            };
            let mode = *self.verify_mode.lock().unwrap();
            let items = match mode {
                VerifyMode::Ok => vec![Ok(epoch), Ok(done)],
                VerifyMode::ErrorAfterFirst => vec![Ok(epoch), Err(Status::data_loss("boom"))],
                VerifyMode::BisectionDivergence => vec![Ok(divergence)],
            };
            Ok(Response::new(Box::pin(tokio_stream::iter(items))))
        }

        async fn run_with_frame_capture(
            &self,
            _request: Request<proto::RunWithFrameCaptureRequest>,
        ) -> Result<Response<Self::RunWithFrameCaptureStream>, Status> {
            Ok(Response::new(Box::pin(tokio_stream::empty())))
        }

        async fn get_worker_info(
            &self,
            _request: Request<proto::GetWorkerInfoRequest>,
        ) -> Result<Response<proto::GetWorkerInfoResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn list_slots(
            &self,
            _request: Request<proto::ListSlotsRequest>,
        ) -> Result<Response<proto::ListSlotsResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn watch_slots(
            &self,
            _request: Request<proto::WatchSlotsRequest>,
        ) -> Result<Response<Self::WatchSlotsStream>, Status> {
            Ok(Response::new(Box::pin(tokio_stream::empty())))
        }
    }

    fn s(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| (*arg).into()).collect()
    }

    async fn run_fake(
        command: &'static str,
        args: &[&str],
        worker: FakeWorker,
    ) -> (Result<(), OpError>, String, FakeWorker) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let worker_for_server = worker.clone();
        let handle = tokio::spawn(async move {
            Server::builder()
                .add_service(HypervisorWorkerServer::new(worker_for_server))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .unwrap();
        });

        let mut owned = s(args);
        owned.push("--endpoint".into());
        owned.push(format!("http://{addr}"));
        let parsed = parse_operator(command, &owned).unwrap();
        let mut out = Vec::new();
        let result = execute_to_writer(parsed, &mut out).await;
        handle.abort();
        (result, String::from_utf8(out).unwrap(), worker)
    }

    #[test]
    fn parses_snapshot_defaults_to_tcp_and_seals_log() {
        let parsed = parse_operator(
            "snapshot",
            &s(&["--lease", "7:00112233445566778899aabbccddeeff", "--json"]),
        )
        .unwrap();
        assert_eq!(parsed.config.endpoint, DEFAULT_ENDPOINT);
        assert!(parsed.config.json);
        assert_eq!(
            parsed.command,
            OperatorCommand::Snapshot {
                lease: proto::Lease {
                    slot_id: 7,
                    token: vec![
                        0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb,
                        0xcc, 0xdd, 0xee, 0xff,
                    ],
                },
                seal_input_log: true,
            }
        );
    }

    #[test]
    fn parses_fork_seed_count_and_endpoint() {
        let parsed = parse_operator(
            "fork",
            &s(&[
                "--endpoint",
                "127.0.0.1:7500",
                "--parent",
                "1:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "--count",
                "2",
                "--entropy-seed",
                "1111111111111111111111111111111111111111111111111111111111111111",
                "--entropy-seed",
                "2222222222222222222222222222222222222222222222222222222222222222",
            ]),
        )
        .unwrap();
        assert_eq!(parsed.config.endpoint, "http://127.0.0.1:7500");
        match parsed.command {
            OperatorCommand::Fork {
                parent,
                count,
                entropy_seeds,
            } => {
                assert_eq!(parent.slot_id, 1);
                assert_eq!(count, 2);
                assert_eq!(entropy_seeds.len(), 2);
                assert_eq!(entropy_seeds[0], vec![0x11; 32]);
                assert_eq!(entropy_seeds[1], vec![0x22; 32]);
            }
            other => panic!("expected fork command, got {other:?}"),
        }
    }

    #[test]
    fn rejects_mismatched_fork_seed_count() {
        let err = parse_operator(
            "fork",
            &s(&[
                "--parent",
                "1:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "--count",
                "2",
                "--entropy-seed",
                "1111111111111111111111111111111111111111111111111111111111111111",
            ]),
        )
        .unwrap_err();
        assert!(err.contains("exactly --count times"));
    }

    #[test]
    fn parses_verify_log_id_and_bisect_flag() {
        let parsed = parse_operator(
            "verify",
            &s(&[
                "--snapshot",
                "abababababababababababababababababababababababababababababababab",
                "--input-log-id",
                "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd",
                "--no-bisect",
            ]),
        )
        .unwrap();
        match parsed.command {
            OperatorCommand::Verify {
                base,
                log,
                bisect_on_divergence,
            } => {
                assert_eq!(base, vec![0xab; 32]);
                assert!(!bisect_on_divergence);
                assert_eq!(log, LogArg::StoreId(vec![0xcd; 32]));
            }
            other => panic!("expected verify command, got {other:?}"),
        }
    }

    #[test]
    fn replay_rejects_verify_only_bisect_flags() {
        let err = parse_operator(
            "replay",
            &s(&[
                "--snapshot",
                "abababababababababababababababababababababababababababababababab",
                "--input-log-id",
                "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd",
                "--bisect",
            ]),
        )
        .unwrap_err();
        assert!(err.contains("unexpected argument --bisect"));
    }

    #[test]
    fn value_flags_reject_missing_flag_like_values() {
        let err = parse_operator("snapshot", &s(&["--lease", "--json"])).unwrap_err();
        assert!(err.contains("--lease requires a value"));
    }

    #[test]
    fn verify_rejects_conflicting_bisect_flags() {
        let err = parse_operator(
            "verify",
            &s(&[
                "--snapshot",
                "abababababababababababababababababababababababababababababababab",
                "--input-log-id",
                "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd",
                "--bisect",
                "--no-bisect",
            ]),
        )
        .unwrap_err();
        assert!(err.contains("use only one"));
    }

    #[tokio::test]
    async fn snapshot_rpc_sends_seal_input_log_default_and_override() {
        let (result, out, worker) = run_fake(
            "snapshot",
            &["--lease", "7:00112233445566778899aabbccddeeff"],
            FakeWorker::default(),
        )
        .await;
        result.unwrap();
        assert!(out.contains("snapshot="));
        match &worker.calls()[0] {
            SeenCall::TakeSnapshot(req) => {
                assert_eq!(req.lease.as_ref().unwrap().slot_id, 7);
                assert_eq!(req.seal_input_log, Some(true));
            }
            other => panic!("expected TakeSnapshot, got {other:?}"),
        }

        let (result, _out, worker) = run_fake(
            "snapshot",
            &[
                "--lease",
                "8:00112233445566778899aabbccddeeff",
                "--no-seal-input-log",
            ],
            FakeWorker::default(),
        )
        .await;
        result.unwrap();
        match &worker.calls()[0] {
            SeenCall::TakeSnapshot(req) => {
                assert_eq!(req.lease.as_ref().unwrap().slot_id, 8);
                assert_eq!(req.seal_input_log, Some(false));
            }
            other => panic!("expected TakeSnapshot, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn restore_and_fork_rpc_fields_are_pinned() {
        let (result, out, worker) = run_fake(
            "restore",
            &[
                "--snapshot",
                "abababababababababababababababababababababababababababababababab",
                "--entropy-seed",
                "1212121212121212121212121212121212121212121212121212121212121212",
            ],
            FakeWorker::default(),
        )
        .await;
        result.unwrap();
        assert!(out.contains("lease=9:"));
        match &worker.calls()[0] {
            SeenCall::RestoreSnapshot(req) => {
                assert_eq!(req.snapshot.as_ref().unwrap().hash, vec![0xab; 32]);
                assert_eq!(req.entropy_seed, vec![0x12; 32]);
            }
            other => panic!("expected RestoreSnapshot, got {other:?}"),
        }

        let (result, out, worker) = run_fake(
            "fork",
            &[
                "--parent",
                "1:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "--count",
                "2",
                "--entropy-seed",
                "1111111111111111111111111111111111111111111111111111111111111111",
                "--entropy-seed",
                "2222222222222222222222222222222222222222222222222222222222222222",
            ],
            FakeWorker::default(),
        )
        .await;
        result.unwrap();
        assert!(out.contains("children=2:"));
        match &worker.calls()[0] {
            SeenCall::Fork(req) => {
                assert_eq!(req.parent.as_ref().unwrap().slot_id, 1);
                assert_eq!(req.count, 2);
                assert_eq!(req.entropy_seeds, vec![vec![0x11; 32], vec![0x22; 32]]);
            }
            other => panic!("expected Fork, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn replay_and_verify_rpc_fields_and_streaming_output_are_pinned() {
        let (result, out, worker) = run_fake(
            "replay",
            &[
                "--snapshot",
                "abababababababababababababababababababababababababababababababab",
                "--input-log-id",
                "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd",
                "--json",
            ],
            FakeWorker::default(),
        )
        .await;
        result.unwrap();
        assert!(out.contains("\"status\":\"progress\""));
        assert!(out.contains("\"status\":\"ok\""));
        match &worker.calls()[0] {
            SeenCall::VerifyReplay(req) => {
                assert_eq!(req.base.as_ref().unwrap().hash, vec![0xab; 32]);
                assert_eq!(req.bisect_on_divergence, Some(false));
                assert_eq!(
                    req.log,
                    Some(proto::verify_replay_request::Log::InputLogId(vec![
                        0xcd;
                        32
                    ]))
                );
            }
            other => panic!("expected VerifyReplay, got {other:?}"),
        }

        let (result, _out, worker) = run_fake(
            "verify",
            &[
                "--snapshot",
                "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
                "--input-log-id",
                "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            ],
            FakeWorker::default(),
        )
        .await;
        result.unwrap();
        match &worker.calls()[0] {
            SeenCall::VerifyReplay(req) => {
                assert_eq!(req.base.as_ref().unwrap().hash, vec![0xee; 32]);
                assert_eq!(req.bisect_on_divergence, Some(true));
                assert_eq!(
                    req.log,
                    Some(proto::verify_replay_request::Log::InputLogId(vec![
                        0xff;
                        32
                    ]))
                );
            }
            other => panic!("expected VerifyReplay, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn verify_stream_preserves_progress_before_late_error() {
        let worker = FakeWorker::default();
        worker.set_verify_mode(VerifyMode::ErrorAfterFirst);
        let (result, out, worker) = run_fake(
            "verify",
            &[
                "--snapshot",
                "abababababababababababababababababababababababababababababababab",
                "--input-log-id",
                "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd",
                "--json",
            ],
            worker,
        )
        .await;
        match result {
            Err(OpError::Rpc(status)) => {
                assert_eq!(status.code(), tonic::Code::DataLoss);
                assert_eq!(status.message(), "boom");
            }
            other => panic!("expected stream DataLoss, got {other:?}"),
        }
        assert!(
            out.contains("\"status\":\"progress\""),
            "progress must be written before the stream error: {out}"
        );
        assert!(matches!(worker.calls()[0], SeenCall::VerifyReplay(_)));
    }

    #[tokio::test]
    async fn verify_renders_bisection_divergence_json_and_human() {
        let worker = FakeWorker::default();
        worker.set_verify_mode(VerifyMode::BisectionDivergence);
        let (result, out, worker) = run_fake(
            "verify",
            &[
                "--snapshot",
                "abababababababababababababababababababababababababababababababab",
                "--input-log-id",
                "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd",
                "--json",
            ],
            worker,
        )
        .await;
        result.unwrap();
        assert!(out.contains("\"type\":\"divergence\""));
        assert!(out.contains("\"first_bad_epoch\":7"));
        assert!(out.contains("\"icount_lo\":60000"));
        assert!(out.contains("\"icount_hi\":80000"));
        assert!(out.contains("\"rip_expected\":18446603336221200384"));
        assert!(out.contains("\"rip_actual\":18446603336221200388"));
        assert!(out.contains("\"reg_diff\":\"a1b2\""));
        assert!(out.contains("\"diff_page_idx\":[1536,1537]"));
        assert!(out.contains("evidence_mode=replay-vs-recorded"));
        assert!(out.contains("\"status\":\"divergence\""));
        assert!(matches!(worker.calls()[0], SeenCall::VerifyReplay(_)));

        let worker = FakeWorker::default();
        worker.set_verify_mode(VerifyMode::BisectionDivergence);
        let (result, out, _worker) = run_fake(
            "verify",
            &[
                "--snapshot",
                "abababababababababababababababababababababababababababababababab",
                "--input-log-id",
                "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd",
            ],
            worker,
        )
        .await;
        result.unwrap();
        assert!(out.contains("divergence first_bad_epoch=7 icount_range=60000..80000"));
        assert!(out.contains("rip_expected=18446603336221200384"));
        assert!(out.contains("rip_actual=18446603336221200388"));
        assert!(out.contains("reg_diff=a1b2"));
        assert!(out.contains("diff_page_idx=[1536,1537]"));
        assert!(out.contains("expected_checkpoint_ref=eeee"));
        assert!(out.contains("verify: divergence"));
    }

    #[test]
    fn renders_verify_progress_json() {
        let progress = proto::VerifyReplayProgress {
            msg: Some(proto::verify_replay_progress::Msg::Done(
                proto::VerifyDone {
                    total_icount: 99,
                    end_state_hash: Some(proto::StateHash {
                        hash: vec![0x5a; 32],
                    }),
                },
            )),
        };
        assert_eq!(
            progress_json(&progress),
            format!(
                "{{\"type\":\"done\",\"total_icount\":99,\"end_state_hash\":\"{}\"}}",
                "5a".repeat(32)
            )
        );
    }

    #[test]
    fn renders_grpc_error_as_json() {
        let err = OpError::Rpc(Status::unimplemented("not wired"));
        assert_eq!(
            error_json(&err),
            "{\"kind\":\"grpc\",\"code\":\"UNIMPLEMENTED\",\"message\":\"not wired\"}"
        );
    }
}
