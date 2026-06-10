#![forbid(unsafe_code)]

// Local debug CLI (ARCH §1): drives the VMM directly. It must not depend on
// dh-worker — "nothing depends on dh-worker" is a normative dependency rule.

/// Valid-JSON string escaping (RFC 8259): printable ASCII passes through,
/// everything else becomes \u00XX — std's ascii::escape_default emits
/// \xNN, which is NOT legal JSON.
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

fn usage() -> ! {
    eprintln!(
        "usage:\n  dh-cli caps\n  dh-cli cpuid-diff\n  dh-cli boot <guest.elf> [--mem-mib N] [--cmdline S] [--json]\n  dh-cli run <guest.elf> (--icount-budget N | --vns-budget N) [--mem-mib N] [--cmdline S]"
    );
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("caps") | None => println!("{}", dh_vmm::m0_missing_caps_summary()),
        Some("boot") => boot_cmd(&args[1..]),
        Some("run") => run_cmd(&args[1..]),
        Some("cpuid-diff") => match dh_cli::cpuid::cpuid_diff() {
            Ok(report) => print!("{report}"),
            Err(e) => {
                eprintln!("dh-cli cpuid-diff: {e}");
                std::process::exit(1);
            }
        },
        _ => usage(),
    }
}

fn run_cmd(args: &[String]) {
    let mut path = None;
    let mut mem_mib = 16u64;
    let mut cmdline = String::new();
    let mut until = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--mem-mib" => {
                mem_mib = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| usage())
            }
            "--cmdline" => cmdline = it.next().cloned().unwrap_or_else(|| usage()),
            "--icount-budget" => {
                let n = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| usage());
                until = Some(dh_vmm::runctl::Until::IcountBudget(n));
            }
            "--vns-budget" => {
                let n = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| usage());
                until = Some(dh_vmm::runctl::Until::VnsBudget(n));
            }
            p if path.is_none() && !p.starts_with("--") => path = Some(p.to_string()),
            _ => usage(),
        }
    }
    let path = path.unwrap_or_else(|| usage());
    let until = until.unwrap_or_else(|| usage());
    let elf = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("dh-cli run: read {path}: {e}");
            std::process::exit(1);
        }
    };
    match dh_cli::run::run(&elf, mem_mib << 20, cmdline.as_bytes(), until) {
        Ok(r) => println!(
            "{{\"reason\":\"{}\",\"icount\":{},\"rip\":\"{:#x}\",\"vns\":{},\"state_hash\":\"{}\",\"serial\":\"{}\"}}",
            r.reason,
            r.icount,
            r.rip,
            r.vns,
            r.state_hash,
            json_escape(&r.serial)
        ),
        Err(e) => {
            eprintln!("dh-cli run: {e}");
            std::process::exit(1);
        }
    }
}

fn boot_cmd(args: &[String]) {
    let mut path = None;
    let mut mem_mib = 16u64;
    let mut cmdline = String::new();
    let mut json = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--mem-mib" => {
                mem_mib = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| usage())
            }
            "--cmdline" => cmdline = it.next().cloned().unwrap_or_else(|| usage()),
            "--json" => json = true,
            p if path.is_none() && !p.starts_with("--") => path = Some(p.to_string()),
            _ => usage(),
        }
    }
    let path = path.unwrap_or_else(|| usage());
    let elf = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("dh-cli boot: read {path}: {e}");
            std::process::exit(1);
        }
    };

    // M0 exit budget: generous enough for the 100M-instruction landing
    // loop's handful of exits, tiny against a runaway exit storm.
    match dh_cli::boot::boot(&elf, mem_mib << 20, cmdline.as_bytes(), 1_000_000) {
        Ok(out) => {
            if json {
                println!(
                    "{{\"serial\":\"{}\",\"exits\":{}}}",
                    json_escape(&out.serial),
                    out.exits
                );
            } else {
                use std::io::Write;
                std::io::stdout().write_all(&out.serial).unwrap();
            }
        }
        Err(e) => {
            eprintln!("dh-cli boot: {e}");
            std::process::exit(1);
        }
    }
}
