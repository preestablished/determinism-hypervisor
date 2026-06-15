//! dh-workerd — the per-host daemon (ARCH §7.5).

use dh_worker::service::{WorkerConfig, DEFAULT_TCP_ADDR, DEFAULT_UDS_PATH};
use std::path::PathBuf;

enum Command {
    Preflight,
    Serve {
        tcp_addr: std::net::SocketAddr,
        uds_path: Option<PathBuf>,
        skip_preflight: bool,
    },
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("{e}");
        std::process::exit(2);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    match parse_args(std::env::args().skip(1))? {
        Command::Preflight => {
            run_preflight_or_exit();
            Ok(())
        }
        Command::Serve {
            tcp_addr,
            uds_path,
            skip_preflight,
        } => {
            if !skip_preflight {
                run_preflight_or_exit();
            }
            let config = WorkerConfig::from_host_defaults()?;
            eprintln!(
                "dh-workerd serving TCP {tcp_addr}{}",
                uds_path
                    .as_ref()
                    .map(|p| format!(" and UDS {}", p.display()))
                    .unwrap_or_else(|| " without UDS".into())
            );
            dh_worker::service::serve(config, tcp_addr, uds_path).await?;
            Ok(())
        }
    }
}

fn run_preflight_or_exit() {
    let (results, ok) = dh_worker::preflight::run_preflight();
    for r in &results {
        println!("{r}");
    }
    if !ok {
        eprintln!("preflight FAILED: host is not §7.4/§2.1 compliant");
        std::process::exit(1);
    }
    println!("preflight OK");
}

fn parse_args<I>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = String>,
{
    let mut args: Vec<String> = args.into_iter().collect();
    if args.first().map(String::as_str) == Some("--preflight") {
        return Ok(Command::Preflight);
    }
    if args.first().map(String::as_str) == Some("serve") {
        args.remove(0);
    }

    let mut tcp_addr = DEFAULT_TCP_ADDR
        .parse()
        .map_err(|e| format!("invalid default TCP addr {DEFAULT_TCP_ADDR}: {e}"))?;
    let mut uds_path = Some(PathBuf::from(DEFAULT_UDS_PATH));
    let mut skip_preflight = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--tcp" => {
                i += 1;
                let value = args.get(i).ok_or("--tcp requires an address")?;
                tcp_addr = value
                    .parse()
                    .map_err(|e| format!("invalid --tcp address {value}: {e}"))?;
            }
            "--uds" => {
                i += 1;
                let value = args.get(i).ok_or("--uds requires a path")?;
                uds_path = Some(PathBuf::from(value));
            }
            "--no-uds" => uds_path = None,
            "--skip-preflight" => skip_preflight = true,
            "--help" | "-h" => return Err(usage()),
            other => return Err(format!("unknown argument {other}\n{}", usage())),
        }
        i += 1;
    }

    Ok(Command::Serve {
        tcp_addr,
        uds_path,
        skip_preflight,
    })
}

fn usage() -> String {
    format!(
        "usage:\n  dh-workerd --preflight\n  dh-workerd [serve] [--tcp ADDR] [--uds PATH|--no-uds] [--skip-preflight]\n\ndefaults: --tcp {DEFAULT_TCP_ADDR} --uds {DEFAULT_UDS_PATH}"
    )
}
