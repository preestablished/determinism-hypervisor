//! dh-workerd — the per-host daemon (ARCH §7.5). Phase 1 ships only the
//! preflight gate; the gRPC service lands with the M3 worker bead.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--preflight") => {
            let (results, ok) = dh_worker::preflight::run_preflight();
            for r in &results {
                println!("{r}");
            }
            if !ok {
                eprintln!("preflight FAILED — host is not §7.4/§2.1 compliant");
                std::process::exit(1);
            }
            println!("preflight OK");
        }
        _ => {
            eprintln!("usage: dh-workerd --preflight");
            eprintln!("(the serving daemon arrives with M3; only the preflight gate exists)");
            std::process::exit(2);
        }
    }
}
