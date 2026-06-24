//! Operator-only M9 READY snapshot handoff generator.

#[cfg(target_arch = "x86_64")]
fn main() {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    if raw_args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{}", dh_worker::m9_handoff::usage());
        return;
    }

    let mut stdout = std::io::stdout();
    match dh_worker::m9_handoff::run_cli(raw_args.clone(), &mut stdout) {
        Ok(_) => {}
        Err(e) => {
            if let Some(private_root) = dh_worker::m9_handoff::private_root_from_raw_args(&raw_args)
            {
                let _ = e.write_private_log(&private_root);
            }
            eprintln!("dh-m9-ready-handoff: {}", e.public_message());
            std::process::exit(1);
        }
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn main() {
    eprintln!("dh-m9-ready-handoff: unsupported platform; x86_64 Linux with KVM is required");
    std::process::exit(2);
}
