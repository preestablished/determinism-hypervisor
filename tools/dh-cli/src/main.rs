#![forbid(unsafe_code)]

// Local debug CLI (ARCH §1): drives the VMM directly. It must not depend on
// dh-worker — "nothing depends on dh-worker" is a normative dependency rule.

fn usage() -> ! {
    eprintln!(
        "usage:\n  dh-cli caps\n  dh-cli boot <guest.elf> [--mem-mib N] [--cmdline S] [--json]"
    );
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("caps") | None => println!("{}", dh_vmm::m0_missing_caps_summary()),
        Some("boot") => boot_cmd(&args[1..]),
        _ => usage(),
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
                let escaped: String = out
                    .serial
                    .iter()
                    .flat_map(|b| std::ascii::escape_default(*b))
                    .map(char::from)
                    .collect();
                println!("{{\"serial\":\"{escaped}\",\"exits\":{}}}", out.exits);
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
