#![cfg(target_arch = "x86_64")]

use std::path::PathBuf;

use dh_cli::cli::{
    parse_boot_args, parse_gate_args, parse_run_args, BootArgs, BootMode, GateArgs, RunArgs,
    RunMode,
};
use dh_cli::linux::{LinuxGuestPaths, DEFAULT_LINUX_MEM_BYTES, DEFAULT_READY_HARD_CAP};

fn s(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_string()).collect()
}

fn linux_args() -> Vec<String> {
    s(&[
        "--linux",
        "--bzimage",
        "/m9/bzImage",
        "--initramfs",
        "/m9/initramfs.cpio",
        "--base-image",
        "/m9/base.img",
        "--game-image",
        "/m9/game.img",
    ])
}

fn expected_linux_paths(mem_bytes: u64, extras: &[&str]) -> LinuxGuestPaths {
    LinuxGuestPaths {
        bzimage: PathBuf::from("/m9/bzImage"),
        initramfs: PathBuf::from("/m9/initramfs.cpio"),
        base_image: PathBuf::from("/m9/base.img"),
        game_image: PathBuf::from("/m9/game.img"),
        cmdline_extra: extras.iter().map(|arg| (*arg).to_string()).collect(),
        mem_bytes,
    }
}

#[test]
fn boot_parser_keeps_default_elf_path() {
    let parsed = parse_boot_args(&s(&[
        "guest.elf",
        "--mem-mib",
        "32",
        "--cmdline",
        "1000",
        "--json",
    ]))
    .unwrap();

    assert_eq!(
        parsed,
        BootArgs {
            mode: BootMode::Elf {
                path: "guest.elf".into(),
                mem_bytes: 32 << 20,
                cmdline: "1000".into(),
            },
            json: true,
        }
    );
}

#[test]
fn boot_parser_accepts_linux_artifact_paths() {
    let mut args = linux_args();
    args.extend(s(&["--cmdline-extra", "loglevel=4", "--json"]));

    assert_eq!(
        parse_boot_args(&args).unwrap(),
        BootArgs {
            mode: BootMode::Linux(expected_linux_paths(
                DEFAULT_LINUX_MEM_BYTES,
                &["loglevel=4"]
            )),
            json: true,
        }
    );
}

#[test]
fn run_parser_keeps_default_elf_budget_path() {
    let parsed = parse_run_args(&s(&[
        "guest.elf",
        "--icount-budget",
        "12345",
        "--mem-mib",
        "64",
        "--cmdline",
        "demo",
        "--paranoid-hash",
    ]))
    .unwrap();

    assert_eq!(
        parsed,
        RunArgs {
            mode: RunMode::Elf {
                path: "guest.elf".into(),
                mem_bytes: 64 << 20,
                cmdline: "demo".into(),
                until: dh_vmm::runctl::Until::IcountBudget(12345),
            },
            paranoid_hash: true,
        }
    );
}

#[test]
fn run_parser_accepts_linux_ready_hard_cap_in_any_order() {
    let mut args = s(&["--icount-budget", "777"]);
    args.extend(linux_args());
    args.extend(s(&["--cmdline-extra", "quiet", "--paranoid-hash"]));

    assert_eq!(
        parse_run_args(&args).unwrap(),
        RunArgs {
            mode: RunMode::Linux {
                paths: expected_linux_paths(DEFAULT_LINUX_MEM_BYTES, &["quiet"]),
                hard_cap: 777,
            },
            paranoid_hash: true,
        }
    );
}

#[test]
fn gate_parser_keeps_default_nanokernel_gate() {
    assert_eq!(
        parse_gate_args(&s(&["--runs", "2"])).unwrap(),
        GateArgs {
            runs: 2,
            linux: None,
        }
    );
}

#[test]
fn gate_parser_accepts_linux_artifact_paths() {
    let mut args = s(&["--runs", "2", "--mem-mib", "256"]);
    args.extend(linux_args());
    args.extend(s(&["--cmdline-extra", "loglevel=3"]));

    assert_eq!(
        parse_gate_args(&args).unwrap(),
        GateArgs {
            runs: 2,
            linux: Some(expected_linux_paths(256 << 20, &["loglevel=3"])),
        }
    );
}

#[test]
fn linux_artifact_flags_require_linux_mode_and_complete_set() {
    let err = parse_boot_args(&s(&["--bzimage", "/m9/bzImage"])).unwrap_err();
    assert!(err.contains("Linux artifact flags require --linux"));

    let err = parse_gate_args(&s(&["--linux", "--bzimage", "/m9/bzImage"])).unwrap_err();
    assert!(err.contains("missing --initramfs"));
}

#[test]
fn linux_run_defaults_to_ready_hard_cap() {
    assert_eq!(
        parse_run_args(&linux_args()).unwrap(),
        RunArgs {
            mode: RunMode::Linux {
                paths: expected_linux_paths(DEFAULT_LINUX_MEM_BYTES, &[]),
                hard_cap: DEFAULT_READY_HARD_CAP,
            },
            paranoid_hash: false,
        }
    );
}
