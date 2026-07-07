//! dh-workerd — the per-host daemon (ARCH §7.5).

#[cfg(target_arch = "x86_64")]
use dh_worker::image_resolver::DEFAULT_IMAGE_CACHE_DIR;
#[cfg(target_arch = "x86_64")]
use dh_worker::service::DEFAULT_SNAPSTORE_TCP;
use dh_worker::service::{
    PreflightHealth, WorkerConfig, DEFAULT_HTTP_ADDR, DEFAULT_TCP_ADDR, DEFAULT_UDS_PATH,
};
use std::path::PathBuf;

#[derive(Debug)]
enum Command {
    Preflight,
    Serve {
        tcp_addr: std::net::SocketAddr,
        http_addr: std::net::SocketAddr,
        uds_path: Option<PathBuf>,
        skip_preflight: bool,
        #[cfg(target_arch = "x86_64")]
        image_cache_dir: PathBuf,
        #[cfg(target_arch = "x86_64")]
        snapstore: Option<snapstore_client::Transport>,
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
            http_addr,
            uds_path,
            skip_preflight,
            #[cfg(target_arch = "x86_64")]
            image_cache_dir,
            #[cfg(target_arch = "x86_64")]
            snapstore,
        } => {
            let preflight = if skip_preflight {
                PreflightHealth::skipped("dh-workerd started with --skip-preflight")
            } else {
                run_preflight_or_exit()
            };
            #[cfg(target_arch = "x86_64")]
            let mut config = {
                let mut config = WorkerConfig::from_host_defaults()?;
                config.image_cache_dir = image_cache_dir;
                config.snapstore = snapstore;
                config
            };
            #[cfg(not(target_arch = "x86_64"))]
            let mut config = WorkerConfig::from_host_defaults()?;
            config.preflight = preflight;
            // Build profile is load-bearing ops truth (play-60fps M1):
            // long-lived operator workers must run release builds, and the
            // runbooks assert this line before acceptance.
            eprintln!(
                "dh-workerd (build_profile={}) serving gRPC TCP {tcp_addr}, HTTP {http_addr}{}",
                if cfg!(debug_assertions) {
                    "debug"
                } else {
                    "release"
                },
                uds_path
                    .as_ref()
                    .map(|p| format!(" and UDS {}", p.display()))
                    .unwrap_or_else(|| " without UDS".into())
            );
            dh_worker::service::serve(config, tcp_addr, uds_path, http_addr).await?;
            Ok(())
        }
    }
}

fn run_preflight_or_exit() -> PreflightHealth {
    let (results, ok) = dh_worker::preflight::run_preflight();
    for r in &results {
        println!("{r}");
    }
    if !ok {
        eprintln!("preflight FAILED: host is not §7.4/§2.1 compliant");
        std::process::exit(1);
    }
    println!("preflight OK");
    PreflightHealth::passed(&results)
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
    let mut http_addr = DEFAULT_HTTP_ADDR
        .parse()
        .map_err(|e| format!("invalid default HTTP addr {DEFAULT_HTTP_ADDR}: {e}"))?;
    let mut uds_path = Some(PathBuf::from(DEFAULT_UDS_PATH));
    let mut skip_preflight = false;
    #[cfg(target_arch = "x86_64")]
    let mut image_cache_dir = PathBuf::from(DEFAULT_IMAGE_CACHE_DIR);
    #[cfg(target_arch = "x86_64")]
    let mut snapstore = Some(snapstore_client::Transport::Tcp(
        DEFAULT_SNAPSTORE_TCP.into(),
    ));
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
            "--http" => {
                i += 1;
                let value = args.get(i).ok_or("--http requires an address")?;
                http_addr = value
                    .parse()
                    .map_err(|e| format!("invalid --http address {value}: {e}"))?;
            }
            "--uds" => {
                i += 1;
                let value = args.get(i).ok_or("--uds requires a path")?;
                uds_path = Some(PathBuf::from(value));
            }
            "--no-uds" => uds_path = None,
            "--skip-preflight" => skip_preflight = true,
            #[cfg(target_arch = "x86_64")]
            "--image-cache" => {
                i += 1;
                let value = args.get(i).ok_or("--image-cache requires a path")?;
                image_cache_dir = PathBuf::from(value);
            }
            #[cfg(target_arch = "x86_64")]
            "--snapstore-tcp" => {
                i += 1;
                let value = args.get(i).ok_or("--snapstore-tcp requires a URI")?;
                snapstore = Some(snapstore_client::Transport::Tcp(value.to_owned()));
            }
            #[cfg(target_arch = "x86_64")]
            "--snapstore-uds" => {
                i += 1;
                let value = args.get(i).ok_or("--snapstore-uds requires a path")?;
                snapstore = Some(snapstore_client::Transport::Uds(PathBuf::from(value)));
            }
            #[cfg(target_arch = "x86_64")]
            "--no-snapstore" => snapstore = None,
            "--help" | "-h" => return Err(usage()),
            other => return Err(format!("unknown argument {other}\n{}", usage())),
        }
        i += 1;
    }

    Ok(Command::Serve {
        tcp_addr,
        http_addr,
        uds_path,
        skip_preflight,
        #[cfg(target_arch = "x86_64")]
        image_cache_dir,
        #[cfg(target_arch = "x86_64")]
        snapstore,
    })
}

