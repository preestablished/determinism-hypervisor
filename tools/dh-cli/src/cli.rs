// Local debug CLI (ARCH §1): drives the VMM directly. It must not depend on
// dh-worker — "nothing depends on dh-worker" is a normative dependency rule.
// This is the x86_64 implementation; the bin's main dispatches here
// (non-x86_64 builds get a stub — bead v5w).

use std::path::PathBuf;

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

#[derive(Debug, PartialEq, Eq)]
pub struct BootArgs {
    pub mode: BootMode,
    pub json: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BootMode {
    Elf {
        path: String,
        mem_bytes: u64,
        cmdline: String,
    },
    Linux(crate::linux::LinuxGuestPaths),
}

#[derive(Debug, PartialEq, Eq)]
pub struct RunArgs {
    pub mode: RunMode,
    pub paranoid_hash: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RunMode {
    Elf {
        path: String,
        mem_bytes: u64,
        cmdline: String,
        until: dh_vmm::runctl::Until,
    },
    Linux {
        paths: crate::linux::LinuxGuestPaths,
        hard_cap: u64,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub struct GateArgs {
    pub runs: usize,
    pub linux: Option<crate::linux::LinuxGuestPaths>,
}

#[derive(Default)]
struct LinuxArgsBuilder {
    bzimage: Option<PathBuf>,
    initramfs: Option<PathBuf>,
    base_image: Option<PathBuf>,
    game_image: Option<PathBuf>,
    cmdline_extra: Vec<String>,
    seen_linux_arg: bool,
}

impl LinuxArgsBuilder {
    fn seen(&self) -> bool {
        self.seen_linux_arg
    }

    fn path_arg(slot: &mut Option<PathBuf>, name: &str, value: String) -> Result<(), String> {
        if slot.is_some() {
            return Err(format!("{name} supplied more than once"));
        }
        *slot = Some(PathBuf::from(value));
        Ok(())
    }

    fn finish(self, mem_bytes: u64) -> Result<crate::linux::LinuxGuestPaths, String> {
        Ok(crate::linux::LinuxGuestPaths {
            bzimage: self.bzimage.ok_or("missing --bzimage")?,
            initramfs: self.initramfs.ok_or("missing --initramfs")?,
            base_image: self.base_image.ok_or("missing --base-image")?,
            game_image: self.game_image.ok_or("missing --game-image")?,
            cmdline_extra: self.cmdline_extra,
            mem_bytes,
        })
    }
}

fn usage() -> ! {
    eprintln!(
        "usage:\n  dh-cli caps\n  dh-cli cpuid-diff\n  dh-cli boot <guest.elf> [--mem-mib N] [--cmdline S] [--json]\n  dh-cli boot --linux --bzimage PATH --initramfs PATH --base-image PATH --game-image PATH [--mem-mib N] [--cmdline-extra S]... [--json]\n  dh-cli run <guest.elf> (--icount-budget N | --vns-budget N) [--mem-mib N] [--cmdline S] [--paranoid-hash]\n  dh-cli run --linux --bzimage PATH --initramfs PATH --base-image PATH --game-image PATH [--mem-mib N] [--cmdline-extra S]... [--icount-budget HARD_CAP] [--paranoid-hash]\n  dh-cli snapshot --lease SLOT:TOKEN_HEX [--endpoint URL] [--no-seal-input-log] [--json]\n  dh-cli restore --snapshot SNAPSHOT_HEX [--endpoint URL] [--entropy-seed HEX] [--json]\n  dh-cli fork --parent SLOT:TOKEN_HEX --count N [--endpoint URL] [--entropy-seed HEX]... [--json]\n  dh-cli replay --snapshot SNAPSHOT_HEX (--input-log PATH | --input-log-id HEX) [--endpoint URL] [--json]\n  dh-cli verify --snapshot SNAPSHOT_HEX (--input-log PATH | --input-log-id HEX) [--endpoint URL] [--bisect|--no-bisect] [--json]\n  dh-cli skid [--samples N]\n  dh-cli gate [--runs N] [--linux --bzimage PATH --initramfs PATH --base-image PATH --game-image PATH [--mem-mib N] [--cmdline-extra S]...]"
    );
    std::process::exit(2);
}

pub fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("caps") | None => println!("{}", dh_vmm::m0_missing_caps_summary()),
        Some("boot") => boot_cmd(&args[1..]),
        Some("run") => run_cmd(&args[1..]),
        Some("snapshot") => crate::ops::dispatch("snapshot", &args[1..]),
        Some("restore") => crate::ops::dispatch("restore", &args[1..]),
        Some("fork") => crate::ops::dispatch("fork", &args[1..]),
        Some("replay") => crate::ops::dispatch("replay", &args[1..]),
        Some("verify") => crate::ops::dispatch("verify", &args[1..]),
        Some("gate") => gate_cmd(&args[1..]),
        Some("skid") => {
            let samples = args
                .get(1)
                .and_then(|a| (a == "--samples").then(|| args.get(2)).flatten())
                .and_then(|v| v.parse().ok())
                .unwrap_or(200);
            match crate::skid::measure(samples) {
                Ok(r) => {
                    print!("{}", r.histogram.artifact());
                    print!("{}", r.histogram.prometheus("dh_pmi_skid_instructions"));
                    match r.gate {
                        Ok(()) => println!(
                            "GATE OK: max skid {} < skid_margin/2 ({})",
                            r.histogram.max().unwrap_or(0),
                            r.skid_margin / 2
                        ),
                        Err(e) => {
                            eprintln!("{e}");
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("dh-cli skid: {e}");
                    std::process::exit(1);
                }
            }
        }
        Some("cpuid-diff") => match crate::cpuid::cpuid_diff() {
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
    let parsed = parse_run_args(args).unwrap_or_else(|e| {
        eprintln!("dh-cli run: {e}");
        usage()
    });
    match parsed.mode {
        RunMode::Elf {
            path,
            mem_bytes,
            cmdline,
            until,
        } => {
            let elf = match std::fs::read(&path) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("dh-cli run: read {path}: {e}");
                    std::process::exit(1);
                }
            };
            match crate::run::run(
                &elf,
                mem_bytes,
                cmdline.as_bytes(),
                until,
                parsed.paranoid_hash,
            ) {
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
        RunMode::Linux { paths, hard_cap } => {
            match crate::linux::run_to_ready(&paths, hard_cap, parsed.paranoid_hash) {
                Ok(r) => println!("{}", linux_ready_json(&r)),
                Err(e) => {
                    eprintln!("dh-cli run: {e}");
                    std::process::exit(1);
                }
            }
        }
    }
}

fn boot_cmd(args: &[String]) {
    let parsed = parse_boot_args(args).unwrap_or_else(|e| {
        eprintln!("dh-cli boot: {e}");
        usage()
    });
    match parsed.mode {
        BootMode::Elf {
            path,
            mem_bytes,
            cmdline,
        } => {
            let elf = match std::fs::read(&path) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("dh-cli boot: read {path}: {e}");
                    std::process::exit(1);
                }
            };

            // M0 exit budget: generous enough for the 100M-instruction landing
            // loop's handful of exits, tiny against a runaway exit storm.
            match crate::boot::boot(&elf, mem_bytes, cmdline.as_bytes(), 1_000_000) {
                Ok(out) => {
                    if parsed.json {
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
        BootMode::Linux(paths) => {
            match crate::linux::run_to_ready(&paths, crate::linux::DEFAULT_READY_HARD_CAP, false) {
                Ok(r) if parsed.json => println!("{}", linux_ready_json(&r)),
                Ok(r) => println!(
                    "READY EventKind {} icount={} vns={} state_hash={}",
                    r.ready_event_kind, r.icount, r.vns, r.state_hash
                ),
                Err(e) => {
                    eprintln!("dh-cli boot: {e}");
                    std::process::exit(1);
                }
            }
        }
    }
}

fn gate_cmd(args: &[String]) {
    let parsed = parse_gate_args(args).unwrap_or_else(|e| {
        eprintln!("dh-cli gate: {e}");
        usage()
    });
    if let Some(paths) = parsed.linux {
        match crate::gate::run_linux_gate(parsed.runs, &paths) {
            Ok(report) => {
                print!("{}", report.artifact());
                if report.passed() {
                    println!(
                        "M9 LINUX READY GATE: PASS ({} runs, Ready EventKind {})",
                        parsed.runs,
                        crate::linux::READY_EVENT_KIND
                    );
                } else {
                    eprintln!("M9 LINUX READY GATE: FAIL");
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("dh-cli gate: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    match crate::gate::run_gate(parsed.runs) {
        Ok((plain, timer)) => {
            print!("{}", plain.artifact());
            print!("{}", timer.artifact());
            if plain.passed() && timer.passed() {
                println!("PHASE-1 DETERMINISM GATE: PASS ({} runs each)", parsed.runs);
            } else {
                eprintln!("PHASE-1 DETERMINISM GATE: FAIL");
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("dh-cli gate: {e}");
            std::process::exit(1);
        }
    }
}

pub fn parse_boot_args(args: &[String]) -> Result<BootArgs, String> {
    let mut path = None;
    let mut mem_mib = None;
    let mut cmdline = String::new();
    let mut json = false;
    let mut linux = false;
    let mut linux_args = LinuxArgsBuilder::default();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--linux" => linux = true,
            "--mem-mib" => {
                mem_mib = Some(parse_u64(&value(args, &mut i, "--mem-mib")?, "--mem-mib")?)
            }
            "--cmdline" => cmdline = value(args, &mut i, "--cmdline")?,
            "--cmdline-extra" => {
                linux_args.seen_linux_arg = true;
                linux_args
                    .cmdline_extra
                    .push(value(args, &mut i, "--cmdline-extra")?);
            }
            "--bzimage" => {
                linux_args.seen_linux_arg = true;
                LinuxArgsBuilder::path_arg(
                    &mut linux_args.bzimage,
                    "--bzimage",
                    value(args, &mut i, "--bzimage")?,
                )?;
            }
            "--initramfs" => {
                linux_args.seen_linux_arg = true;
                LinuxArgsBuilder::path_arg(
                    &mut linux_args.initramfs,
                    "--initramfs",
                    value(args, &mut i, "--initramfs")?,
                )?;
            }
            "--base-image" => {
                linux_args.seen_linux_arg = true;
                LinuxArgsBuilder::path_arg(
                    &mut linux_args.base_image,
                    "--base-image",
                    value(args, &mut i, "--base-image")?,
                )?;
            }
            "--game-image" => {
                linux_args.seen_linux_arg = true;
                LinuxArgsBuilder::path_arg(
                    &mut linux_args.game_image,
                    "--game-image",
                    value(args, &mut i, "--game-image")?,
                )?;
            }
            "--json" => json = true,
            p if path.is_none() && !p.starts_with("--") => path = Some(p.to_string()),
            other => return Err(format!("unexpected argument {other}")),
        }
        i += 1;
    }

    if linux {
        if path.is_some() {
            return Err("--linux boot does not take a guest ELF path".into());
        }
        if !cmdline.is_empty() {
            return Err("--cmdline is only valid for ELF boot; use --cmdline-extra".into());
        }
        let mem_bytes = mem_mib.unwrap_or(crate::linux::DEFAULT_LINUX_MEM_BYTES >> 20) << 20;
        return Ok(BootArgs {
            mode: BootMode::Linux(linux_args.finish(mem_bytes)?),
            json,
        });
    }
    if linux_args.seen() {
        return Err("Linux artifact flags require --linux".into());
    }
    Ok(BootArgs {
        mode: BootMode::Elf {
            path: path.ok_or("missing guest ELF path")?,
            mem_bytes: mem_mib.unwrap_or(16) << 20,
            cmdline,
        },
        json,
    })
}

pub fn parse_run_args(args: &[String]) -> Result<RunArgs, String> {
    let mut path = None;
    let mut mem_mib = None;
    let mut cmdline = String::new();
    let mut icount_budget = None;
    let mut vns_budget = None;
    let mut paranoid_hash = false;
    let mut linux = false;
    let mut linux_args = LinuxArgsBuilder::default();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--linux" => linux = true,
            "--mem-mib" => {
                mem_mib = Some(parse_u64(&value(args, &mut i, "--mem-mib")?, "--mem-mib")?)
            }
            "--cmdline" => cmdline = value(args, &mut i, "--cmdline")?,
            "--cmdline-extra" => {
                linux_args.seen_linux_arg = true;
                linux_args
                    .cmdline_extra
                    .push(value(args, &mut i, "--cmdline-extra")?);
            }
            "--bzimage" => {
                linux_args.seen_linux_arg = true;
                LinuxArgsBuilder::path_arg(
                    &mut linux_args.bzimage,
                    "--bzimage",
                    value(args, &mut i, "--bzimage")?,
                )?;
            }
            "--initramfs" => {
                linux_args.seen_linux_arg = true;
                LinuxArgsBuilder::path_arg(
                    &mut linux_args.initramfs,
                    "--initramfs",
                    value(args, &mut i, "--initramfs")?,
                )?;
            }
            "--base-image" => {
                linux_args.seen_linux_arg = true;
                LinuxArgsBuilder::path_arg(
                    &mut linux_args.base_image,
                    "--base-image",
                    value(args, &mut i, "--base-image")?,
                )?;
            }
            "--game-image" => {
                linux_args.seen_linux_arg = true;
                LinuxArgsBuilder::path_arg(
                    &mut linux_args.game_image,
                    "--game-image",
                    value(args, &mut i, "--game-image")?,
                )?;
            }
            "--icount-budget" => {
                let n = parse_u64(&value(args, &mut i, "--icount-budget")?, "--icount-budget")?;
                set_once(&mut icount_budget, n, "--icount-budget")?;
            }
            "--vns-budget" => {
                let n = parse_u64(&value(args, &mut i, "--vns-budget")?, "--vns-budget")?;
                set_once(&mut vns_budget, n, "--vns-budget")?;
            }
            "--paranoid-hash" => paranoid_hash = true,
            p if path.is_none() && !p.starts_with("--") => path = Some(p.to_string()),
            other => return Err(format!("unexpected argument {other}")),
        }
        i += 1;
    }

    if linux {
        if path.is_some() {
            return Err("--linux run does not take a guest ELF path".into());
        }
        if !cmdline.is_empty() {
            return Err("--cmdline is only valid for ELF run; use --cmdline-extra".into());
        }
        if vns_budget.is_some() {
            return Err("--vns-budget is not supported for Linux READY runs".into());
        }
        let mem_bytes = mem_mib.unwrap_or(crate::linux::DEFAULT_LINUX_MEM_BYTES >> 20) << 20;
        return Ok(RunArgs {
            mode: RunMode::Linux {
                paths: linux_args.finish(mem_bytes)?,
                hard_cap: icount_budget.unwrap_or(crate::linux::DEFAULT_READY_HARD_CAP),
            },
            paranoid_hash,
        });
    }
    if linux_args.seen() {
        return Err("Linux artifact flags require --linux".into());
    }
    Ok(RunArgs {
        mode: RunMode::Elf {
            path: path.ok_or("missing guest ELF path")?,
            mem_bytes: mem_mib.unwrap_or(16) << 20,
            cmdline,
            until: parse_elf_until(icount_budget, vns_budget)?,
        },
        paranoid_hash,
    })
}

pub fn parse_gate_args(args: &[String]) -> Result<GateArgs, String> {
    let mut runs = 100usize;
    let mut linux = false;
    let mut mem_mib = None;
    let mut linux_args = LinuxArgsBuilder::default();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--runs" => {
                runs = value(args, &mut i, "--runs")?
                    .parse()
                    .map_err(|e| format!("--runs: {e}"))?
            }
            "--linux" => linux = true,
            "--mem-mib" => {
                mem_mib = Some(parse_u64(&value(args, &mut i, "--mem-mib")?, "--mem-mib")?)
            }
            "--cmdline-extra" => {
                linux_args.seen_linux_arg = true;
                linux_args
                    .cmdline_extra
                    .push(value(args, &mut i, "--cmdline-extra")?);
            }
            "--bzimage" => {
                linux_args.seen_linux_arg = true;
                LinuxArgsBuilder::path_arg(
                    &mut linux_args.bzimage,
                    "--bzimage",
                    value(args, &mut i, "--bzimage")?,
                )?;
            }
            "--initramfs" => {
                linux_args.seen_linux_arg = true;
                LinuxArgsBuilder::path_arg(
                    &mut linux_args.initramfs,
                    "--initramfs",
                    value(args, &mut i, "--initramfs")?,
                )?;
            }
            "--base-image" => {
                linux_args.seen_linux_arg = true;
                LinuxArgsBuilder::path_arg(
                    &mut linux_args.base_image,
                    "--base-image",
                    value(args, &mut i, "--base-image")?,
                )?;
            }
            "--game-image" => {
                linux_args.seen_linux_arg = true;
                LinuxArgsBuilder::path_arg(
                    &mut linux_args.game_image,
                    "--game-image",
                    value(args, &mut i, "--game-image")?,
                )?;
            }
            other => return Err(format!("unexpected argument {other}")),
        }
        i += 1;
    }
    if linux {
        let mem_bytes = mem_mib.unwrap_or(crate::linux::DEFAULT_LINUX_MEM_BYTES >> 20) << 20;
        return Ok(GateArgs {
            runs,
            linux: Some(linux_args.finish(mem_bytes)?),
        });
    }
    if linux_args.seen() || mem_mib.is_some() {
        return Err("Linux gate flags require --linux".into());
    }
    Ok(GateArgs { runs, linux: None })
}

fn value(args: &[String], i: &mut usize, flag: &str) -> Result<String, String> {
    *i += 1;
    args.get(*i)
        .filter(|value| !value.starts_with("--"))
        .cloned()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_u64(value: &str, flag: &str) -> Result<u64, String> {
    value.parse().map_err(|e| format!("{flag}: {e}"))
}

fn set_once(slot: &mut Option<u64>, value: u64, flag: &str) -> Result<(), String> {
    if slot.replace(value).is_some() {
        return Err(format!("{flag} supplied more than once"));
    }
    Ok(())
}

fn parse_elf_until(
    icount_budget: Option<u64>,
    vns_budget: Option<u64>,
) -> Result<dh_vmm::runctl::Until, String> {
    match (icount_budget, vns_budget) {
        (Some(_), Some(_)) => Err("choose only one run budget".into()),
        (Some(n), None) => Ok(dh_vmm::runctl::Until::IcountBudget(n)),
        (None, Some(n)) => Ok(dh_vmm::runctl::Until::VnsBudget(n)),
        (None, None) => Err("missing --icount-budget or --vns-budget".into()),
    }
}

fn linux_ready_json(r: &crate::linux::LinuxReadyReport) -> String {
    format!(
        "{{\"reason\":\"{}\",\"event_kind\":{},\"ready_payload_len\":{},\"ready_unit\":{},\"ready_region_count\":{},\"ready_manifest_generation\":{},\"ready_payload_digest\":\"{}\",\"icount\":{},\"vns\":{},\"state_hash\":\"{}\",\"config_hash\":\"{}\",\"game_image_hash\":\"{}\",\"base_image_hash\":\"{}\"}}",
        r.reason,
        r.ready_event_kind,
        r.ready_payload_len,
        r.ready_unit,
        r.ready_region_count,
        r.ready_manifest_generation,
        r.ready_payload_digest,
        r.icount,
        r.vns,
        r.state_hash,
        r.config_hash,
        r.game_image_hash,
        r.base_image_hash
    )
}