#[cfg(target_arch = "x86_64")]
fn usage() -> String {
    format!(
        "usage:\n  dh-workerd --preflight\n  dh-workerd [serve] [--tcp ADDR] [--http ADDR] [--uds PATH|--no-uds] [--image-cache PATH] [--snapstore-tcp URI|--snapstore-uds PATH|--no-snapstore] [--skip-preflight]\n\ndefaults: --tcp {DEFAULT_TCP_ADDR} --http {DEFAULT_HTTP_ADDR} --uds {DEFAULT_UDS_PATH} --image-cache {DEFAULT_IMAGE_CACHE_DIR} --snapstore-tcp {DEFAULT_SNAPSTORE_TCP}"
    )
}

#[cfg(not(target_arch = "x86_64"))]
fn usage() -> String {
    format!(
        "usage:\n  dh-workerd --preflight\n  dh-workerd [serve] [--tcp ADDR] [--http ADDR] [--uds PATH|--no-uds] [--skip-preflight]\n\ndefaults: --tcp {DEFAULT_TCP_ADDR} --http {DEFAULT_HTTP_ADDR} --uds {DEFAULT_UDS_PATH}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_default_serve_args() {
        let command = parse_args(Vec::<String>::new()).unwrap();
        let Command::Serve {
            tcp_addr,
            http_addr,
            uds_path,
            skip_preflight,
            #[cfg(target_arch = "x86_64")]
            image_cache_dir,
            #[cfg(target_arch = "x86_64")]
            snapstore,
        } = command
        else {
            panic!("expected serve command");
        };
        assert_eq!(tcp_addr, DEFAULT_TCP_ADDR.parse().unwrap());
        assert_eq!(http_addr, DEFAULT_HTTP_ADDR.parse().unwrap());
        assert_eq!(uds_path, Some(PathBuf::from(DEFAULT_UDS_PATH)));
        assert!(!skip_preflight);
        #[cfg(target_arch = "x86_64")]
        {
            assert_eq!(image_cache_dir, PathBuf::from(DEFAULT_IMAGE_CACHE_DIR));
            match snapstore {
                Some(snapstore_client::Transport::Tcp(uri)) => {
                    assert_eq!(uri, DEFAULT_SNAPSTORE_TCP)
                }
                other => panic!("expected default TCP snapstore transport, got {other:?}"),
            }
        }
    }

    #[test]
    fn parse_preflight_short_circuits_serve_defaults() {
        let command = parse_args(["--preflight".to_owned()]).unwrap();
        assert!(matches!(command, Command::Preflight));
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn parse_x86_resource_flags() {
        let command = parse_args([
            "serve".to_owned(),
            "--tcp".to_owned(),
            "127.0.0.1:7500".to_owned(),
            "--http".to_owned(),
            "127.0.0.1:7501".to_owned(),
            "--no-uds".to_owned(),
            "--image-cache".to_owned(),
            "/tmp/dh-images".to_owned(),
            "--snapstore-uds".to_owned(),
            "/tmp/snapstore.sock".to_owned(),
            "--skip-preflight".to_owned(),
        ])
        .unwrap();
        let Command::Serve {
            tcp_addr,
            http_addr,
            uds_path,
            skip_preflight,
            image_cache_dir,
            snapstore,
        } = command
        else {
            panic!("expected serve command");
        };
        assert_eq!(tcp_addr, "127.0.0.1:7500".parse().unwrap());
        assert_eq!(http_addr, "127.0.0.1:7501".parse().unwrap());
        assert_eq!(uds_path, None);
        assert!(skip_preflight);
        assert_eq!(image_cache_dir, PathBuf::from("/tmp/dh-images"));
        match snapstore {
            Some(snapstore_client::Transport::Uds(path)) => {
                assert_eq!(path, PathBuf::from("/tmp/snapstore.sock"))
            }
            other => panic!("expected UDS snapstore transport, got {other:?}"),
        }
    }

    #[test]
    fn parse_missing_value_reports_specific_flag() {
        let err = parse_args(["--tcp".to_owned()]).unwrap_err();
        assert_eq!(err, "--tcp requires an address");
        let err = parse_args(["--http".to_owned()]).unwrap_err();
        assert_eq!(err, "--http requires an address");
    }
}
